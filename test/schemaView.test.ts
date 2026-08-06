import { describe, expect, it } from "vitest";

import {
  filterSchemaEntries,
  formatSchemaValue,
  inferSchemaFields,
  schemaQueryFor,
} from "../src/schema";

const entries = [
  { label: "User", count: 12 },
  { label: "Quarry File", count: 4 },
  { label: "Chunk", count: 30 },
];

describe("schema workspace helpers", () => {
  it("filters labels case-insensitively without mutating their order", () => {
    expect(filterSchemaEntries(entries, "r")).toEqual([entries[0], entries[1]]);
    expect(filterSchemaEntries(entries, "  CHUNK ")).toEqual([entries[2]]);
    expect(filterSchemaEntries(entries, "")).toEqual(entries);
  });

  it("builds safe starter queries for node and edge labels", () => {
    expect(schemaQueryFor("nodes", "User")).toBe("SELECT * FROM NODES:User LIMIT 50");
    expect(schemaQueryFor("nodes", "Quarry File")).toBe(
      'SELECT * FROM NODES:"Quarry File" LIMIT 50',
    );
    expect(schemaQueryFor("edges", 'Worked "At"')).toBe(
      'SELECT * FROM EDGES:"Worked ""At""" LIMIT 50',
    );
    expect(schemaQueryFor("nodes", "User", 5)).toBe("SELECT * FROM NODES:User LIMIT 5");
  });

  it("infers property names, types and representative values from sampled rows", () => {
    expect(
      inferSchemaFields([
        { id: 1n, label: "QuarryFile", path: "/tmp/a.pdf", pages: 4, archived: false },
        { id: 2n, label: "QuarryFile", path: "/tmp/b.pdf", pages: 8, archived: null },
        { id: 3n, label: "QuarryFile", path: "/tmp/a.pdf", pages: 8 },
      ]),
    ).toEqual([
      { name: "archived", types: ["boolean", "null"], values: [false, null], presentOn: 2 },
      { name: "pages", types: ["number"], values: [4, 8], presentOn: 3 },
      { name: "path", types: ["string"], values: ["/tmp/a.pdf", "/tmp/b.pdf"], presentOn: 3 },
    ]);
  });

  it("formats nested and long sample values compactly", () => {
    expect(formatSchemaValue([1, 2, 3])).toBe("[1,2,3]");
    expect(formatSchemaValue(9223372036854775807n)).toBe("9223372036854775807");
    expect(formatSchemaValue("")).toBe('""');
    expect(formatSchemaValue("x".repeat(100))).toBe(`${"x".repeat(77)}…`);
  });
});
