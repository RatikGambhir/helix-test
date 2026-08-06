import { describe, expect, it } from "vitest";

import { HqlError } from "../src/hql/ast";
import { parse } from "../src/hql/parser";
import { tokenize } from "../src/hql/lexer";

describe("lexer", () => {
  it("resolves string escapes and strips comments", () => {
    const tokens = tokenize("SELECT 'it''s' -- trailing\n/* block */ FROM");
    expect(tokens.map((t) => t.text)).toEqual(["SELECT", "it's", "FROM", ""]);
  });

  it("keeps large integers as bigint and small ones as number", () => {
    const [small, large] = tokenize("1 9223372036854775807");
    expect(small.numeric).toBe(1);
    expect(large.numeric).toBe(9223372036854775807n);
  });

  it("reports the line and column of a failure", () => {
    try {
      tokenize("SELECT *\nFROM 'unterminated");
      expect.unreachable("expected a lexer error");
    } catch (error) {
      expect(error).toBeInstanceOf(HqlError);
      expect((error as HqlError).span).toMatchObject({ line: 2, column: 6 });
    }
  });

  it("treats a quoted identifier as a plain word", () => {
    const [token] = tokenize('"select"');
    expect(token).toMatchObject({ kind: "word", text: "select", quoted: true });
  });
});

describe("parser", () => {
  it("parses a bare source", () => {
    expect(parse("SELECT * FROM NODES")).toMatchObject({
      kind: "select",
      projection: { kind: "star" },
      selection: { source: { entity: "nodes", label: null }, limit: null },
    });
  });

  it("accepts both label spellings", () => {
    const colon = parse("SELECT * FROM NODES:User") as any;
    const space = parse("SELECT * FROM NODES User") as any;
    expect(colon.selection.source.label).toBe("User");
    expect(space.selection.source.label).toBe("User");
  });

  it("is case-insensitive for keywords but not for labels", () => {
    const statement = parse("select * from nodes:User limit 5") as any;
    expect(statement.selection.source.label).toBe("User");
    expect(statement.selection.limit).toBe(5);
  });

  it("resolves virtual columns and leaves properties alone", () => {
    const statement = parse("SELECT id, label, source, target, name FROM EDGES") as any;
    expect(statement.projection.columns.map((c: any) => c.source)).toEqual([
      "$id",
      "$label",
      "$from.$id",
      "$to.$id",
      "name",
    ]);
  });

  it("lets a quoted column shadow a virtual one", () => {
    const statement = parse('SELECT "id" FROM NODES') as any;
    expect(statement.projection.columns[0]).toMatchObject({ source: "id", virtual: false });
  });

  it("accepts a reserved field written out in full", () => {
    const statement = parse("SELECT $from.$id FROM EDGES") as any;
    expect(statement.projection.columns[0]).toMatchObject({ source: "$from.$id", virtual: true });
  });

  it("parses each traversal direction", () => {
    const directions = (source: string) => (parse(source) as any).selection.hops.map((h: any) => h.direction);
    expect(directions("SELECT id FROM NODES TRAVERSE OUT TRAVERSE IN TRAVERSE BOTH")).toEqual([
      "out",
      "in",
      "both",
    ]);
    expect(
      directions("SELECT id FROM NODES TRAVERSE OUT EDGES TRAVERSE SOURCE TRAVERSE BOTH EDGES TRAVERSE OTHER"),
    ).toEqual(["oute", "fromnode", "bothe", "othernode"]);
  });

  it("attaches a per-hop WHERE to that hop only", () => {
    const statement = parse(
      "SELECT id FROM NODES:User WHERE a = 1 TRAVERSE OUT Follows WHERE b = 2",
    ) as any;
    expect(statement.selection.where).toMatchObject({ kind: "compare" });
    expect(statement.selection.hops[0].where).toMatchObject({ kind: "compare" });
  });

  it("parses the GRAPH statement and its clauses", () => {
    expect(parse("GRAPH")).toMatchObject({
      kind: "graph",
      selection: { source: { entity: "nodes", label: null } },
    });
    expect(parse("GRAPH NODES:User VIA Follows WITH name, age LIMIT 50 EDGE LIMIT 200")).toMatchObject({
      kind: "graph",
      edgeLabel: "Follows",
      withProperties: ["name", "age"],
      maxEdges: 200,
      selection: { limit: 50 },
    });
  });

  it("parses SHOW and DESCRIBE", () => {
    expect(parse("SHOW LABELS")).toMatchObject({ kind: "show", target: "labels", sample: 5000 });
    expect(parse("SHOW NODE LABELS SAMPLE 10")).toMatchObject({ target: "nodeLabels", sample: 10 });
    expect(parse("SHOW STATS")).toMatchObject({ target: "stats" });
    expect(parse("DESCRIBE NODE 42")).toMatchObject({ kind: "describe", entity: "nodes", id: "42" });
    expect(parse("DESCRIBE EDGE 7 LIMIT 10")).toMatchObject({ entity: "edges", id: "7", limit: 10 });
  });

  it("tolerates a trailing semicolon", () => {
    expect(() => parse("SELECT * FROM NODES;")).not.toThrow();
  });
});

describe("parse errors", () => {
  const failure = (source: string): HqlError => {
    try {
      parse(source);
    } catch (error) {
      if (error instanceof HqlError) return error;
      throw error;
    }
    throw new Error(`expected ${JSON.stringify(source)} to fail`);
  };

  it("points at the offending token", () => {
    const error = failure("SELECT * FROM PEOPLE");
    expect(error.message).toMatch(/expected NODES or EDGES/);
    expect(error.span).toMatchObject({ line: 1, column: 15 });
  });

  it("suggests a fix for an empty query", () => {
    expect(failure("").hint).toMatch(/SELECT \* FROM NODES/);
  });

  it("explains that writes are not supported", () => {
    expect(failure("INSERT INTO NODES VALUES (1)").hint).toMatch(/read-only/);
  });

  it("points at from/to in a select list", () => {
    expect(failure("SELECT from FROM EDGES").hint).toMatch(/source \/ target/);
  });

  it("rejects a bare keyword used as a column", () => {
    expect(failure("SELECT limit FROM NODES").hint).toMatch(/quote it/);
  });

  it("rejects more than one statement", () => {
    expect(failure("SELECT * FROM NODES; SELECT * FROM EDGES").message).toMatch(/trailing input/);
  });

  it("rejects a negative or fractional limit", () => {
    expect(failure("SELECT * FROM NODES LIMIT -1").message).toMatch(/cannot be negative/);
    expect(failure("SELECT * FROM NODES LIMIT 1.5").message).toMatch(/whole number/);
  });

  it("rejects GROUP BY without COUNT(*)", () => {
    expect(failure("SELECT name FROM NODES GROUP BY label").message).toMatch(/only supported with COUNT/);
  });

  it("rejects a GRAPH that ends on edges", () => {
    expect(failure("GRAPH NODES:User TRAVERSE OUT EDGES").message).toMatch(/must land on nodes/);
  });

  it("rejects an unterminated string with a usable hint", () => {
    expect(failure("SELECT * FROM NODES WHERE name = 'alice").hint).toMatch(/closing '/);
  });
});
