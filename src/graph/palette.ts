/**
 * Colour assignment for the graph view.
 *
 * Colour here is categorical — it encodes an entity's label, nothing else. The
 * eight hues are taken in fixed order and never cycled: a graph with more than
 * eight labels folds the rarest into a neutral "Other" slot rather than reusing
 * a hue, so two labels can never share a colour.
 *
 * Both columns are selected for their own surface rather than one being a
 * lightened flip of the other. The order is the colourblind-safety mechanism,
 * not decoration.
 *
 * Because nodes land next to each other arbitrarily, colour is never the only
 * cue: the legend is always on screen, and the canvas direct-labels hovered,
 * selected and high-degree nodes.
 */

export type Theme = "light" | "dark";

export const OTHER_LABEL = "Other";

const CATEGORICAL: Readonly<Record<Theme, readonly string[]>> = {
  light: ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300", "#4a3aa7", "#e34948"],
  dark: ["#3987e5", "#d95926", "#199e70", "#c98500", "#d55181", "#008300", "#9085e9", "#e66767"],
};

/** Reserved for the overflow bucket and for entities with no label at all. */
const NEUTRAL: Readonly<Record<Theme, string>> = {
  light: "#898781",
  dark: "#898781",
};

export const MAX_COLOURED_LABELS = CATEGORICAL.light.length;

/** Chart chrome, kept in sync with the CSS custom properties in styles.css. */
export const CHROME: Readonly<Record<Theme, Record<string, string>>> = {
  light: {
    surface: "#fcfcfb",
    ink: "#0b0b0b",
    secondaryInk: "#52514e",
    muted: "#898781",
    edge: "#c3c2b7",
    edgeStrong: "#52514e",
    ring: "rgba(11,11,11,0.10)",
  },
  dark: {
    surface: "#1a1a19",
    ink: "#ffffff",
    secondaryInk: "#c3c2b7",
    muted: "#898781",
    edge: "#383835",
    edgeStrong: "#898781",
    ring: "rgba(255,255,255,0.10)",
  },
};

export interface LegendEntry {
  label: string;
  count: number;
  slot: number | null;
}

/**
 * Assigns colour slots to labels.
 *
 * Slots go to the most common labels first and ties break alphabetically, so
 * the same graph always paints the same way. Identity is bound to the label,
 * not to its rank within the current result — re-running a narrower query keeps
 * a label's colour only if the label set is unchanged, which is the honest
 * behaviour for an ad-hoc query tool.
 */
export class LabelPalette {
  private readonly slots = new Map<string, number>();
  readonly legend: LegendEntry[] = [];

  constructor(labels: Iterable<string | null>) {
    const counts = new Map<string, number>();
    for (const label of labels) {
      const key = label ?? OTHER_LABEL;
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }

    const ordered = [...counts.entries()].sort(
      (a, b) => b[1] - a[1] || a[0].localeCompare(b[0]),
    );

    let overflow = 0;
    for (const [label, count] of ordered) {
      if (label !== OTHER_LABEL && this.slots.size < MAX_COLOURED_LABELS) {
        this.slots.set(label, this.slots.size);
        this.legend.push({ label, count, slot: this.slots.get(label)! });
      } else {
        overflow += count;
      }
    }
    if (overflow > 0) {
      this.legend.push({ label: OTHER_LABEL, count: overflow, slot: null });
    }
  }

  /** The number of labels that did not get their own hue. */
  get overflowCount(): number {
    return this.legend.filter((entry) => entry.slot === null).length;
  }

  colour(label: string | null, theme: Theme): string {
    const slot = label === null ? undefined : this.slots.get(label);
    return slot === undefined ? NEUTRAL[theme] : CATEGORICAL[theme][slot];
  }
}

/** Colour for a legend entry, including the neutral overflow bucket. */
export function legendColour(entry: LegendEntry, theme: Theme): string {
  return entry.slot === null ? NEUTRAL[theme] : CATEGORICAL[theme][entry.slot];
}
