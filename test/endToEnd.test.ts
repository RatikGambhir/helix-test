/**
 * End-to-end coverage: HelixSQL text → compiled AST → HTTP → decoded result.
 *
 * The server here is the bundled mock (`tools/mock-helix-server.mjs`), which
 * interprets the same `/v2/query` AST HelixDB does over a fixed sample graph.
 * That makes these tests a check of this app's own pipeline — compiler, wire
 * format, and response reader together — not a conformance test against a real
 * HelixDB instance.
 */
import type { AddressInfo } from "node:net";
import type { Server } from "node:http";

import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { parseJson } from "@helix-db/helix-db";

import { compile } from "../src/hql/compiler";
import { parse } from "../src/hql/parser";
import { readResult, type QueryResult } from "../src/results";

// The mock is plain JavaScript with no type declarations; it is a dev tool.
// @ts-expect-error -- untyped .mjs helper
import { createMockServer } from "../tools/mock-helix-server.mjs";

let server: Server;
let endpoint: string;

beforeAll(async () => {
  ({ server } = createMockServer({ seed: 7 }));
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address() as AddressInfo;
  endpoint = `http://127.0.0.1:${port}/v2/query`;
});

afterAll(async () => {
  await new Promise<void>((resolve, reject) =>
    server.close((error) => (error ? reject(error) : resolve())),
  );
});

/** Runs a HelixSQL query the same way the app does and decodes the response. */
async function run(query: string): Promise<QueryResult> {
  const compiled = compile(parse(query));
  const response = await fetch(endpoint, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: compiled.request.toJsonString(),
  });
  const text = await response.text();
  if (response.status !== 200) {
    throw new Error(`HTTP ${response.status}: ${text}`);
  }
  return readResult(parseJson(text), compiled.shape);
}

const rowsOf = (result: QueryResult) => {
  if (result.kind !== "rows") throw new Error(`expected rows, got ${result.kind}`);
  return result;
};

describe("select", () => {
  it("returns every stored property for SELECT *, with identity merged in", async () => {
    const { columns, rows } = rowsOf(await run("SELECT * FROM NODES:User LIMIT 5"));
    expect(rows).toHaveLength(5);
    // Identity columns lead, then the stored properties.
    expect(columns.slice(0, 2)).toEqual(["id", "label"]);
    expect(columns).toContain("name");
    expect(rows[0].label).toBe("User");
    expect(String(rows[0].id)).toMatch(/^\d+$/);
  });

  it("projects only the requested columns", async () => {
    const { columns, rows } = rowsOf(await run("SELECT id, name FROM NODES:User LIMIT 3"));
    expect(columns).toEqual(["id", "name"]);
    expect(Object.keys(rows[0]).sort()).toEqual(["id", "name"]);
  });

  it("filters, orders and pages", async () => {
    const all = rowsOf(await run("SELECT name, age FROM NODES:User WHERE age >= 30 ORDER BY age"));
    expect(all.rows.length).toBeGreaterThan(0);
    expect(all.rows.every((row) => Number(row.age) >= 30)).toBe(true);

    const ages = all.rows.map((row) => Number(row.age));
    expect([...ages].sort((a, b) => a - b)).toEqual(ages);

    const page = rowsOf(await run("SELECT name, age FROM NODES:User WHERE age >= 30 ORDER BY age SKIP 2 LIMIT 2"));
    expect(page.rows).toEqual(all.rows.slice(2, 4));
  });

  it("orders descending", async () => {
    const { rows } = rowsOf(await run("SELECT age FROM NODES:User ORDER BY age DESC LIMIT 5"));
    const ages = rows.map((row) => Number(row.age));
    expect([...ages].sort((a, b) => b - a)).toEqual(ages);
  });

  it("applies LIKE, IN and BETWEEN", async () => {
    const like = rowsOf(await run("SELECT name FROM NODES:User WHERE name LIKE 'A%'"));
    expect(like.rows.length).toBeGreaterThan(0);
    expect(like.rows.every((row) => String(row.name).startsWith("A"))).toBe(true);

    const inList = rowsOf(await run("SELECT name, city FROM NODES:User WHERE city IN ('Berlin', 'Lagos')"));
    expect(inList.rows.every((row) => ["Berlin", "Lagos"].includes(String(row.city)))).toBe(true);

    const between = rowsOf(await run("SELECT age FROM NODES:User WHERE age BETWEEN 25 AND 30"));
    expect(between.rows.every((row) => Number(row.age) >= 25 && Number(row.age) <= 30)).toBe(true);
  });

  it("combines predicates with AND/OR/NOT", async () => {
    const { rows } = rowsOf(
      await run("SELECT name, age, city FROM NODES:User WHERE age < 25 OR (city = 'Osaka' AND verified = true)"),
    );
    expect(
      rows.every(
        (row) => Number(row.age) < 25 || (row.city === "Osaka" && row.verified === true),
      ),
    ).toBe(true);
  });

  it("reads edge endpoints through source and target", async () => {
    const { rows } = rowsOf(await run("SELECT id, label, source, target FROM EDGES:Follows LIMIT 5"));
    expect(rows).toHaveLength(5);
    for (const row of rows) {
      expect(row.label).toBe("Follows");
      expect(String(row.source)).toMatch(/^\d+$/);
      expect(String(row.target)).toMatch(/^\d+$/);
    }
  });

  it("survives an i64 that JavaScript cannot represent as a number", async () => {
    // The sample graph gives exactly one user this value; a lossy round trip
    // would come back as 9223372036854776000.
    const { rows } = rowsOf(
      await run("SELECT name, externalId FROM NODES:User WHERE externalId = 9223372036854775807"),
    );
    expect(rows).toHaveLength(1);
    expect(rows[0].externalId).toBe(9223372036854775807n);
  });
});

describe("traversal", () => {
  it("follows an outgoing edge label", async () => {
    const seed = rowsOf(await run("SELECT id, name FROM NODES:User LIMIT 1")).rows[0];
    const followed = rowsOf(
      await run(`SELECT id, label FROM NODES WHERE id = ${seed.id} TRAVERSE OUT Follows`),
    );
    for (const row of followed.rows) expect(row.label).toBe("User");
  });

  it("reaches edges and steps back onto their endpoints", async () => {
    const edges = rowsOf(
      await run("SELECT id, label FROM NODES:User TRAVERSE OUT EDGES WorksAt LIMIT 5"),
    );
    expect(edges.rows.every((row) => row.label === "WorksAt")).toBe(true);

    const orgs = rowsOf(
      await run("SELECT DISTINCT id, label FROM NODES:User TRAVERSE OUT EDGES WorksAt TRAVERSE TARGET"),
    );
    expect(orgs.rows.length).toBeGreaterThan(0);
    expect(orgs.rows.every((row) => row.label === "Org")).toBe(true);
  });

  it("chains two hops", async () => {
    const { rows } = rowsOf(
      await run("SELECT DISTINCT id FROM NODES:User TRAVERSE OUT Follows TRAVERSE OUT Likes LIMIT 20"),
    );
    expect(rows.length).toBeGreaterThan(0);
  });

  it("filters at the far end of a hop", async () => {
    const { rows } = rowsOf(
      await run("SELECT id, label, title FROM NODES:User TRAVERSE OUT Likes WHERE title = 'rust'"),
    );
    expect(rows.every((row) => row.title === "rust")).toBe(true);
  });
});

describe("aggregates and schema", () => {
  it("counts", async () => {
    const total = await run("SELECT COUNT(*) FROM NODES");
    expect(total.kind).toBe("count");
    const users = await run("SELECT COUNT(*) FROM NODES:User");
    expect(users.kind === "count" && total.kind === "count" && users.value < total.value).toBe(true);
  });

  it("groups a count by label", async () => {
    const result = await run("SELECT COUNT(*) FROM NODES GROUP BY label");
    if (result.kind !== "groupCount") throw new Error("expected a grouped count");
    const labels = result.groups.map((group) => group.label);
    expect(labels).toContain("User");
    expect(labels).toContain("Topic");
    // Sorted by descending count.
    const counts = result.groups.map((group) => group.count);
    expect([...counts].sort((a, b) => b - a)).toEqual(counts);
  });

  it("derives both label sets", async () => {
    const result = await run("SHOW LABELS");
    if (result.kind !== "labels") throw new Error("expected labels");
    expect(result.nodes?.map((entry) => entry.label).sort()).toEqual(["Org", "Post", "Topic", "User"]);
    expect(result.edges?.map((entry) => entry.label)).toContain("Follows");
  });

  it("reports instance totals", async () => {
    const result = await run("SHOW STATS");
    if (result.kind !== "stats") throw new Error("expected stats");
    expect(result.nodeCount).toBeGreaterThan(0);
    expect(result.edgeCount).toBeGreaterThan(0);
    expect(result.nodeLabels.length).toBe(4);
  });
});

describe("graph", () => {
  it("draws a connected whole-graph view", async () => {
    const result = await run("GRAPH LIMIT 400 EDGE LIMIT 2000");
    if (result.kind !== "graph") throw new Error("expected a graph");
    const { nodes, edges } = result.graph;
    expect(nodes.length).toBeGreaterThan(50);
    expect(edges.length).toBeGreaterThan(50);

    // Every drawn edge must reference nodes that are actually present.
    const ids = new Set(nodes.map((node) => node.id));
    for (const edge of edges) {
      expect(ids.has(edge.source)).toBe(true);
      expect(ids.has(edge.target)).toBe(true);
    }
  });

  it("reports edges that left the selection instead of dropping them silently", async () => {
    // Users are only some of the nodes, so their Likes/WorksAt edges point out
    // of the selection and cannot be drawn.
    const result = await run("GRAPH NODES:User");
    if (result.kind !== "graph") throw new Error("expected a graph");
    expect(result.graph.nodes.every((node) => node.label === "User")).toBe(true);
    expect(result.graph.danglingEdges).toBeGreaterThan(0);
  });

  it("restricts drawn edges with VIA", async () => {
    const result = await run("GRAPH NODES:User VIA Follows");
    if (result.kind !== "graph") throw new Error("expected a graph");
    expect(result.graph.edges.length).toBeGreaterThan(0);
    expect(result.graph.edges.every((edge) => edge.label === "Follows")).toBe(true);
  });

  it("attaches requested properties to the nodes", async () => {
    const result = await run("GRAPH NODES:User WITH name, city LIMIT 10");
    if (result.kind !== "graph") throw new Error("expected a graph");
    expect(Object.keys(result.graph.nodes[0].properties).sort()).toEqual(["city", "name"]);
  });

  it("flags a truncated view", async () => {
    const result = await run("GRAPH LIMIT 5 EDGE LIMIT 5");
    if (result.kind !== "graph") throw new Error("expected a graph");
    expect(result.graph.truncatedNodes).toBe(true);
    expect(result.graph.nodes.length).toBeLessThanOrEqual(5);
  });
});

describe("describe", () => {
  it("returns a node with its properties, edges, neighbours and degree", async () => {
    const seed = rowsOf(await run("SELECT id FROM NODES:User LIMIT 1")).rows[0];
    const result = await run(`DESCRIBE NODE ${seed.id}`);
    if (result.kind !== "describe") throw new Error("expected a description");

    expect(result.entity).toBe("nodes");
    expect(result.id).toBe(String(seed.id));
    expect(result.label).toBe("User");
    expect(result.properties.name).toBeTypeOf("string");
    expect(result.degree).toBeGreaterThan(0);
    expect(result.edges.length).toBeGreaterThan(0);
    expect(result.neighbours.length).toBeGreaterThan(0);
    // Every incident edge touches the described node.
    for (const edge of result.edges) {
      expect(edge.source === result.id || edge.target === result.id).toBe(true);
    }
  });

  it("returns an edge with both endpoints resolved", async () => {
    const seed = rowsOf(await run("SELECT id, source, target FROM EDGES:Follows LIMIT 1")).rows[0];
    const result = await run(`DESCRIBE EDGE ${seed.id}`);
    if (result.kind !== "describe") throw new Error("expected a description");

    expect(result.entity).toBe("edges");
    expect(result.label).toBe("Follows");
    expect(result.endpoints?.source?.id).toBe(String(seed.source));
    expect(result.endpoints?.target?.id).toBe(String(seed.target));
  });
});

describe("failure handling", () => {
  it("surfaces the server's message for an unsupported request", async () => {
    // `path` is not something the compiler emits, so it exercises the error
    // path end to end rather than a query the app would ever build.
    const response = await fetch(endpoint, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        request_type: "read",
        query: { read: { entries: [{ query: { name: "x", root: { path: { input: { nodes: { reference: "all" } } } } } }], returns: ["x"] } },
      }),
    });
    expect(response.status).toBe(400);
    expect(await response.text()).toContain("path");
  });
});
