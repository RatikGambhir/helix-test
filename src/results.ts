/** UI-facing result contracts produced by the Rust backend. */

export type Row = Record<string, unknown>;

export interface GraphNodeData {
  id: string;
  label: string | null;
  properties: Row;
}

export interface GraphEdgeData {
  id: string;
  label: string | null;
  source: string;
  target: string;
}

export interface GraphData {
  nodes: GraphNodeData[];
  edges: GraphEdgeData[];
  danglingEdges: number;
  truncatedNodes: boolean;
  truncatedEdges: boolean;
}

export interface LabelCount {
  label: string;
  count: number;
}

export type QueryResult =
  | { kind: "rows"; columns: string[]; rows: Row[] }
  | { kind: "count"; value: number }
  | { kind: "groupCount"; by: string; groups: LabelCount[] }
  | { kind: "graph"; graph: GraphData }
  | { kind: "labels"; nodes: LabelCount[] | null; edges: LabelCount[] | null }
  | {
      kind: "stats";
      nodeCount: number;
      edgeCount: number;
      nodeLabels: LabelCount[];
      edgeLabels: LabelCount[];
    }
  | {
      kind: "describe";
      entity: "nodes" | "edges";
      id: string | null;
      label: string | null;
      properties: Row;
      edges: GraphEdgeData[];
      neighbours: GraphNodeData[];
      degree: number | null;
      endpoints: { source: GraphNodeData | null; target: GraphNodeData | null } | null;
    };

/** Presentation-only formatting for values that Rust has already decoded. */
export function stringify(value: unknown): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}
