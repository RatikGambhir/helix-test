import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import type { GraphData } from "../results";
import { ForceLayout, type LayoutEdge, type LayoutNode } from "./layout";
import { CHROME, LabelPalette, type Theme } from "./palette";

export interface GraphSelectionEvent {
  kind: "node" | "edge";
  id: string;
  label: string | null;
}

interface Props {
  graph: GraphData;
  theme: Theme;
  selectedId: string | null;
  onSelect: (selection: GraphSelectionEvent | null) => void;
  /** Double-click asks the app to expand a node's neighbourhood. */
  onExpand: (nodeId: string) => void;
}

interface Viewport {
  scale: number;
  x: number;
  y: number;
}

/** Nodes above this count stop drawing labels except on hover/selection. */
const LABEL_BUDGET = 60;
const MIN_SCALE = 0.05;
const MAX_SCALE = 8;

export function GraphCanvas({ graph, theme, selectedId, onSelect, onExpand }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const layout = useMemo(() => {
    const next = new ForceLayout(graph);
    next.warmUp();
    return next;
  }, [graph]);

  const palette = useMemo(
    () => new LabelPalette(graph.nodes.map((node) => node.label)),
    [graph],
  );

  const viewportRef = useRef<Viewport>({ scale: 1, x: 0, y: 0 });
  const hoverRef = useRef<{ node: LayoutNode | null; edge: LayoutEdge | null }>({
    node: null,
    edge: null,
  });
  const dragRef = useRef<{ node: LayoutNode | null; panning: boolean; lastX: number; lastY: number }>({
    node: null,
    panning: false,
    lastX: 0,
    lastY: 0,
  });
  // 0 means "no frame scheduled" — `requestAnimationFrame` never returns 0.
  const frameRef = useRef(0);
  const dirtyRef = useRef(true);
  const wakeRef = useRef<() => void>(() => {});

  /**
   * Requests a redraw. The frame loop parks itself once the simulation has
   * cooled and nothing needs repainting, so anything that changes what is on
   * screen — or reheats the layout — has to come through here to restart it.
   */
  const markDirty = useCallback(() => {
    dirtyRef.current = true;
    wakeRef.current();
  }, []);

  const [hoverInfo, setHoverInfo] = useState<{
    x: number;
    y: number;
    title: string;
    lines: string[];
  } | null>(null);

  /** Scales and centres the layout so the whole graph is visible. */
  const fitToView = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas || layout.nodes.length === 0) return;
    const width = canvas.clientWidth;
    const height = canvas.clientHeight;
    const { minX, minY, maxX, maxY } = layout.bounds();
    const padding = 48;
    const scale = Math.min(
      (width - padding * 2) / Math.max(maxX - minX, 1),
      (height - padding * 2) / Math.max(maxY - minY, 1),
      2.5,
    );
    viewportRef.current = {
      scale: Math.max(MIN_SCALE, scale),
      x: width / 2 - ((minX + maxX) / 2) * Math.max(MIN_SCALE, scale),
      y: height / 2 - ((minY + maxY) / 2) * Math.max(MIN_SCALE, scale),
    };
    markDirty();
  }, [layout, markDirty]);

  useEffect(() => {
    fitToView();
  }, [fitToView]);

  const toGraphSpace = useCallback((clientX: number, clientY: number) => {
    const canvas = canvasRef.current;
    if (!canvas) return { x: 0, y: 0 };
    const rect = canvas.getBoundingClientRect();
    const { scale, x, y } = viewportRef.current;
    return {
      x: (clientX - rect.left - x) / scale,
      y: (clientY - rect.top - y) / scale,
    };
  }, []);

  // ---- rendering ----------------------------------------------------------

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d");
    if (!canvas || !context) return;

    const ratio = window.devicePixelRatio || 1;
    const width = canvas.clientWidth;
    const height = canvas.clientHeight;
    if (canvas.width !== width * ratio || canvas.height !== height * ratio) {
      canvas.width = width * ratio;
      canvas.height = height * ratio;
    }

    const chrome = CHROME[theme];
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    context.clearRect(0, 0, width, height);
    context.fillStyle = chrome.surface;
    context.fillRect(0, 0, width, height);

    const { scale, x: panX, y: panY } = viewportRef.current;
    context.translate(panX, panY);
    context.scale(scale, scale);

    const hovered = hoverRef.current;
    const selectedNode = selectedId ? layout.byId.get(selectedId) ?? null : null;
    const focus = hovered.node ?? selectedNode;
    // Everything one hop from the focused node stays fully opaque; the rest of
    // the graph dims, which is what makes a neighbourhood readable in a hairball.
    const related = new Set<string>();
    if (focus) {
      related.add(focus.id);
      for (const edge of layout.edges) {
        if (edge.source === focus) related.add(edge.target.id);
        if (edge.target === focus) related.add(edge.source.id);
      }
    }

    // --- edges
    context.lineCap = "round";
    for (const edge of layout.edges) {
      const incident = focus ? edge.source === focus || edge.target === focus : false;
      const dimmed = focus !== null && !incident;
      const isHovered = hovered.edge === edge;
      const isSelected = selectedId === edge.id;

      context.globalAlpha = dimmed ? 0.12 : isHovered || isSelected ? 1 : 0.55;
      context.strokeStyle = isHovered || isSelected ? chrome.edgeStrong : chrome.edge;
      context.lineWidth = (isHovered || isSelected ? 2.6 : 1.4) / scale;
      drawEdgePath(context, edge);
      context.stroke();

      if (!dimmed && scale > 0.45) {
        drawArrowhead(context, edge, scale, context.strokeStyle);
      }
    }

    // --- nodes
    context.globalAlpha = 1;
    for (const node of layout.nodes) {
      const dimmed = focus !== null && !related.has(node.id);
      const isSelected = node.id === selectedId;
      const isHovered = hovered.node === node;

      context.globalAlpha = dimmed ? 0.18 : 1;
      context.beginPath();
      context.arc(node.x, node.y, node.radius, 0, Math.PI * 2);
      context.fillStyle = palette.colour(node.label, theme);
      context.fill();

      // A surface-coloured ring keeps overlapping nodes readable as separate
      // marks instead of merging into one blob.
      context.lineWidth = 2 / scale;
      context.strokeStyle = chrome.surface;
      context.stroke();

      if (isSelected || isHovered) {
        context.beginPath();
        context.arc(node.x, node.y, node.radius + 4 / scale, 0, Math.PI * 2);
        context.lineWidth = 2 / scale;
        context.strokeStyle = chrome.ink;
        context.stroke();
      }
    }

    // --- direct labels
    // Colour alone never has to carry identity: the busiest nodes, plus
    // whatever is hovered or selected, are always named on the canvas.
    context.globalAlpha = 1;
    const fontSize = 12 / scale;
    context.font = `${fontSize}px system-ui, -apple-system, "Segoe UI", sans-serif`;
    context.textAlign = "center";
    context.textBaseline = "middle";

    const labelled = pickLabelled(layout.nodes, focus, related, scale);
    for (const node of labelled) {
      const text = nodeCaption(node);
      if (!text) continue;
      const y = node.y + node.radius + fontSize * 0.9;
      // Halo the text so it stays legible where it crosses an edge.
      context.lineWidth = 3 / scale;
      context.strokeStyle = chrome.surface;
      context.strokeText(text, node.x, y);
      context.fillStyle = node === focus ? chrome.ink : chrome.secondaryInk;
      context.fillText(text, node.x, y);
    }

    context.setTransform(1, 0, 0, 1, 0, 0);
  }, [layout, palette, selectedId, theme]);

  // ---- animation loop -----------------------------------------------------

  useEffect(() => {
    let cancelled = false;

    const loop = () => {
      frameRef.current = 0;
      if (cancelled) return;
      if (!layout.settled) {
        layout.tick();
        dirtyRef.current = true;
      }
      if (dirtyRef.current) {
        dirtyRef.current = false;
        draw();
      }
      // Park once the simulation has cooled and the canvas is up to date;
      // `markDirty` schedules the next frame. Without this the loop would keep
      // waking at the display refresh rate for the life of the window.
      if (layout.settled && !dirtyRef.current) return;
      frameRef.current = requestAnimationFrame(loop);
    };

    const wake = () => {
      if (cancelled || frameRef.current !== 0) return;
      frameRef.current = requestAnimationFrame(loop);
    };

    wakeRef.current = wake;
    wake();

    return () => {
      cancelled = true;
      if (frameRef.current !== 0) cancelAnimationFrame(frameRef.current);
      frameRef.current = 0;
      wakeRef.current = () => {};
    };
  }, [draw, layout]);

  useEffect(() => {
    markDirty();
  }, [markDirty, selectedId, theme]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      markDirty();
    });
    observer.observe(container);
    return () => observer.disconnect();
  }, [markDirty]);

  // ---- pointer interaction ------------------------------------------------

  const handlePointerDown = (event: React.PointerEvent<HTMLCanvasElement>) => {
    (event.target as Element).setPointerCapture(event.pointerId);
    const point = toGraphSpace(event.clientX, event.clientY);
    const node = layout.nodeAt(point.x, point.y);
    if (node) {
      node.pinned = true;
      dragRef.current = { node, panning: false, lastX: event.clientX, lastY: event.clientY };
    } else {
      dragRef.current = { node: null, panning: true, lastX: event.clientX, lastY: event.clientY };
    }
  };

  const handlePointerMove = (event: React.PointerEvent<HTMLCanvasElement>) => {
    const drag = dragRef.current;

    if (drag.node) {
      const point = toGraphSpace(event.clientX, event.clientY);
      drag.node.x = point.x;
      drag.node.y = point.y;
      layout.reheat(0.3);
      markDirty();
      return;
    }

    if (drag.panning) {
      const viewport = viewportRef.current;
      viewport.x += event.clientX - drag.lastX;
      viewport.y += event.clientY - drag.lastY;
      drag.lastX = event.clientX;
      drag.lastY = event.clientY;
      markDirty();
      return;
    }

    const point = toGraphSpace(event.clientX, event.clientY);
    const node = layout.nodeAt(point.x, point.y);
    const edge = node ? null : edgeAt(layout, point.x, point.y, 6 / viewportRef.current.scale);
    const previous = hoverRef.current;
    if (previous.node !== node || previous.edge !== edge) {
      hoverRef.current = { node, edge };
      markDirty();
      setHoverInfo(describeHover(node, edge, event.clientX, event.clientY, containerRef.current));
    } else if (hoverInfo && (node || edge)) {
      setHoverInfo(describeHover(node, edge, event.clientX, event.clientY, containerRef.current));
    }
  };

  const handlePointerUp = (event: React.PointerEvent<HTMLCanvasElement>) => {
    const drag = dragRef.current;
    const moved =
      Math.abs(event.clientX - drag.lastX) > 3 || Math.abs(event.clientY - drag.lastY) > 3;

    if (drag.node) {
      // Releasing without a real drag is a click, so unpin and select instead.
      drag.node.pinned = false;
      if (!moved) onSelect({ kind: "node", id: drag.node.id, label: drag.node.label });
      layout.reheat(0.25);
    } else if (drag.panning && !moved) {
      const point = toGraphSpace(event.clientX, event.clientY);
      const edge = edgeAt(layout, point.x, point.y, 6 / viewportRef.current.scale);
      onSelect(edge ? { kind: "edge", id: edge.id, label: edge.label } : null);
    }

    dragRef.current = { node: null, panning: false, lastX: 0, lastY: 0 };
    markDirty();
  };

  const handleDoubleClick = (event: React.MouseEvent<HTMLCanvasElement>) => {
    const point = toGraphSpace(event.clientX, event.clientY);
    const node = layout.nodeAt(point.x, point.y);
    if (node) onExpand(node.id);
  };

  const handleWheel = (event: React.WheelEvent<HTMLCanvasElement>) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const viewport = viewportRef.current;
    const factor = Math.exp(-event.deltaY * 0.0015);
    const next = Math.max(MIN_SCALE, Math.min(MAX_SCALE, viewport.scale * factor));
    // Zoom about the cursor rather than the canvas centre.
    const px = event.clientX - rect.left;
    const py = event.clientY - rect.top;
    viewport.x = px - ((px - viewport.x) / viewport.scale) * next;
    viewport.y = py - ((py - viewport.y) / viewport.scale) * next;
    viewport.scale = next;
    markDirty();
  };

  const handlePointerLeave = () => {
    hoverRef.current = { node: null, edge: null };
    setHoverInfo(null);
    markDirty();
  };

  const zoomBy = (factor: number) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const viewport = viewportRef.current;
    const next = Math.max(MIN_SCALE, Math.min(MAX_SCALE, viewport.scale * factor));
    const cx = canvas.clientWidth / 2;
    const cy = canvas.clientHeight / 2;
    viewport.x = cx - ((cx - viewport.x) / viewport.scale) * next;
    viewport.y = cy - ((cy - viewport.y) / viewport.scale) * next;
    viewport.scale = next;
    markDirty();
  };

  return (
    <div className="graph-canvas" ref={containerRef}>
      <canvas
        ref={canvasRef}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onPointerLeave={handlePointerLeave}
        onDoubleClick={handleDoubleClick}
        onWheel={handleWheel}
      />

      <div className="graph-controls">
        <button type="button" onClick={() => zoomBy(1.3)} title="Zoom in" aria-label="Zoom in">
          +
        </button>
        <button type="button" onClick={() => zoomBy(1 / 1.3)} title="Zoom out" aria-label="Zoom out">
          −
        </button>
        <button type="button" onClick={fitToView} title="Fit to view">
          Fit
        </button>
        <button
          type="button"
          onClick={() => {
            for (const node of layout.nodes) node.pinned = false;
            layout.reheat(1);
            markDirty();
          }}
          title="Re-run the layout"
        >
          Relayout
        </button>
      </div>

      <ul className="graph-legend" aria-label="Node labels">
        {palette.legend.map((entry) => (
          <li key={`${entry.slot ?? "other"}-${entry.label}`}>
            <span
              className="swatch"
              style={{ background: palette.colour(entry.slot === null ? null : entry.label, theme) }}
              aria-hidden="true"
            />
            <span className="legend-label">{entry.label}</span>
            <span className="legend-count">{entry.count.toLocaleString()}</span>
          </li>
        ))}
      </ul>

      {hoverInfo && (
        <div className="graph-tooltip" style={{ left: hoverInfo.x, top: hoverInfo.y }} role="tooltip">
          <strong>{hoverInfo.title}</strong>
          {hoverInfo.lines.map((line) => (
            <span key={line}>{line}</span>
          ))}
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Drawing helpers
// ---------------------------------------------------------------------------

/** Quadratic control point that bows parallel edges apart from each other. */
function edgeCurvature(edge: LayoutEdge): number {
  if (edge.parallelCount <= 1) return 0;
  // Spread indices symmetrically around zero: 0, +1, -1, +2, -2 …
  const centred = edge.parallelIndex - (edge.parallelCount - 1) / 2;
  return centred * 22;
}

function drawEdgePath(context: CanvasRenderingContext2D, edge: LayoutEdge): void {
  context.beginPath();
  if (edge.loop) {
    // Self-edges are drawn as a small circle sitting above the node.
    const r = edge.source.radius + 9 + edge.parallelIndex * 6;
    context.arc(edge.source.x, edge.source.y - r * 0.8, r, 0, Math.PI * 2);
    return;
  }

  const curvature = edgeCurvature(edge);
  context.moveTo(edge.source.x, edge.source.y);
  if (curvature === 0) {
    context.lineTo(edge.target.x, edge.target.y);
    return;
  }
  const mx = (edge.source.x + edge.target.x) / 2;
  const my = (edge.source.y + edge.target.y) / 2;
  const dx = edge.target.x - edge.source.x;
  const dy = edge.target.y - edge.source.y;
  const length = Math.hypot(dx, dy) || 1;
  context.quadraticCurveTo(
    mx - (dy / length) * curvature,
    my + (dx / length) * curvature,
    edge.target.x,
    edge.target.y,
  );
}

/** Direction is part of the data, so every edge gets an arrowhead. */
function drawArrowhead(
  context: CanvasRenderingContext2D,
  edge: LayoutEdge,
  scale: number,
  colour: string | CanvasGradient | CanvasPattern,
): void {
  if (edge.loop) return;
  const dx = edge.target.x - edge.source.x;
  const dy = edge.target.y - edge.source.y;
  const length = Math.hypot(dx, dy);
  if (length < 1) return;

  const ux = dx / length;
  const uy = dy / length;
  // Sit the head on the rim of the target node, not at its centre.
  const tipX = edge.target.x - ux * (edge.target.radius + 1.5 / scale);
  const tipY = edge.target.y - uy * (edge.target.radius + 1.5 / scale);
  const size = 7 / scale;

  context.beginPath();
  context.moveTo(tipX, tipY);
  context.lineTo(tipX - ux * size + uy * size * 0.45, tipY - uy * size - ux * size * 0.45);
  context.lineTo(tipX - ux * size - uy * size * 0.45, tipY - uy * size + ux * size * 0.45);
  context.closePath();
  context.fillStyle = colour;
  context.fill();
}

/** Picks which nodes get a permanent caption, within a readability budget. */
function pickLabelled(
  nodes: LayoutNode[],
  focus: LayoutNode | null,
  related: Set<string>,
  scale: number,
): LayoutNode[] {
  if (focus) {
    // Zoomed into a neighbourhood: name the focus and everything around it.
    return nodes.filter((node) => related.has(node.id)).slice(0, LABEL_BUDGET * 2);
  }
  if (nodes.length <= LABEL_BUDGET || scale > 1.4) return nodes;
  // Too many to name them all — the highest-degree nodes carry the map.
  return [...nodes].sort((a, b) => b.degree - a.degree).slice(0, LABEL_BUDGET);
}

/** The most human-readable name a node has: a name-ish property, else its label. */
function nodeCaption(node: LayoutNode): string {
  for (const key of ["name", "title", "label", "username", "email", "id"]) {
    const value = node.properties[key];
    if (typeof value === "string" && value.length > 0) return truncate(value, 24);
    if (typeof value === "number" || typeof value === "bigint") return String(value);
  }
  return node.label ?? "";
}

function truncate(value: string, max: number): string {
  return value.length <= max ? value : `${value.slice(0, max - 1)}…`;
}

/** Distance from a point to an edge, used for hit-testing. */
function edgeAt(
  layout: ForceLayout,
  x: number,
  y: number,
  tolerance: number,
): LayoutEdge | null {
  let best: LayoutEdge | null = null;
  let bestDistance = tolerance;
  for (const edge of layout.edges) {
    if (edge.loop) continue;
    const distance = distanceToSegment(x, y, edge.source.x, edge.source.y, edge.target.x, edge.target.y);
    if (distance < bestDistance) {
      best = edge;
      bestDistance = distance;
    }
  }
  return best;
}

function distanceToSegment(
  px: number,
  py: number,
  ax: number,
  ay: number,
  bx: number,
  by: number,
): number {
  const dx = bx - ax;
  const dy = by - ay;
  const lengthSquared = dx * dx + dy * dy;
  if (lengthSquared === 0) return Math.hypot(px - ax, py - ay);
  const t = Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / lengthSquared));
  return Math.hypot(px - (ax + t * dx), py - (ay + t * dy));
}

function describeHover(
  node: LayoutNode | null,
  edge: LayoutEdge | null,
  clientX: number,
  clientY: number,
  container: HTMLElement | null,
): { x: number; y: number; title: string; lines: string[] } | null {
  if (!node && !edge) return null;
  const rect = container?.getBoundingClientRect();
  const x = clientX - (rect?.left ?? 0) + 14;
  const y = clientY - (rect?.top ?? 0) + 14;

  if (node) {
    const lines = [`id ${node.id}`, `${node.degree} edge${node.degree === 1 ? "" : "s"}`];
    for (const [key, value] of Object.entries(node.properties).slice(0, 6)) {
      lines.push(`${key}: ${truncate(formatValue(value), 40)}`);
    }
    return { x, y, title: node.label ?? "(no label)", lines };
  }

  return {
    x,
    y,
    title: edge!.label ?? "(no label)",
    lines: [`id ${edge!.id}`, `${edge!.source.id} → ${edge!.target.id}`],
  };
}

function formatValue(value: unknown): string {
  if (typeof value === "bigint") return value.toString();
  if (value === null || value === undefined) return "";
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}
