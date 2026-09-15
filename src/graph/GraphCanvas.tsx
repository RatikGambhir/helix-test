import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Background,
  BackgroundVariant,
  BaseEdge,
  ControlButton,
  Controls,
  EdgeLabelRenderer,
  Handle,
  MarkerType,
  MiniMap,
  Panel,
  Position,
  ReactFlow,
  useEdgesState,
  useNodesState,
  type Edge,
  type EdgeProps,
  type Node,
  type NodeProps,
  type ReactFlowInstance,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";

import { Button } from "@/components/ui/button";
import type { GraphData } from "../results";
import { ForceLayout } from "./layout";
import { LabelPalette, legendColour, type Theme } from "./palette";

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

type EntityNodeData = {
  label: string | null;
  caption: string;
  degree: number;
  properties: Record<string, unknown>;
  colour: string;
};

type RelationshipEdgeData = {
  label: string | null;
  parallelIndex: number;
  parallelCount: number;
  loop: boolean;
};

type EntityNode = Node<EntityNodeData, "entity">;
type RelationshipEdge = Edge<RelationshipEdgeData, "relationship">;

const NODE_TYPES = { entity: EntityNodeCard };
const EDGE_TYPES = { relationship: RelationshipEdgePath };
const NODE_ORIGIN: [number, number] = [0.5, 0.5];
const CARD_WIDTH = 164;
const CARD_HEIGHT = 52;
const CARD_GAP = 18;

export function GraphCanvas({ graph, theme, selectedId, onSelect, onExpand }: Props) {
  const palette = useMemo(
    () => new LabelPalette(graph.nodes.map((node) => node.label)),
    [graph],
  );
  const initial = useMemo(() => buildFlowElements(graph), [graph]);
  const [nodes, setNodes, onNodesChange] = useNodesState<EntityNode>(initial.nodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState<RelationshipEdge>(initial.edges);
  const [hoveredNodeId, setHoveredNodeId] = useState<string | null>(null);
  const [hoveredLabel, setHoveredLabel] = useState<string | null>(null);
  const flowRef = useRef<ReactFlowInstance<EntityNode, RelationshipEdge> | null>(null);

  const adjacency = useMemo(() => {
    const next = new Map<string, Set<string>>();
    for (const edge of graph.edges) {
      if (!next.has(edge.source)) next.set(edge.source, new Set());
      if (!next.has(edge.target)) next.set(edge.target, new Set());
      next.get(edge.source)!.add(edge.target);
      next.get(edge.target)!.add(edge.source);
    }
    return next;
  }, [graph.edges]);

  useEffect(() => {
    setNodes(initial.nodes);
    setEdges(initial.edges);
    setHoveredNodeId(null);
    requestAnimationFrame(() => flowRef.current?.fitView({ padding: 0.2, duration: 300 }));
  }, [initial, setEdges, setNodes]);

  useEffect(() => {
    setNodes((current) => current.map((node) => ({ ...node, selected: node.id === selectedId })));
    setEdges((current) => current.map((edge) => ({ ...edge, selected: edge.id === selectedId })));
  }, [selectedId, setEdges, setNodes]);

  const focusId = hoveredNodeId ?? (nodes.some((node) => node.id === selectedId) ? selectedId : null);
  const visibleNodeIds = useMemo(() => {
    if (!focusId) return null;
    return new Set([focusId, ...(adjacency.get(focusId) ?? [])]);
  }, [adjacency, focusId]);

  const displayNodes = useMemo(
    () => nodes.map((node) => {
      const colour = palette.colour(node.data.label, theme);
      const labelMuted = hoveredLabel !== null && node.data.label !== hoveredLabel;
      const neighbourhoodMuted = visibleNodeIds !== null && !visibleNodeIds.has(node.id);
      return {
        ...node,
        className: labelMuted || neighbourhoodMuted ? "is-dimmed" : undefined,
        data: { ...node.data, colour },
      };
    }),
    [hoveredLabel, nodes, palette, theme, visibleNodeIds],
  );

  const displayEdges = useMemo(
    () => edges.map((edge) => {
      const sourceLabel = nodes.find((node) => node.id === edge.source)?.data.label;
      const targetLabel = nodes.find((node) => node.id === edge.target)?.data.label;
      const labelMuted = hoveredLabel !== null && sourceLabel !== hoveredLabel && targetLabel !== hoveredLabel;
      const neighbourhoodMuted = focusId !== null && edge.source !== focusId && edge.target !== focusId;
      return {
        ...edge,
        className: labelMuted || neighbourhoodMuted ? "is-dimmed" : undefined,
      };
    }),
    [edges, focusId, hoveredLabel, nodes],
  );

  const restoreLayout = useCallback(() => {
    const next = buildFlowElements(graph);
    setNodes(next.nodes);
    setEdges(next.edges);
    requestAnimationFrame(() => flowRef.current?.fitView({ padding: 0.2, duration: 300 }));
  }, [graph, setEdges, setNodes]);

  return (
    <div className="graph-canvas">
      <ReactFlow<EntityNode, RelationshipEdge>
        nodes={displayNodes}
        edges={displayEdges}
        nodeTypes={NODE_TYPES}
        edgeTypes={EDGE_TYPES}
        nodeOrigin={NODE_ORIGIN}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onInit={(instance) => { flowRef.current = instance; }}
        onNodeClick={(_event, node) => onSelect({ kind: "node", id: node.id, label: node.data.label })}
        onNodeDoubleClick={(_event, node) => onExpand(node.id)}
        onNodeMouseEnter={(_event, node) => setHoveredNodeId(node.id)}
        onNodeMouseLeave={() => setHoveredNodeId(null)}
        onEdgeClick={(_event, edge) => onSelect({ kind: "edge", id: edge.id, label: edge.data?.label ?? null })}
        onPaneClick={() => onSelect(null)}
        nodesDraggable
        nodesConnectable={false}
        edgesReconnectable={false}
        elementsSelectable
        panOnDrag={[0, 1]}
        minZoom={0.08}
        maxZoom={3.5}
        fitView
        fitViewOptions={{ padding: 0.2, maxZoom: 1.35 }}
        colorMode={theme}
        deleteKeyCode={null}
        multiSelectionKeyCode="Shift"
        aria-label="Interactive graph visualization"
      >
        <Background variant={BackgroundVariant.Dots} gap={22} size={1.2} />
        <Controls position="top-right" showInteractive={false}>
          <ControlButton onClick={restoreLayout} title="Reset node layout" aria-label="Reset node layout">
            <ResetIcon />
          </ControlButton>
        </Controls>
        {graph.nodes.length <= 500 ? (
          <MiniMap
            position="bottom-right"
            pannable
            zoomable
            nodeColor={(node) => (node.data as EntityNodeData).colour}
            nodeStrokeWidth={2}
            ariaLabel="Graph minimap"
          />
        ) : null}
        <Panel position="bottom-left" className="graph-legend-panel">
          <div className="graph-legend-heading">
            <span>Node labels</span>
            <span>{graph.nodes.length.toLocaleString()} total</span>
          </div>
          <ul className="graph-legend" aria-label="Node labels">
            {palette.legend.map((entry) => (
              <li key={`${entry.slot ?? "other"}-${entry.label}`}>
                <Button
                  variant="ghost"
                  onMouseEnter={() => setHoveredLabel(entry.slot === null ? null : entry.label)}
                  onMouseLeave={() => setHoveredLabel(null)}
                  onFocus={() => setHoveredLabel(entry.slot === null ? null : entry.label)}
                  onBlur={() => setHoveredLabel(null)}
                >
                  <span className="swatch" style={{ background: legendColour(entry, theme) }} aria-hidden="true" />
                  <span className="legend-label">{entry.label}</span>
                  <span className="legend-count">{entry.count.toLocaleString()}</span>
                </Button>
              </li>
            ))}
          </ul>
        </Panel>
        <Panel position="top-left" className="graph-help">
          Drag nodes · scroll to zoom · drag canvas to pan · shift-drag to select
        </Panel>
      </ReactFlow>
    </div>
  );
}

function EntityNodeCard({ data, selected }: NodeProps<EntityNode>) {
  const propertyCount = Object.keys(data.properties).length;
  const description = [
    data.label ?? "Unlabelled node",
    data.caption,
    `${data.degree} relationship${data.degree === 1 ? "" : "s"}`,
    `${propertyCount} propert${propertyCount === 1 ? "y" : "ies"}`,
  ].join(" · ");

  return (
    <div
      className={`entity-node${selected ? " is-selected" : ""}`}
      style={{ "--node-colour": data.colour } as React.CSSProperties}
      title={description}
    >
      <span className="entity-node-accent" aria-hidden="true" />
      <div className="entity-node-copy">
        <span className="entity-node-caption">{data.caption}</span>
        <span className="entity-node-meta">
          {data.label ?? "Unlabelled"} <span aria-hidden="true">·</span> {data.degree}
        </span>
      </div>
      <span className="entity-node-port" aria-hidden="true" />
      {([Position.Top, Position.Right, Position.Bottom, Position.Left] as const).flatMap((position) => {
        const side = position.toLowerCase();
        return [
          <Handle key={`source-${side}`} type="source" position={position} id={`source-${side}`} isConnectable={false} />,
          <Handle key={`target-${side}`} type="target" position={position} id={`target-${side}`} isConnectable={false} />,
        ];
      })}
    </div>
  );
}

function RelationshipEdgePath({
  id,
  source,
  target,
  sourceX,
  sourceY,
  targetX,
  targetY,
  markerEnd,
  selected,
  data,
  style,
}: EdgeProps<RelationshipEdge>) {
  const parallelIndex = data?.parallelIndex ?? 0;
  const parallelCount = data?.parallelCount ?? 1;
  const centred = parallelIndex - (parallelCount - 1) / 2;
  const offset = centred * 28;
  const dx = targetX - sourceX;
  const dy = targetY - sourceY;
  const length = Math.hypot(dx, dy) || 1;
  const middleX = (sourceX + targetX) / 2 - (dy / length) * offset;
  const middleY = (sourceY + targetY) / 2 + (dx / length) * offset;
  const isLoop = data?.loop || source === target;
  const path = isLoop
    ? `M ${sourceX} ${sourceY} C ${sourceX + 84} ${sourceY - 92}, ${targetX - 84} ${targetY - 92}, ${targetX} ${targetY}`
    : `M ${sourceX} ${sourceY} Q ${middleX} ${middleY} ${targetX} ${targetY}`;
  const labelX = isLoop ? (sourceX + targetX) / 2 : (sourceX + 2 * middleX + targetX) / 4;
  const labelY = isLoop ? Math.min(sourceY, targetY) - 70 : (sourceY + 2 * middleY + targetY) / 4;

  return (
    <>
      <BaseEdge
        id={id}
        path={path}
        markerEnd={markerEnd}
        interactionWidth={18}
        style={style}
        className={selected ? "is-selected" : undefined}
      />
      {selected && data?.label ? (
        <EdgeLabelRenderer>
          <div className="edge-label nodrag nopan" style={{ transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)` }}>
            {data.label}
          </div>
        </EdgeLabelRenderer>
      ) : null}
    </>
  );
}

function buildFlowElements(graph: GraphData): { nodes: EntityNode[]; edges: RelationshipEdge[] } {
  const layout = new ForceLayout(graph);
  layout.warmUp(150);
  spreadCardNodes(layout.nodes);

  const nodes: EntityNode[] = layout.nodes.map((node) => ({
    id: node.id,
    type: "entity",
    position: { x: node.x, y: node.y },
    data: {
      label: node.label,
      caption: nodeCaption(node.properties, node.label, node.id),
      degree: node.degree,
      properties: node.properties,
      colour: "currentColor",
    },
    ariaLabel: `${node.label ?? "Unlabelled node"}: ${nodeCaption(node.properties, node.label, node.id)}`,
  }));

  const positions = new Map(layout.nodes.map((node) => [node.id, node]));
  const edges: RelationshipEdge[] = layout.edges.map((edge) => {
    const source = positions.get(edge.source.id)!;
    const target = positions.get(edge.target.id)!;
    const handles = edge.loop
      ? { sourceHandle: "source-right", targetHandle: "target-top" }
      : handlesFor(source.x, source.y, target.x, target.y);
    return {
      id: edge.id,
      type: "relationship",
      source: edge.source.id,
      target: edge.target.id,
      ...handles,
      data: {
        label: edge.label,
        parallelIndex: edge.parallelIndex,
        parallelCount: edge.parallelCount,
        loop: edge.loop,
      },
      markerEnd: { type: MarkerType.ArrowClosed, width: 14, height: 14 },
      ariaLabel: `${edge.label ?? "Relationship"}: ${edge.source.id} to ${edge.target.id}`,
    };
  });

  return { nodes, edges };
}

/**
 * The force engine was originally tuned for circular canvas marks. React Flow
 * renders information-rich cards, so resolve their rectangular bounds before
 * handing the positions over. This keeps the topology organic without leaving
 * labels stacked on top of each other.
 */
function spreadCardNodes(nodes: Array<{ id: string; x: number; y: number }>): void {
  for (const node of nodes) {
    node.x *= 1.35;
    node.y *= 1.15;
  }

  const minimumX = CARD_WIDTH + CARD_GAP;
  const minimumY = CARD_HEIGHT + CARD_GAP;
  const passes = nodes.length > 800 ? 8 : 22;

  for (let pass = 0; pass < passes; pass++) {
    let moved = false;
    for (let index = 0; index < nodes.length; index++) {
      const node = nodes[index];
      for (let otherIndex = index + 1; otherIndex < nodes.length; otherIndex++) {
        const other = nodes[otherIndex];
        const dx = other.x - node.x;
        const dy = other.y - node.y;
        const overlapX = minimumX - Math.abs(dx);
        const overlapY = minimumY - Math.abs(dy);
        if (overlapX <= 0 || overlapY <= 0) continue;

        moved = true;
        if (overlapX < overlapY) {
          const direction = dx === 0 ? (node.id < other.id ? 1 : -1) : Math.sign(dx);
          const shift = overlapX / 2 + 0.5;
          node.x -= direction * shift;
          other.x += direction * shift;
        } else {
          const direction = dy === 0 ? (node.id < other.id ? 1 : -1) : Math.sign(dy);
          const shift = overlapY / 2 + 0.5;
          node.y -= direction * shift;
          other.y += direction * shift;
        }
      }
    }
    if (!moved) break;
  }
}

function handlesFor(sourceX: number, sourceY: number, targetX: number, targetY: number) {
  const dx = targetX - sourceX;
  const dy = targetY - sourceY;
  if (Math.abs(dx) >= Math.abs(dy)) {
    return dx >= 0
      ? { sourceHandle: "source-right", targetHandle: "target-left" }
      : { sourceHandle: "source-left", targetHandle: "target-right" };
  }
  return dy >= 0
    ? { sourceHandle: "source-bottom", targetHandle: "target-top" }
    : { sourceHandle: "source-top", targetHandle: "target-bottom" };
}

function nodeCaption(properties: Record<string, unknown>, label: string | null, id: string): string {
  for (const key of ["name", "title", "label", "username", "email"]) {
    const value = properties[key];
    if (typeof value === "string" && value.length > 0) return truncate(value, 28);
    if (typeof value === "number" || typeof value === "bigint") return String(value);
  }
  return label ? `${label} ${truncate(id, 16)}` : truncate(id, 22);
}

function truncate(value: string, max: number): string {
  return value.length <= max ? value : `${value.slice(0, max - 1)}…`;
}

function ResetIcon() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M4.8 7.7A8 8 0 1 1 4 14M4.8 7.7V3.5m0 4.2H9" />
    </svg>
  );
}
