/**
 * Turns a raw HelixDB response into the view models the UI renders.
 *
 * The reader is deliberately forgiving about the response envelope: a batch
 * that returns one variable may come back as a bare array, and group/count
 * terminals can reasonably be encoded either as a scalar or as a one-row table.
 * Every shape below is unwrapped rather than assumed.
 */
import { GRAPH_FIELDS, type ResultShape } from "./hql/compiler";

export type Scalar = string | number | bigint | boolean | null;
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
  /** Edges whose endpoints were not both in the node set, so nothing to draw. */
  danglingEdges: number;
  /** True when the node/edge caps were reached and the view may be partial. */
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
      /** Populated for nodes. */
      edges: GraphEdgeData[];
      neighbours: GraphNodeData[];
      degree: number | null;
      /** Populated for edges. */
      endpoints: { source: GraphNodeData | null; target: GraphNodeData | null } | null;
    };

/** Raised when the response cannot be read as the shape the query asked for. */
export class ResultError extends Error {
  readonly body: unknown;

  constructor(message: string, body: unknown) {
    super(message);
    this.name = "ResultError";
    this.body = body;
  }
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

/**
 * Extracts one named variable from a response body.
 *
 * Handles `{ "rows": [...] }`, a `{ "data": { … } }` wrapper, and the bare
 * array a single-variable batch may return.
 */
function pickVariable(body: unknown, name: string): unknown {
  if (Array.isArray(body)) return body;
  if (!isRecord(body)) return body;

  if (name in body) return body[name];
  for (const wrapper of ["data", "result", "results"]) {
    const inner = body[wrapper];
    if (isRecord(inner) && name in inner) return inner[name];
  }
  const keys = Object.keys(body);
  if (keys.length === 1) return body[keys[0]];
  throw new ResultError(`the response has no variable named "${name}"`, body);
}

/** Unwraps whatever a count terminal came back as into a plain number. */
function readCount(value: unknown): number {
  const unwrapped = Array.isArray(value) ? value[0] : value;
  if (typeof unwrapped === "number") return unwrapped;
  if (typeof unwrapped === "bigint") return Number(unwrapped);
  if (isRecord(unwrapped)) {
    for (const key of ["count", "value", "total"]) {
      const inner = unwrapped[key];
      if (typeof inner === "number") return inner;
      if (typeof inner === "bigint") return Number(inner);
    }
  }
  throw new ResultError("expected a count", value);
}

/**
 * Reads a group-count result.
 *
 * Accepts the map form (`{ "User": 12 }`) and the tabular form
 * (`[{ key: "User", count: 12 }]`), sorted by descending count either way.
 */
function readGroupCount(value: unknown): LabelCount[] {
  const groups: LabelCount[] = [];
  const unwrapped = isRecord(value) && Array.isArray(value.properties) ? value.properties : value;

  if (isRecord(unwrapped)) {
    for (const [label, count] of Object.entries(unwrapped)) {
      groups.push({ label, count: Number(count as number | bigint) });
    }
  } else if (Array.isArray(unwrapped)) {
    const sampled = new Map<string, number>();
    for (const entry of unwrapped) {
      if (!isRecord(entry)) continue;
      const label =
        entry.key ?? entry.group ?? entry.label ?? entry.value ?? entry[GRAPH_FIELDS.label] ?? entry.$label;
      const count = entry.count ?? entry.total ?? entry.n;
      if (label === undefined) continue;
      const name = stringify(label as Scalar);
      if (count === undefined) sampled.set(name, (sampled.get(name) ?? 0) + 1);
      else groups.push({ label: name, count: Number(count as number | bigint) });
    }
    for (const [label, count] of sampled) groups.push({ label, count });
  } else {
    throw new ResultError("expected a grouped count", value);
  }

  return groups.sort((a, b) => b.count - a.count || a.label.localeCompare(b.label));
}

function asRows(value: unknown, context: string): Row[] {
  if (value === null || value === undefined) return [];
  if (Array.isArray(value)) return value.filter(isRecord);
  if (isRecord(value) && Array.isArray(value.properties)) return value.properties.filter(isRecord);
  if (isRecord(value)) return [value];
  throw new ResultError(`expected rows for ${context}`, value);
}

/** Renders a scalar the way it should read in a table cell. */
export function stringify(value: unknown): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "bigint") return value.toString();
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  try {
    return JSON.stringify(value, (_key, inner) =>
      typeof inner === "bigint" ? inner.toString() : inner,
    );
  } catch {
    return String(value);
  }
}

/** Entity ids are i64; they are keyed as strings so bigint and number agree. */
function readId(value: unknown): string | null {
  if (typeof value === "bigint" || typeof value === "number") return value.toString();
  if (typeof value === "string" && value.length > 0) return value;
  return null;
}

function readLabel(value: unknown): string | null {
  return typeof value === "string" && value.length > 0 ? value : null;
}

function readGraphNode(row: Row): GraphNodeData | null {
  const id = readId(row[GRAPH_FIELDS.id] ?? row.$id ?? row.id);
  if (id === null) return null;
  const properties: Row = {};
  for (const [key, value] of Object.entries(row)) {
    if ([GRAPH_FIELDS.id, GRAPH_FIELDS.label, "$id", "$label"].includes(key)) continue;
    properties[key] = value;
  }
  return { id, label: readLabel(row[GRAPH_FIELDS.label] ?? row.$label ?? row.label), properties };
}

function readGraphEdge(row: Row): GraphEdgeData | null {
  const id = readId(row[GRAPH_FIELDS.id] ?? row.$id ?? row.id);
  const source = readId(row[GRAPH_FIELDS.source] ?? row.$from ?? row.from ?? row.source);
  const target = readId(row[GRAPH_FIELDS.target] ?? row.$to ?? row.to ?? row.target);
  if (id === null || source === null || target === null) return null;
  return { id, label: readLabel(row[GRAPH_FIELDS.label] ?? row.$label ?? row.label), source, target };
}

/**
 * Builds the drawable graph.
 *
 * Edges are restricted to the induced subgraph: HelixDB happily returns edges
 * that leave the selected node set, and drawing one would need a node that was
 * never fetched. The count of those is surfaced so the view can say so rather
 * than silently losing them.
 */
function readGraph(body: unknown, shape: Extract<ResultShape, { kind: "graph" }>): GraphData {
  const nodeRows = asRows(pickVariable(body, shape.nodeVariable), "graph nodes");
  const edgeRows = asRows(pickVariable(body, shape.edgeVariable), "graph edges");

  const nodes: GraphNodeData[] = [];
  const seen = new Set<string>();
  for (const row of nodeRows) {
    const node = readGraphNode(row);
    if (!node || seen.has(node.id)) continue;
    seen.add(node.id);
    nodes.push(node);
  }

  const edges: GraphEdgeData[] = [];
  const edgeIds = new Set<string>();
  let danglingEdges = 0;
  for (const row of edgeRows) {
    const edge = readGraphEdge(row);
    // Deduplicate before classifying, so an edge repeated in the response is
    // reported once rather than inflating the dropped-edge count.
    if (!edge || edgeIds.has(edge.id)) continue;
    edgeIds.add(edge.id);
    if (!seen.has(edge.source) || !seen.has(edge.target)) {
      danglingEdges++;
      continue;
    }
    edges.push(edge);
  }

  return {
    nodes,
    edges,
    danglingEdges,
    truncatedNodes: nodeRows.length >= shape.nodeLimit,
    truncatedEdges: edgeRows.length >= shape.edgeLimit,
  };
}

/**
 * Merges the identity companion variable into the `SELECT *` rows.
 *
 * A traversal has exactly one terminal, so `valueMap(null)` (all stored
 * properties) cannot also project `$id`/`$label`. The compiler runs the same
 * selection twice in one batch; the two results line up positionally. If they
 * ever do not, the properties are shown on their own rather than mislabelled.
 */
function mergeIdentity(rows: Row[], identity: Row[]): Row[] {
  if (identity.length !== rows.length) return rows;
  return rows.map((row, index) => {
    const merged: Row = {};
    for (const [key, value] of Object.entries(identity[index])) {
      // `$from.$id` reads better as `source` in a table header.
      const alias =
        key === "$id" ? "id" : key === "$label" ? "label" : key === "$from.$id" || key === "$from" ? "source" : key === "$to.$id" || key === "$to" ? "target" : key;
      merged[alias] = value;
    }
    // Identity wins: a stored property called `id` or `label` must not shadow
    // the entity's own, which is what the table keys rows by and what clicking
    // a row sends to DESCRIBE.
    const properties = Object.fromEntries(
      Object.entries(row).filter(([key]) => !["$id", "$label", "$from", "$to"].includes(key)),
    );
    return { ...properties, ...merged };
  });
}

function normalizeDisplayRow(row: Row): Row {
  const normalized: Row = {};
  for (const [key, value] of Object.entries(row)) {
    if (!["$id", "$label", "$from", "$to"].includes(key)) normalized[key] = value;
  }
  // Virtual entity identity always wins over same-named stored properties,
  // regardless of JSON object key order in the server response.
  if ("$id" in row) normalized.id = row.$id;
  if ("$label" in row) normalized.label = row.$label;
  if ("$from" in row) normalized.source = row.$from;
  if ("$to" in row) normalized.target = row.$to;
  return normalized;
}

/** Column order: identity first, then properties in first-seen order. */
function collectColumns(rows: Row[]): string[] {
  const seen = new Set<string>();
  for (const row of rows) for (const key of Object.keys(row)) seen.add(key);
  const leading = ["id", "label", "source", "target"].filter((key) => seen.has(key));
  const rest = [...seen].filter((key) => !leading.includes(key)).sort();
  return [...leading, ...rest];
}

/** Decodes a parsed response body according to the compiled result shape. */
export function readResult(body: unknown, shape: ResultShape): QueryResult {
  switch (shape.kind) {
    case "rows": {
      let rows = asRows(pickVariable(body, shape.variable), "rows").map(normalizeDisplayRow);
      if (shape.identityVariable) {
        rows = mergeIdentity(rows, asRows(pickVariable(body, shape.identityVariable), "identity"));
      }
      return { kind: "rows", columns: shape.columns ?? collectColumns(rows), rows };
    }

    case "count":
      return { kind: "count", value: readCount(pickVariable(body, shape.variable)) };

    case "groupCount":
      return {
        kind: "groupCount",
        by: shape.by,
        groups: readGroupCount(pickVariable(body, shape.variable)),
      };

    case "graph":
      return { kind: "graph", graph: readGraph(body, shape) };

    case "labels":
      return {
        kind: "labels",
        nodes: shape.variables.nodes
          ? readGroupCount(pickVariable(body, shape.variables.nodes))
          : null,
        edges: shape.variables.edges
          ? readGroupCount(pickVariable(body, shape.variables.edges))
          : null,
      };

    case "stats":
      return {
        kind: "stats",
        nodeCount: readCount(pickVariable(body, shape.variables.nodeCount)),
        edgeCount: readCount(pickVariable(body, shape.variables.edgeCount)),
        nodeLabels: readGroupCount(pickVariable(body, shape.variables.nodeLabels)),
        edgeLabels: readGroupCount(pickVariable(body, shape.variables.edgeLabels)),
      };

    case "describe": {
      const entity = asRows(pickVariable(body, shape.variables.entity), "entity")[0] ?? {};
      const properties = Object.fromEntries(
        Object.entries(entity).filter(([key]) => !["$id", "$label", "$from", "$to"].includes(key)),
      );
      const identity = asRows(pickVariable(body, shape.variables.identity), "identity")[0] ?? {};

      if (shape.entity === "edges") {
        const source = asRows(pickVariable(body, shape.variables.sourceNode), "source node")[0];
        const target = asRows(pickVariable(body, shape.variables.targetNode), "target node")[0];
        return {
          kind: "describe",
          entity: "edges",
          id: readId(identity["$id"]),
          label: readLabel(identity["$label"]),
          properties,
          edges: [],
          neighbours: [],
          degree: null,
          endpoints: {
            source: source ? readGraphNode(source) : null,
            target: target ? readGraphNode(target) : null,
          },
        };
      }

      const edges = asRows(pickVariable(body, shape.variables.edges), "incident edges")
        .map(readGraphEdge)
        .filter((edge): edge is GraphEdgeData => edge !== null);
      const neighbours = asRows(pickVariable(body, shape.variables.neighbours), "neighbours")
        .map(readGraphNode)
        .filter((node): node is GraphNodeData => node !== null);

      return {
        kind: "describe",
        entity: "nodes",
        id: readId(identity["$id"]),
        label: readLabel(identity["$label"]),
        properties,
        edges,
        neighbours,
        degree: readCount(pickVariable(body, shape.variables.degree)),
        endpoints: null,
      };
    }
  }
}
