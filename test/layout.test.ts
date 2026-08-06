import { describe, expect, it } from "vitest";

import { ForceLayout, nodeRadius } from "../src/graph/layout";
import type { GraphData } from "../src/results";

function graph(
  nodes: { id: string; label?: string }[],
  edges: { id: string; source: string; target: string; label?: string }[] = [],
): GraphData {
  return {
    nodes: nodes.map((node) => ({ id: node.id, label: node.label ?? null, properties: {} })),
    edges: edges.map((edge) => ({
      id: edge.id,
      label: edge.label ?? null,
      source: edge.source,
      target: edge.target,
    })),
    danglingEdges: 0,
    truncatedNodes: false,
    truncatedEdges: false,
  };
}

/** A ring of `count` nodes, each linked to the next. */
function ring(count: number): GraphData {
  const nodes = Array.from({ length: count }, (_, index) => ({ id: `n${index}` }));
  const edges = Array.from({ length: count }, (_, index) => ({
    id: `e${index}`,
    source: `n${index}`,
    target: `n${(index + 1) % count}`,
  }));
  return graph(nodes, edges);
}

describe("force layout", () => {
  it("indexes nodes and resolves edge endpoints to them", () => {
    const layout = new ForceLayout(graph([{ id: "a" }, { id: "b" }], [{ id: "e", source: "a", target: "b" }]));
    expect(layout.nodes).toHaveLength(2);
    expect(layout.edges[0].source).toBe(layout.byId.get("a"));
    expect(layout.edges[0].target).toBe(layout.byId.get("b"));
  });

  it("drops an edge whose endpoint was never fetched", () => {
    const layout = new ForceLayout(graph([{ id: "a" }], [{ id: "e", source: "a", target: "ghost" }]));
    expect(layout.edges).toHaveLength(0);
  });

  it("counts degree from incident edges and sizes nodes by it", () => {
    const layout = new ForceLayout(
      graph(
        [{ id: "hub" }, { id: "a" }, { id: "b" }],
        [
          { id: "e1", source: "hub", target: "a" },
          { id: "e2", source: "b", target: "hub" },
        ],
      ),
    );
    expect(layout.byId.get("hub")!.degree).toBe(2);
    expect(layout.byId.get("a")!.degree).toBe(1);
    expect(layout.byId.get("hub")!.radius).toBeGreaterThan(layout.byId.get("a")!.radius);
  });

  it("marks self-edges as loops", () => {
    const layout = new ForceLayout(graph([{ id: "a" }], [{ id: "e", source: "a", target: "a" }]));
    expect(layout.edges[0].loop).toBe(true);
  });

  it("indexes parallel edges so they can be drawn apart", () => {
    const layout = new ForceLayout(
      graph(
        [{ id: "a" }, { id: "b" }],
        [
          { id: "e1", source: "a", target: "b" },
          { id: "e2", source: "a", target: "b" },
          // Reversed direction is still the same pair for arc-spreading.
          { id: "e3", source: "b", target: "a" },
        ],
      ),
    );
    expect(layout.edges.map((edge) => edge.parallelIndex)).toEqual([0, 1, 2]);
    expect(layout.edges.every((edge) => edge.parallelCount === 3)).toBe(true);
  });

  it("is deterministic for the same input", () => {
    const positions = () => {
      const layout = new ForceLayout(ring(40));
      layout.warmUp(120);
      return layout.nodes.map((node) => [node.x, node.y]);
    };
    expect(positions()).toEqual(positions());
  });

  it("settles and reports progress", () => {
    const layout = new ForceLayout(ring(20));
    expect(layout.settled).toBe(false);
    for (let i = 0; i < 2000 && !layout.settled; i++) layout.tick();
    expect(layout.settled).toBe(true);
    expect(layout.progress).toBeCloseTo(1, 1);
  });

  it("reheats after a drag so the layout can respond", () => {
    const layout = new ForceLayout(ring(10));
    for (let i = 0; i < 2000 && !layout.settled; i++) layout.tick();
    layout.reheat(0.6);
    expect(layout.settled).toBe(false);
  });

  it("separates overlapping nodes", () => {
    // Every node starts at the same spot, the worst case for the repulsion
    // term on its own.
    const layout = new ForceLayout(graph(Array.from({ length: 30 }, (_, i) => ({ id: `n${i}` }))));
    for (const node of layout.nodes) {
      node.x = 0;
      node.y = 0;
    }
    layout.warmUp(200);

    let worstOverlap = 0;
    for (let i = 0; i < layout.nodes.length; i++) {
      for (let j = i + 1; j < layout.nodes.length; j++) {
        const a = layout.nodes[i];
        const b = layout.nodes[j];
        const gap = Math.hypot(a.x - b.x, a.y - b.y) - (a.radius + b.radius);
        worstOverlap = Math.min(worstOverlap, gap);
      }
    }
    expect(worstOverlap).toBeGreaterThanOrEqual(0);
  });

  it("keeps disconnected nodes within a bounded area", () => {
    const layout = new ForceLayout(graph(Array.from({ length: 60 }, (_, i) => ({ id: `n${i}` }))));
    layout.warmUp(400);
    const { minX, minY, maxX, maxY } = layout.bounds();
    // Gravity has to beat repulsion, or unlinked components drift to infinity.
    expect(maxX - minX).toBeLessThan(6000);
    expect(maxY - minY).toBeLessThan(6000);
    expect(layout.nodes.every((node) => Number.isFinite(node.x) && Number.isFinite(node.y))).toBe(true);
  });

  it("hit-tests the node under a point", () => {
    const layout = new ForceLayout(graph([{ id: "a" }, { id: "b" }]));
    const target = layout.byId.get("a")!;
    expect(layout.nodeAt(target.x, target.y)).toBe(target);
    expect(layout.nodeAt(target.x + 100_000, target.y)).toBeNull();
  });

  it("leaves a pinned node where it was put", () => {
    const layout = new ForceLayout(ring(12));
    const pinned = layout.nodes[0];
    pinned.pinned = true;
    pinned.x = 250;
    pinned.y = -125;
    layout.warmUp(150);
    expect(pinned.x).toBe(250);
    expect(pinned.y).toBe(-125);
  });

  it("handles an empty graph", () => {
    const layout = new ForceLayout(graph([]));
    expect(() => layout.warmUp(10)).not.toThrow();
    expect(layout.bounds()).toEqual({ minX: -1, minY: -1, maxX: 1, maxY: 1 });
    expect(layout.nodeAt(0, 0)).toBeNull();
  });

  it("caps node radius so a hub cannot dominate the canvas", () => {
    expect(nodeRadius(0)).toBeLessThan(nodeRadius(10));
    expect(nodeRadius(100_000)).toBeLessThanOrEqual(22);
  });
});
