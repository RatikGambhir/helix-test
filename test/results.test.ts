import { describe, expect, it } from "vitest";

import { LabelPalette, MAX_COLOURED_LABELS, OTHER_LABEL } from "../src/graph/palette";
import { stringify } from "../src/results";

describe("cell formatting", () => {
  it("renders backend-decoded values for a table cell", () => {
    expect(stringify(null)).toBe("");
    expect(stringify("text")).toBe("text");
    expect(stringify(false)).toBe("false");
    expect(stringify("9223372036854775807")).toBe("9223372036854775807");
    expect(stringify({ a: ["1", 2] })).toBe('{"a":["1",2]}');
  });
});

describe("label colours", () => {
  it("gives the busiest labels their own hue, deterministically", () => {
    const palette = new LabelPalette(["b", "a", "a", "a", "b", "b", "b", "c"]);
    expect(palette.legend.map((entry) => entry.label)).toEqual(["b", "a", "c"]);
    expect(palette.legend.map((entry) => entry.count)).toEqual([4, 3, 1]);
    expect(new LabelPalette(["b", "a", "a", "a", "b", "b", "b", "c"]).legend).toEqual(
      palette.legend,
    );
  });

  it("never gives two labels the same hue", () => {
    const labels = Array.from({ length: MAX_COLOURED_LABELS }, (_, index) => `L${index}`);
    const palette = new LabelPalette(labels);
    const colours = labels.map((label) => palette.colour(label, "dark"));
    expect(new Set(colours).size).toBe(MAX_COLOURED_LABELS);
  });

  it("folds overflow into a neutral bucket instead of cycling hues", () => {
    const labels = Array.from({ length: MAX_COLOURED_LABELS + 4 }, (_, index) => `L${index}`);
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
    const palette = new LabelPalette([OTHER_LABEL, OTHER_LABEL, "User", null]);
    expect(palette.colour(OTHER_LABEL, "dark")).not.toBe(palette.colour(null, "dark"));
    const bucket = palette.legend.find((entry) => entry.slot === null)!;
    expect(bucket.label).not.toBe(OTHER_LABEL);
    expect(bucket.count).toBe(1);
    expect(new Set(palette.legend.map((entry) => entry.label)).size).toBe(
      palette.legend.length,
    );
  });

  it("counts how many labels were folded into the bucket", () => {
    const labels = Array.from({ length: MAX_COLOURED_LABELS + 4 }, (_, index) => `L${index}`);
    expect(new LabelPalette(labels).overflowCount).toBe(4);
    expect(new LabelPalette(["a", "b"]).overflowCount).toBe(0);
    expect(new LabelPalette(["a", null, null]).overflowCount).toBe(0);
  });

  it("steps each theme separately rather than flipping one", () => {
    const palette = new LabelPalette(["User"]);
    expect(palette.colour("User", "light")).not.toBe(palette.colour("User", "dark"));
  });
});
