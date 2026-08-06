import { describe, expect, it } from "vitest";

import { compile } from "../src/hql/compiler";
import { compileLegacy } from "../src/hql/legacy";
import { parse } from "../src/hql/parser";

function wire(source: string): any {
  const statement = parse(source);
  return JSON.parse(compileLegacy(statement, compile(statement).shape));
}

function query(source: string, name = "rows"): any {
  return wire(source).query.queries.find((entry: any) => entry.Query.name === name).Query;
}

describe("Explorer-compatible dynamic query wire format", () => {
  it("uses the v1 inline read envelope", () => {
    const request = wire("SELECT COUNT(*) FROM NODES");
    expect(request).toMatchObject({
      request_type: "read",
      query_name: "helix_visualizer_select",
      query: { returns: ["rows"] },
      parameters: {},
    });
    expect(query("SELECT COUNT(*) FROM NODES").steps).toEqual([{ N: "All" }, "Count"]);
  });

  it("compiles labels, filters, ordering and paging to flat steps", () => {
    expect(query("SELECT name FROM NODES:User WHERE age >= 21 ORDER BY age DESC SKIP 2 LIMIT 5").steps).toEqual([
      { NWhere: { And: [{ Eq: ["$label", { String: "User" }] }, { Gte: ["age", { I64: 21 }] }] } },
      { OrderBy: ["age", "Desc"] },
      { Skip: 2 },
      { Limit: 5 },
      { ValueMap: null },
    ]);
  });

  it("fans graph edges out from the selected node set", () => {
    expect(query("GRAPH NODES:User VIA Follows LIMIT 10 EDGE LIMIT 20", "edges").steps).toEqual([
      { NWhere: { Eq: ["$label", { String: "User" }] } },
      "Dedup",
      { Limit: 10 },
      { BothE: "Follows" },
      "Dedup",
      { Limit: 20 },
      "EdgeProperties",
    ]);
  });

  it("derives edge labels without using unsupported E All", () => {
    expect(query("SHOW EDGE LABELS SAMPLE 100", "edgeLabels").steps).toEqual([
      { N: "All" },
      { OutE: null },
      "Dedup",
      { Limit: 100 },
      "EdgeProperties",
    ]);
  });

  it("addresses described entities by numeric id", () => {
    expect(query("DESCRIBE NODE 42", "entity").steps).toEqual([
      { N: { Ids: [42] } },
      { ValueMap: null },
    ]);
  });
});
