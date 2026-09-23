import { describe, expect, it } from "vitest";

import {
  filterSchemaEntries,
  formatSchemaValue,
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

  it("formats nested and long sample values compactly", () => {
    expect(formatSchemaValue([1, 2, 3])).toBe("[1,2,3]");
    expect(formatSchemaValue("9223372036854775807")).toBe("9223372036854775807");
    expect(formatSchemaValue("")).toBe('""');
    expect(formatSchemaValue("x".repeat(100))).toBe(`${"x".repeat(77)}…`);
  });
});
