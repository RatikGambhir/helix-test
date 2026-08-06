import { describe, expect, it } from "vitest";

import { LabelPalette, MAX_COLOURED_LABELS, OTHER_LABEL } from "../src/graph/palette";
import { compile } from "../src/hql/compiler";
import { parse } from "../src/hql/parser";
import { readResult, ResultError, stringify } from "../src/results";

const shapeOf = (query: string) => compile(parse(query)).shape;

describe("response envelopes", () => {
  const shape = shapeOf("SELECT id FROM NODES");

  it("reads the named variable", () => {
    const result = readResult({ rows: [{ id: 1 }] }, shape);
    expect(result).toEqual({ kind: "rows", columns: ["id"], rows: [{ id: 1 }] });
  });

  it("accepts a bare array from a single-variable batch", () => {
    expect(readResult([{ id: 1 }], shape)).toMatchObject({ rows: [{ id: 1 }] });
  });

  it("looks inside a data wrapper", () => {
    expect(readResult({ data: { rows: [{ id: 2 }] } }, shape)).toMatchObject({ rows: [{ id: 2 }] });
  });

  it("falls back to the only key present", () => {
    expect(readResult({ somethingElse: [{ id: 3 }] }, shape)).toMatchObject({ rows: [{ id: 3 }] });
  });

  it("reports a body it cannot read", () => {
    expect(() => readResult({ a: [], b: [] }, shape)).toThrow(ResultError);
  });

  it("treats a missing variable as no rows rather than an error", () => {
    expect(readResult({ rows: null }, shape)).toMatchObject({ rows: [] });
  });
});

describe("counts", () => {
  it("unwraps a scalar, a one-element array and an object", () => {
    const shape = shapeOf("SELECT COUNT(*) FROM NODES");
    expect(readResult({ rows: 7 }, shape)).toEqual({ kind: "count", value: 7 });
    expect(readResult({ rows: [7] }, shape)).toEqual({ kind: "count", value: 7 });
    expect(readResult({ rows: { count: 7 } }, shape)).toEqual({ kind: "count", value: 7 });
    expect(readResult({ rows: 7n }, shape)).toEqual({ kind: "count", value: 7 });
  });

  it("reads a grouped count in map or tabular form, sorted by size", () => {
    const shape = shapeOf("SELECT COUNT(*) FROM NODES GROUP BY label");
    const expected = [
      { label: "Post", count: 9 },
      { label: "User", count: 4 },
    ];
    expect(readResult({ rows: { User: 4, Post: 9 } }, shape)).toMatchObject({ groups: expected });
    expect(
      readResult({ rows: [{ key: "User", count: 4 }, { key: "Post", count: 9 }] }, shape),
    ).toMatchObject({ groups: expected });
  });
});

describe("select star", () => {
  const shape = shapeOf("SELECT * FROM EDGES");

  it("merges identity into the property rows and renames the endpoints", () => {
    const result = readResult(
      {
        rows: [{ since: 2021 }],
        identity: [{ $id: 5n, $label: "Follows", "$from.$id": 1n, "$to.$id": 2n }],
      },
      shape,
    );
    expect(result).toMatchObject({
      columns: ["id", "label", "source", "target", "since"],
      rows: [{ id: 5n, label: "Follows", source: 1n, target: 2n, since: 2021 }],
    });
  });

  it("does not let a stored property shadow the entity's identity", () => {
    // A node may store its own `id`/`label`; the row is keyed by the entity's,
    // and clicking it sends that id to DESCRIBE.
    const result = readResult(
      {
        rows: [{ id: "legacy-7", label: "user-supplied", since: 2021 }],
        identity: [{ $id: 5n, $label: "Follows", "$from.$id": 1n, "$to.$id": 2n }],
      },
      shape,
    );
    expect(result).toMatchObject({ rows: [{ id: 5n, label: "Follows", since: 2021 }] });
  });

  it("shows the properties alone rather than misaligning them", () => {
    // A length mismatch means the two passes cannot be zipped safely.
    const result = readResult({ rows: [{ a: 1 }, { a: 2 }], identity: [{ $id: 1n }] }, shape);
    expect(result).toMatchObject({ columns: ["a"], rows: [{ a: 1 }, { a: 2 }] });
  });
});

describe("graph decoding", () => {
  const shape = shapeOf("GRAPH LIMIT 10 EDGE LIMIT 10");

  const body = {
    nodes: [
      { _id: 1, _label: "User", name: "Alice" },
      { _id: 2, _label: "User", name: "Bob" },
    ],
    edges: [
      { _id: 10, _label: "Follows", _src: 1, _dst: 2 },
      // Points at a node that was not fetched.
      { _id: 11, _label: "Follows", _src: 1, _dst: 99 },
    ],
  };

  it("keeps only the induced subgraph and counts what it dropped", () => {
    const result = readResult(body, shape);
    if (result.kind !== "graph") throw new Error("expected a graph");
    expect(result.graph.edges.map((edge) => edge.id)).toEqual(["10"]);
    expect(result.graph.danglingEdges).toBe(1);
  });

  it("counts a repeated dangling edge once", () => {
    const repeated = {
      ...body,
      edges: [...body.edges, { _id: 11, _label: "Follows", _src: 1, _dst: 99 }],
    };
    const result = readResult(repeated, shape);
    if (result.kind !== "graph") throw new Error("expected a graph");
    expect(result.graph.danglingEdges).toBe(1);
  });

  it("separates identity fields from displayable properties", () => {
    const result = readResult(body, shape);
    if (result.kind !== "graph") throw new Error("expected a graph");
    expect(result.graph.nodes[0]).toEqual({
      id: "1",
      label: "User",
      properties: { name: "Alice" },
    });
  });

  it("keys entities as strings so bigint and number ids still match", () => {
    const result = readResult(
      {
        nodes: [{ _id: 9007199254740993n, _label: "User" }, { _id: 2, _label: "User" }],
        edges: [{ _id: 3, _label: "Follows", _src: 9007199254740993n, _dst: 2 }],
      },
      shape,
    );
    if (result.kind !== "graph") throw new Error("expected a graph");
    expect(result.graph.edges).toHaveLength(1);
    expect(result.graph.nodes[0].id).toBe("9007199254740993");
  });

  it("drops duplicates rather than drawing them twice", () => {
    const result = readResult(
      {
        nodes: [{ _id: 1, _label: "User" }, { _id: 1, _label: "User" }],
        edges: [
          { _id: 7, _label: "Follows", _src: 1, _dst: 1 },
          { _id: 7, _label: "Follows", _src: 1, _dst: 1 },
        ],
      },
      shape,
    );
    if (result.kind !== "graph") throw new Error("expected a graph");
    expect(result.graph.nodes).toHaveLength(1);
    expect(result.graph.edges).toHaveLength(1);
  });
});

describe("describe decoding", () => {
  it("splits identity, properties and relationships for a node", () => {
    const result = readResult(
      {
        entity: [{ name: "Alice", age: 30 }],
        identity: [{ $id: 1n, $label: "User" }],
        edges: [{ _id: 10, _label: "Follows", _src: 1, _dst: 2 }],
        neighbours: [{ _id: 2, _label: "User" }],
        degree: 3,
      },
      shapeOf("DESCRIBE NODE 1"),
    );
    expect(result).toMatchObject({
      kind: "describe",
      entity: "nodes",
      id: "1",
      label: "User",
      properties: { name: "Alice", age: 30 },
      degree: 3,
    });
  });

  it("resolves both endpoints for an edge", () => {
    const result = readResult(
      {
        entity: [{ since: 2020 }],
        identity: [{ $id: 10n, $label: "Follows" }],
        sourceNode: [{ _id: 1, _label: "User" }],
        targetNode: [{ _id: 2, _label: "User" }],
      },
      shapeOf("DESCRIBE EDGE 10"),
    );
    if (result.kind !== "describe") throw new Error("expected a description");
    expect(result.endpoints?.source?.id).toBe("1");
    expect(result.endpoints?.target?.id).toBe("2");
  });
});

describe("cell formatting", () => {
  it("renders each scalar the way a table cell should read", () => {
    expect(stringify(null)).toBe("");
    expect(stringify(undefined)).toBe("");
    expect(stringify("text")).toBe("text");
    expect(stringify(false)).toBe("false");
    expect(stringify(9223372036854775807n)).toBe("9223372036854775807");
    // Nested values are still readable, and bigint inside them survives.
    expect(stringify({ a: [1n, 2] })).toBe('{"a":["1",2]}');
  });
});

describe("label colours", () => {
  it("gives the busiest labels their own hue, deterministically", () => {
    const palette = new LabelPalette(["b", "a", "a", "a", "b", "b", "b", "c"]);
    expect(palette.legend.map((entry) => entry.label)).toEqual(["b", "a", "c"]);
    expect(palette.legend.map((entry) => entry.count)).toEqual([4, 3, 1]);
    // Same input, same assignment.
    expect(new LabelPalette(["b", "a", "a", "a", "b", "b", "b", "c"]).legend).toEqual(palette.legend);
  });

  it("never gives two labels the same hue", () => {
    const labels = Array.from({ length: MAX_COLOURED_LABELS }, (_, i) => `L${i}`);
    const palette = new LabelPalette(labels);
    const colours = labels.map((label) => palette.colour(label, "dark"));
    expect(new Set(colours).size).toBe(MAX_COLOURED_LABELS);
  });

  it("folds the overflow into a neutral bucket instead of cycling hues", () => {
    const labels = Array.from({ length: MAX_COLOURED_LABELS + 4 }, (_, i) => `L${i}`);
    const palette = new LabelPalette(labels);
    expect(palette.legend).toHaveLength(MAX_COLOURED_LABELS + 1);

    const other = palette.legend.at(-1)!;
    expect(other.label).toBe(OTHER_LABEL);
    expect(other.slot).toBeNull();
    expect(other.count).toBe(4);
  });

  it("treats an unlabelled entity as neutral, not as a series", () => {
    const palette = new LabelPalette(["User", null, null]);
    expect(palette.colour(null, "dark")).not.toBe(palette.colour("User", "dark"));
  });

  it("gives a label genuinely named Other its own hue", () => {
    // The bucket's name must not swallow a real label, or the two would share
    // the neutral colour and the legend would show the name twice.
    const palette = new LabelPalette([OTHER_LABEL, OTHER_LABEL, "User", null]);
    expect(palette.colour(OTHER_LABEL, "dark")).not.toBe(palette.colour(null, "dark"));

    const bucket = palette.legend.find((entry) => entry.slot === null)!;
    expect(bucket.label).not.toBe(OTHER_LABEL);
    expect(bucket.count).toBe(1); // the one unlabelled entity
    expect(new Set(palette.legend.map((e) => e.label)).size).toBe(palette.legend.length);
  });

  it("counts how many labels were folded into the bucket", () => {
    const labels = Array.from({ length: MAX_COLOURED_LABELS + 4 }, (_, i) => `L${i}`);
    expect(new LabelPalette(labels).overflowCount).toBe(4);
    expect(new LabelPalette(["a", "b"]).overflowCount).toBe(0);
    // Unlabelled entities fill the bucket without being a folded label.
    expect(new LabelPalette(["a", null, null]).overflowCount).toBe(0);
  });

  it("steps each theme separately rather than flipping one", () => {
    const palette = new LabelPalette(["User"]);
    expect(palette.colour("User", "light")).not.toBe(palette.colour("User", "dark"));
  });
});
