import { describe, expect, it } from "vitest";

import { HqlError } from "../src/hql/ast";
import { compile, DEFAULT_GRAPH_EDGE_LIMIT, DEFAULT_ROW_LIMIT } from "../src/hql/compiler";
import { parse } from "../src/hql/parser";

/** Compiles a statement and returns the wire JSON as a plain object. */
function wire(source: string): any {
  return JSON.parse(compile(parse(source)).json);
}

/** The compiled request as text, with i64 values still exact. */
function wireText(source: string): string {
  return compile(parse(source)).json;
}

/** The root traversal AST of one named variable in the compiled read batch. */
function root(source: string, variable = "rows"): any {
  const entries = wire(source).query.read.entries;
  const entry = entries.find((e: any) => e.query?.name === variable);
  if (!entry) throw new Error(`no variable ${variable} in ${JSON.stringify(entries)}`);
  return entry.query.root;
}

/** Collects every `{ step: … }` key along the nested `input` chain, outermost first. */
function stepChain(node: any): string[] {
  const steps: string[] = [];
  let current = node;
  while (current && typeof current === "object") {
    const key = Object.keys(current)[0];
    steps.push(key);
    const payload = current[key];
    current = payload && typeof payload === "object" ? payload.input : undefined;
  }
  return steps;
}

/** Every `limit` count along the nested `input` chain, outermost first. */
function limitChain(node: any): number[] {
  const limits: number[] = [];
  let current = node;
  while (current && typeof current === "object") {
    const key = Object.keys(current)[0];
    const payload = current[key];
    if (key === "limit") limits.push(Number(payload.count.literal));
    current = payload && typeof payload === "object" ? payload.input : undefined;
  }
  return limits;
}

describe("select", () => {
  it("reads every node with a default limit", () => {
    const compiled = compile(parse("SELECT * FROM NODES"));
    expect(compiled.shape).toMatchObject({ kind: "rows", identityVariable: "identity" });

    const steps = stepChain(root("SELECT * FROM NODES"));
    expect(steps).toEqual(["value_map", "limit", "nodes"]);
    expect(root("SELECT * FROM NODES").value_map.input.limit.count).toEqual({
      literal: DEFAULT_ROW_LIMIT,
    });
  });

  it("selects by label at the source instead of filtering afterwards", () => {
    const withColon = root("SELECT id FROM NODES:User");
    const withWhere = root("SELECT id FROM NODES WHERE label = 'User'");
    // The two spellings are the same query.
    expect(withColon).toEqual(withWhere);
    // The label lands on the source step (`nodes_where`), not on a trailing
    // `where`, which is what lets HelixDB use its label index.
    expect(withColon.project.input.limit.input).toMatchObject({
      nodes_where: {
        predicate: { eq: { left: { property: "$label" }, right: { constant: { string: "User" } } } },
      },
    });
  });

  it("keeps non-label predicates alongside a label filter", () => {
    const inner = root("SELECT id FROM NODES WHERE label = 'User' AND age > 21").project.input.limit
      .input;
    // Label at the source, everything else in a `where` wrapped around it.
    expect(inner.where.input.nodes_where).toBeDefined();
    expect(inner.where.predicate).toMatchObject({
      gt: { left: { property: "age" }, right: { constant: { i64: 21 } } },
    });
  });

  it("projects named columns with their written names as aliases", () => {
    const projections = root("SELECT id, label, name FROM NODES:User").project.projections;
    expect(projections).toEqual([
      { property: { source: "$id", alias: "id" } },
      { property: { source: "$label", alias: "label" } },
      { property: { source: "name", alias: "name" } },
    ]);
  });

  it("maps source and target onto the edge endpoint fields", () => {
    const projections = root("SELECT source, target FROM EDGES").project.projections;
    expect(projections).toEqual([
      { property: { source: "$from.$id", alias: "source" } },
      { property: { source: "$to.$id", alias: "target" } },
    ]);
  });

  it("fetches identity alongside the properties for SELECT *", () => {
    const identity = root("SELECT * FROM EDGES", "identity");
    expect(identity.value_map.properties).toEqual(["$id", "$label", "$from.$id", "$to.$id"]);
  });

  it("compiles ordering, paging and DISTINCT", () => {
    const steps = stepChain(
      root("SELECT DISTINCT name FROM NODES:User ORDER BY age DESC SKIP 10 LIMIT 5"),
    );
    expect(steps).toEqual(["project", "limit", "skip", "order_by", "dedup", "nodes_where"]);
  });

  it("accepts SKIP and LIMIT in either order", () => {
    expect(stepChain(root("SELECT id FROM NODES LIMIT 5 SKIP 2"))).toEqual(
      stepChain(root("SELECT id FROM NODES SKIP 2 LIMIT 5")),
    );
  });

  it("compiles COUNT(*) to a count terminal with no default limit", () => {
    const steps = stepChain(root("SELECT COUNT(*) FROM NODES:User"));
    expect(steps).toEqual(["count", "nodes_where"]);
    expect(compile(parse("SELECT COUNT(*) FROM NODES")).shape).toEqual({
      kind: "count",
      variable: "rows",
    });
  });

  it("compiles GROUP BY to a group_count over the grouping source", () => {
    const compiled = compile(parse("SELECT COUNT(*) FROM NODES GROUP BY label"));
    expect(compiled.shape).toEqual({ kind: "groupCount", variable: "rows", by: "label" });
    expect(root("SELECT COUNT(*) FROM NODES GROUP BY label").group_count).toMatchObject({
      property: "$label",
    });
  });
});

describe("where clauses", () => {
  const predicate = (where: string) =>
    root(`SELECT id FROM NODES WHERE ${where}`).project.input.limit.input.where.predicate;

  it("compiles the comparison operators", () => {
    expect(predicate("age > 21")).toMatchObject({
      gt: { left: { property: "age" }, right: { constant: { i64: 21 } } },
    });
    expect(Object.keys(predicate("age <> 21"))).toEqual(["neq"]);
    expect(Object.keys(predicate("age != 21"))).toEqual(["neq"]);
    expect(Object.keys(predicate("age <= 21"))).toEqual(["lte"]);
    // A non-integer literal keeps its float type rather than being truncated.
    expect(predicate("score > 1.5")).toMatchObject({ gt: { right: { constant: { f64: 1.5 } } } });
  });

  it("compiles boolean structure with correct precedence", () => {
    // AND binds tighter than OR, so this is or(a, and(b, c)).
    const p = predicate("a = 1 OR b = 2 AND c = 3");
    expect(Object.keys(p)).toEqual(["or"]);
    expect(Object.keys(p.or.predicates[0])).toEqual(["eq"]);
    expect(Object.keys(p.or.predicates[1])).toEqual(["and"]);
    expect(p.or.predicates[1].and.predicates).toHaveLength(2);
  });

  it("lets parentheses override precedence", () => {
    const p = predicate("(a = 1 OR b = 2) AND c = 3");
    expect(Object.keys(p)).toEqual(["and"]);
    expect(Object.keys(p.and.predicates[0])).toEqual(["or"]);
  });

  it("maps LIKE patterns onto the matching string predicate", () => {
    expect(Object.keys(predicate("name LIKE 'ali%'"))).toEqual(["starts_with"]);
    expect(Object.keys(predicate("name LIKE '%son'"))).toEqual(["ends_with"]);
    expect(Object.keys(predicate("name LIKE '%li%'"))).toEqual(["contains"]);
    // A pattern with no wildcard is plain equality, as in SQL.
    expect(Object.keys(predicate("name LIKE 'alice'"))).toEqual(["eq"]);
    expect(Object.keys(predicate("name NOT LIKE 'ali%'"))).toEqual(["not"]);
  });

  it("compiles IS NULL, HAS, IN and BETWEEN", () => {
    expect(Object.keys(predicate("bio IS NULL"))).toEqual(["is_null"]);
    expect(Object.keys(predicate("bio IS NOT NULL"))).toEqual(["is_not_null"]);
    expect(Object.keys(predicate("HAS(bio)"))).toEqual(["has_key"]);
    expect(Object.keys(predicate("age IN (1, 2, 3)"))).toEqual(["is_in"]);
    expect(Object.keys(predicate("age NOT IN (1, 2)"))).toEqual(["not"]);
    expect(Object.keys(predicate("age BETWEEN 18 AND 30"))).toEqual(["between"]);
  });

  it("treats = NULL as IS NULL rather than comparing against a null value", () => {
    expect(Object.keys(predicate("bio = NULL"))).toEqual(["is_null"]);
    expect(Object.keys(predicate("bio != NULL"))).toEqual(["is_not_null"]);
  });

  it("keeps integers beyond the JavaScript safe range exact", () => {
    // 2^63-1 is a valid i64 but not a representable double, so the value must
    // never pass through a plain JSON.parse on its way to the wire.
    expect(wireText("SELECT id FROM NODES WHERE externalId = 9223372036854775807")).toContain(
      "9223372036854775807",
    );
  });

  it("rejects a LIKE wildcard in the middle of a pattern", () => {
    expect(() => compile(parse("SELECT id FROM NODES WHERE name LIKE 'a%e%'"))).toThrow(HqlError);
  });
});

describe("traversals", () => {
  it("chains hops in order", () => {
    const steps = stepChain(root("SELECT id FROM NODES:User TRAVERSE OUT Follows TRAVERSE IN Likes"));
    // The chain reads outermost-first, so the hops are reversed here.
    expect(steps).toEqual(["project", "limit", "in", "out", "nodes_where"]);
  });

  it("carries the edge label onto the hop step", () => {
    const hop = root("SELECT id FROM NODES:User TRAVERSE OUT Follows").project.input.limit.input;
    expect(hop.out.label).toBe("Follows");
    // An unlabelled hop follows every edge type and omits the label entirely.
    expect(
      root("SELECT id FROM NODES:User TRAVERSE OUT").project.input.limit.input.out,
    ).not.toHaveProperty("label");
  });

  it("stops on edges with the EDGES form", () => {
    const steps = stepChain(root("SELECT id FROM NODES:User TRAVERSE OUT EDGES Follows"));
    expect(steps).toEqual(["project", "limit", "out_e", "nodes_where"]);
  });

  it("steps from an edge back onto its endpoints", () => {
    expect(stepChain(root("SELECT id FROM EDGES:Follows TRAVERSE TARGET"))).toEqual([
      "project",
      "limit",
      "in_n",
      "edges_where",
    ]);
    expect(stepChain(root("SELECT id FROM EDGES:Follows TRAVERSE SOURCE"))).toEqual([
      "project",
      "limit",
      "out_n",
      "edges_where",
    ]);
  });

  it("filters a hop with its own WHERE", () => {
    const json = JSON.stringify(root("SELECT id FROM NODES:User TRAVERSE OUT Follows WHERE age > 30"));
    expect(json).toContain('"gt"');
  });

  it("rejects a node hop taken from an edge position", () => {
    expect(() => compile(parse("SELECT id FROM EDGES TRAVERSE OUT"))).toThrow(
      /starts from a node, but the query is on edges/,
    );
  });

  it("rejects an endpoint hop taken from a node position", () => {
    expect(() => compile(parse("SELECT id FROM NODES TRAVERSE TARGET"))).toThrow(
      /starts from an edge, but the query is on nodes/,
    );
  });
});

describe("graph", () => {
  it("walks out from the selected nodes even for an unfiltered whole-graph request", () => {
    // The node set is capped independently of the edge scan, so reading the
    // edge table directly would return a slice unrelated to the nodes drawn and
    // almost every edge would be discarded as dangling.
    const compiled = compile(parse("GRAPH"));
    expect(compiled.shape).toMatchObject({ kind: "graph", nodeVariable: "nodes" });
    expect(stepChain(root("GRAPH", "edges"))).toEqual([
      "project",
      "limit",
      "dedup",
      "both_e",
      "limit",
      "dedup",
      "nodes",
    ]);
  });

  it("caps the node scan behind the edge fan-out at the node limit", () => {
    // Outermost limit is the edge cap, innermost the node cap; swapping them
    // would silently change how much of the graph is reachable.
    expect(limitChain(root("GRAPH LIMIT 25", "edges"))).toEqual([DEFAULT_GRAPH_EDGE_LIMIT, 25]);
    expect(limitChain(root("GRAPH LIMIT 25 EDGE LIMIT 90", "edges"))).toEqual([90, 25]);
  });

  it("walks out from the selected nodes once the selection is filtered", () => {
    const steps = stepChain(root("GRAPH NODES:User", "edges"));
    expect(steps).toEqual(["project", "limit", "dedup", "both_e", "limit", "dedup", "nodes_where"]);
  });

  it("projects the endpoints needed to draw an edge", () => {
    const projections = root("GRAPH", "edges").project.projections;
    expect(projections).toEqual([
      { property: { source: "$id", alias: "_id" } },
      { property: { source: "$label", alias: "_label" } },
      { property: { source: "$from.$id", alias: "_src" } },
      { property: { source: "$to.$id", alias: "_dst" } },
    ]);
  });

  it("adds requested properties to the node projection", () => {
    const projections = root("GRAPH NODES:User WITH name, age", "nodes").project.projections;
    expect(projections.map((p: any) => p.property.alias)).toEqual(["_id", "_label", "name", "age"]);
  });

  it("restricts the drawn edges with VIA", () => {
    const edges = root("GRAPH NODES:User VIA Follows", "edges");
    expect(edges.project.input.limit.input.dedup.input.both_e.label).toBe("Follows");
  });

  it("honours LIMIT and EDGE LIMIT", () => {
    const compiled = compile(parse("GRAPH NODES:User LIMIT 40 EDGE LIMIT 90"));
    expect(compiled.shape).toMatchObject({ nodeLimit: 40, edgeLimit: 90 });
  });
});

describe("show and describe", () => {
  it("derives labels from a bounded sample", () => {
    const compiled = compile(parse("SHOW LABELS SAMPLE 100"));
    expect(compiled.shape).toEqual({
      kind: "labels",
      variables: { nodes: "nodeLabels", edges: "edgeLabels" },
    });
    expect(stepChain(root("SHOW LABELS SAMPLE 100", "nodeLabels"))).toEqual([
      "group_count",
      "limit",
      "nodes",
    ]);
  });

  it("asks only for the side that was requested", () => {
    expect(compile(parse("SHOW EDGE LABELS")).shape).toEqual({
      kind: "labels",
      variables: { edges: "edgeLabels" },
    });
  });

  it("returns counts and label breakdowns for SHOW STATS", () => {
    const shape = compile(parse("SHOW STATS")).shape;
    expect(shape.kind).toBe("stats");
  });

  it("describes a node with its edges, neighbours and degree", () => {
    const compiled = compile(parse("DESCRIBE NODE 42"));
    expect(compiled.shape).toMatchObject({ kind: "describe", entity: "nodes" });
    const json = JSON.stringify(wire("DESCRIBE NODE 42"));
    expect(json).toContain('"ids":[42]');
    expect(json).toContain("both_e");
    expect(json).toContain("count");
  });

  it("describes an edge with both endpoints", () => {
    const json = JSON.stringify(wire("DESCRIBE EDGE 7"));
    expect(json).toContain("out_n");
    expect(json).toContain("in_n");
  });

  it("keeps ids beyond the safe integer range exact", () => {
    expect(wireText("DESCRIBE NODE 9223372036854775807")).toContain("9223372036854775807");
  });
});

describe("request envelope", () => {
  it("always produces a read request", () => {
    for (const source of ["SELECT * FROM NODES", "GRAPH", "SHOW LABELS", "DESCRIBE NODE 1"]) {
      expect(wire(source).request_type).toBe("read");
    }
  });

  it("names the query for server-side logs", () => {
    expect(wire("SELECT * FROM NODES").query_name).toBe("hql_select_star");
  });
});
