/**
 * Compiles a parsed HelixSQL statement into a HelixDB query request.
 *
 * Everything here goes through `@helix-db/helix-db`, the official SDK, so the
 * JSON that reaches the server is byte-identical to what the Rust/Go/Python
 * SDKs emit. The app never hand-rolls the wire format.
 */
import {
  EdgeRef,
  NodeRef,
  Order,
  Predicate,
  Projection as HelixProjection,
  PropertyValue,
  QueryRequest,
  parseJson,
  readBatch,
  stringifyJson,
  g,
  type ReadBatch,
  type Traversal,
} from "@helix-db/helix-db";

import {
  HqlError,
  type ColumnRef,
  type Condition,
  type DescribeStatement,
  type GraphStatement,
  type Hop,
  type Literal,
  type Selection,
  type SelectStatement,
  type ShowStatement,
  type Statement,
} from "./ast";

/** Anything the app is willing to pull back without an explicit LIMIT. */
export const DEFAULT_ROW_LIMIT = 500;
export const DEFAULT_GRAPH_NODE_LIMIT = 400;
export const DEFAULT_GRAPH_EDGE_LIMIT = 2000;

/** Wire aliases for the graph view. Kept short — they travel on every row. */
export const GRAPH_FIELDS = {
  id: "_id",
  label: "_label",
  source: "_src",
  target: "_dst",
} as const;

/** Tells the UI how to read the response for a given compiled statement. */
export type ResultShape =
  | {
      kind: "rows";
      variable: string;
      /** `null` for `SELECT *`, where the columns are whatever came back. */
      columns: string[] | null;
      /**
       * Companion variable holding `$id`/`$label` for the same rows, in the
       * same order. `valueMap(null)` returns stored properties only, and a
       * traversal has exactly one terminal, so identity is fetched alongside.
       */
      identityVariable?: string;
    }
  | { kind: "count"; variable: string }
  | { kind: "groupCount"; variable: string; by: string }
  | { kind: "graph"; nodeVariable: string; edgeVariable: string; nodeLimit: number; edgeLimit: number }
  | { kind: "labels"; variables: { nodes?: string; edges?: string } }
  | { kind: "stats"; variables: Record<string, string> }
  | { kind: "describe"; entity: "nodes" | "edges"; variables: Record<string, string> };

export interface CompiledQuery {
  request: QueryRequest;
  /** Pretty-printed JSON of the request, for the "wire format" inspector. */
  json: string;
  shape: ResultShape;
  /** Human-readable summary shown next to the results. */
  summary: string;
}

// A read traversal at any point in its lifecycle. The SDK's phantom state
// parameter is there to catch nonsense at author time; we validate dynamically
// against the parsed statement instead, so this stays deliberately loose.
type AnyTraversal = Traversal<any, "read">;

// ---------------------------------------------------------------------------
// Literals and predicates
// ---------------------------------------------------------------------------

function literalToValue(literal: Literal, column: ColumnRef): PropertyValue {
  switch (literal.kind) {
    case "string":
      return PropertyValue.string(literal.value);
    case "number":
      return Number.isInteger(literal.value)
        ? PropertyValue.i64(literal.value)
        : PropertyValue.f64(literal.value);
    case "bigint":
      return PropertyValue.i64(literal.value);
    case "boolean":
      return PropertyValue.bool(literal.value);
    case "null":
      if (column.virtual) {
        throw new HqlError(`${column.name} is never null`, column.span);
      }
      return PropertyValue.null();
  }
}

/** Splits a LIKE pattern into the prefix/suffix/substring form Helix supports. */
function compileLike(column: ColumnRef, pattern: string): Predicate {
  if (pattern.includes("_")) {
    throw new HqlError(
      "LIKE does not support the single-character wildcard _",
      column.span,
      "use % for a run of characters",
    );
  }
  const starts = pattern.startsWith("%");
  const ends = pattern.endsWith("%");
  const core = pattern.slice(starts ? 1 : 0, ends ? pattern.length - 1 : undefined);

  if (core.includes("%")) {
    throw new HqlError(
      "LIKE only supports % at the start and/or end of the pattern",
      column.span,
      "for example 'ali%', '%son' or '%li%'",
    );
  }
  if (starts && ends) return Predicate.contains(column.source, core);
  if (ends) return Predicate.startsWith(column.source, core);
  if (starts) return Predicate.endsWith(column.source, core);
  // No wildcard at all behaves like equality, as it does in SQL.
  return Predicate.eq(column.source, PropertyValue.string(core));
}

function compileCondition(condition: Condition): Predicate {
  switch (condition.kind) {
    case "and":
      return Predicate.and(condition.parts.map(compileCondition));
    case "or":
      return Predicate.or(condition.parts.map(compileCondition));
    case "not":
      return Predicate.not(compileCondition(condition.part));

    case "compare": {
      const { column, op, value } = condition;
      if (value.kind === "null") {
        if (op === "=") return Predicate.isNull(column.source);
        if (op === "!=") return Predicate.isNotNull(column.source);
        throw new HqlError(`cannot use ${op} with NULL`, column.span, "use IS NULL / IS NOT NULL");
      }
      const operand = literalToValue(value, column);
      switch (op) {
        case "=":
          return Predicate.eq(column.source, operand);
        case "!=":
          return Predicate.neq(column.source, operand);
        case ">":
          return Predicate.gt(column.source, operand);
        case ">=":
          return Predicate.gte(column.source, operand);
        case "<":
          return Predicate.lt(column.source, operand);
        case "<=":
          return Predicate.lte(column.source, operand);
      }
      break;
    }

    case "between":
      return Predicate.between(
        condition.column.source,
        literalToValue(condition.low, condition.column),
        literalToValue(condition.high, condition.column),
      );

    case "in": {
      const values = condition.values.map((value) => literalToValue(value, condition.column));
      const predicate = Predicate.isIn(condition.column.source, PropertyValue.array(values));
      return condition.negated ? Predicate.not(predicate) : predicate;
    }

    case "like": {
      const predicate = compileLike(condition.column, condition.pattern);
      return condition.negated ? Predicate.not(predicate) : predicate;
    }

    case "isNull":
      return condition.negated
        ? Predicate.isNotNull(condition.column.source)
        : Predicate.isNull(condition.column.source);

    case "has":
      return Predicate.hasKey(condition.column.source);
  }
  // Unreachable for a well-formed AST; keeps the switch exhaustive for TS.
  throw new HqlError("unsupported condition");
}

/**
 * Pulls `label = 'X'` out of the top level of a WHERE clause.
 *
 * A label is not a stored property, so it is matched with a dedicated
 * `hasLabel` step rather than a property predicate — that is also what lets
 * HelixDB use its label index instead of scanning.
 */
function extractLabelFilter(condition: Condition | null): {
  labels: string[];
  rest: Condition | null;
} {
  if (!condition) return { labels: [], rest: null };

  const isLabelEquality = (part: Condition): string | null =>
    part.kind === "compare" &&
    part.column.source === "$label" &&
    part.op === "=" &&
    part.value.kind === "string"
      ? part.value.value
      : null;

  const direct = isLabelEquality(condition);
  if (direct !== null) return { labels: [direct], rest: null };

  if (condition.kind === "and") {
    const labels: string[] = [];
    const remaining: Condition[] = [];
    for (const part of condition.parts) {
      const label = isLabelEquality(part);
      if (label !== null) labels.push(label);
      else remaining.push(part);
    }
    if (labels.length === 0) return { labels: [], rest: condition };
    const rest =
      remaining.length === 0
        ? null
        : remaining.length === 1
          ? remaining[0]
          : { kind: "and" as const, parts: remaining };
    return { labels, rest };
  }

  return { labels: [], rest: condition };
}

// ---------------------------------------------------------------------------
// Selections
// ---------------------------------------------------------------------------

function applyHop(traversal: AnyTraversal, hop: Hop): AnyTraversal {
  let next: AnyTraversal;
  switch (hop.direction) {
    case "out":
      next = traversal.out(hop.label);
      break;
    case "in":
      next = traversal.in(hop.label);
      break;
    case "both":
      next = traversal.both(hop.label);
      break;
    case "oute":
      next = traversal.outE(hop.label);
      break;
    case "ine":
      next = traversal.inE(hop.label);
      break;
    case "bothe":
      next = traversal.bothE(hop.label);
      break;
    case "fromnode":
      next = traversal.outN();
      break;
    case "tonode":
      next = traversal.inN();
      break;
    case "othernode":
      next = traversal.otherN();
      break;
  }

  if (hop.where) {
    const { labels, rest } = extractLabelFilter(hop.where);
    for (const label of labels) next = next.hasLabel(label);
    if (rest) next = next.where(compileCondition(rest));
  }
  return next;
}

const HOPS_FROM_NODES = new Set(["out", "in", "both", "oute", "ine", "bothe"]);
const HOPS_LANDING_ON_EDGES = new Set(["oute", "ine", "bothe"]);

/**
 * Walks the hops, checking each one starts from the right kind of entity and
 * reporting where the traversal ends up.
 *
 * Getting this wrong is the easiest mistake to make in the language, and the
 * server-side error for it is opaque, so it is caught here with the span of the
 * offending clause.
 */
function landsOnEdges(selection: Selection): boolean {
  let onEdges = selection.source.entity === "edges";
  for (const hop of selection.hops) {
    const startsFromNodes = HOPS_FROM_NODES.has(hop.direction);
    if (startsFromNodes && onEdges) {
      throw new HqlError(
        `TRAVERSE ${hop.direction.toUpperCase()} starts from a node, but the query is on edges here`,
        hop.span,
        "step back onto nodes first with TRAVERSE SOURCE, TARGET or OTHER",
      );
    }
    if (!startsFromNodes && !onEdges) {
      throw new HqlError(
        `TRAVERSE ${hop.direction === "fromnode" ? "SOURCE" : hop.direction === "tonode" ? "TARGET" : "OTHER"} starts from an edge, but the query is on nodes here`,
        hop.span,
        "reach the edges first with TRAVERSE OUT EDGES / IN EDGES / BOTH EDGES",
      );
    }
    onEdges = HOPS_LANDING_ON_EDGES.has(hop.direction);
  }
  return onEdges;
}

interface SelectionOptions {
  /** Limit to apply when the statement did not specify one. */
  defaultLimit: number | null;
  /** Skip the paging steps — used when the caller appends its own. */
  omitPaging?: boolean;
}

function buildSelection(selection: Selection, options: SelectionOptions): AnyTraversal {
  const { labels, rest } = extractLabelFilter(selection.where);
  const sourceLabels = selection.source.label ? [selection.source.label, ...labels] : labels;

  let traversal: AnyTraversal;
  if (selection.source.entity === "nodes") {
    traversal = sourceLabels.length > 0 ? g().nWithLabel(sourceLabels[0]) : g().n(NodeRef.all());
  } else {
    traversal = sourceLabels.length > 0 ? g().eWithLabel(sourceLabels[0]) : g().e(EdgeRef.all());
  }
  // A second label on the same entity can never match, but the parser allows
  // it to be written; a redundant `hasLabel` keeps the semantics honest.
  for (const label of sourceLabels.slice(1)) traversal = traversal.hasLabel(label);

  if (rest) traversal = traversal.where(compileCondition(rest));
  for (const hop of selection.hops) traversal = applyHop(traversal, hop);
  if (selection.distinct) traversal = traversal.dedup();

  for (const term of selection.orderBy) {
    traversal = traversal.orderBy(term.column.source, term.descending ? Order.Desc : Order.Asc);
  }

  if (options.omitPaging) return traversal;

  if (selection.skip !== null) traversal = traversal.skip(selection.skip);
  const limit = selection.limit ?? options.defaultLimit;
  if (limit !== null) traversal = traversal.limit(limit);
  return traversal;
}

function describeSelection(selection: Selection): string {
  const parts: string[] = [];
  const label = selection.source.label ? `:${selection.source.label}` : "";
  parts.push(`${selection.source.entity}${label}`);
  for (const hop of selection.hops) {
    parts.push(`${hop.direction}${hop.label ? `(${hop.label})` : ""}`);
  }
  return parts.join(" → ");
}

// ---------------------------------------------------------------------------
// Statement compilation
// ---------------------------------------------------------------------------

function finish(batch: ReadBatch, shape: ResultShape, summary: string, name: string): CompiledQuery {
  const request = batch.toQueryRequest({ queryName: name });
  return {
    request,
    // Round-tripping through the SDK's own JSON helpers rather than the global
    // `JSON` keeps i64 values exact — `JSON.parse` would quietly round an id
    // like 9223372036854775807 down to the nearest double.
    json: stringifyJson(parseJson(request.toJsonString()), true),
    shape,
    summary,
  };
}

function compileSelect(statement: SelectStatement): CompiledQuery {
  const { selection, projection, groupBy } = statement;
  const onEdges = landsOnEdges(selection);

  if (projection.kind === "count") {
    // COUNT(*) and GROUP BY are terminals: paging them makes no sense, so the
    // selection is built without a default limit.
    const base = buildSelection(
      { ...selection, limit: selection.limit, orderBy: [] },
      { defaultLimit: null },
    );
    if (groupBy) {
      const batch = readBatch()
        .varAs("rows", base.groupCount(groupBy.source))
        .returning(["rows"]);
      return finish(
        batch,
        { kind: "groupCount", variable: "rows", by: groupBy.name },
        `count of ${describeSelection(selection)} grouped by ${groupBy.name}`,
        "hql_group_count",
      );
    }
    const batch = readBatch().varAs("rows", base.count()).returning(["rows"]);
    return finish(
      batch,
      { kind: "count", variable: "rows" },
      `count of ${describeSelection(selection)}`,
      "hql_count",
    );
  }

  const base = buildSelection(selection, { defaultLimit: DEFAULT_ROW_LIMIT });

  if (projection.kind === "star") {
    // `valueMap` with an explicit list is the only way to get the reserved
    // fields alongside the stored properties, so `*` asks for every stored
    // property (null) and the identity fields are added by a second pass below.
    const identity = onEdges
      ? ["$id", "$label", "$from.$id", "$to.$id"]
      : ["$id", "$label"];
    const batch = readBatch()
      .varAs("rows", base.valueMap(null))
      .varAs("identity", buildSelection(selection, { defaultLimit: DEFAULT_ROW_LIMIT }).valueMap(identity))
      .returning(["rows", "identity"]);
    return finish(
      batch,
      { kind: "rows", variable: "rows", columns: null, identityVariable: "identity" },
      `all properties of ${describeSelection(selection)}`,
      "hql_select_star",
    );
  }

  const columns = projection.columns.map((column) => column.source);
  const batch = readBatch()
    .varAs(
      "rows",
      base.project(
        projection.columns.map((column) => HelixProjection.property(column.source, column.name)),
      ),
    )
    .returning(["rows"]);
  return finish(
    batch,
    { kind: "rows", variable: "rows", columns: projection.columns.map((c) => c.name) },
    `${columns.length} column${columns.length === 1 ? "" : "s"} of ${describeSelection(selection)}`,
    "hql_select",
  );
}

function compileGraph(statement: GraphStatement): CompiledQuery {
  const { selection, withProperties, edgeLabel, maxEdges } = statement;
  landsOnEdges(selection); // validates the hop chain; GRAPH always ends on nodes
  const nodeLimit = selection.limit ?? DEFAULT_GRAPH_NODE_LIMIT;
  const edgeLimit = maxEdges ?? DEFAULT_GRAPH_EDGE_LIMIT;

  const nodeFields = [
    HelixProjection.property("$id", GRAPH_FIELDS.id),
    HelixProjection.property("$label", GRAPH_FIELDS.label),
    ...withProperties.map((property) => HelixProjection.property(property, property)),
  ];
  const edgeFields = [
    HelixProjection.property("$id", GRAPH_FIELDS.id),
    HelixProjection.property("$label", GRAPH_FIELDS.label),
    HelixProjection.fromEndpoint("$id", GRAPH_FIELDS.source),
    HelixProjection.toEndpoint("$id", GRAPH_FIELDS.target),
  ];

  const nodes = buildSelection({ ...selection, limit: nodeLimit }, { defaultLimit: nodeLimit });

  // Edges are always fanned out from the selected nodes. Reading the edge table
  // directly is cheaper, but the node set is capped independently — by LIMIT or
  // by DEFAULT_GRAPH_NODE_LIMIT — so on any graph larger than that cap the two
  // scans return unrelated slices and almost every fetched edge is discarded as
  // dangling. Fanning out cannot lose a drawable edge: an edge with neither
  // endpoint in the node set was never drawable to begin with.
  const edgeSelection: AnyTraversal = buildSelection(
    { ...selection, limit: nodeLimit },
    { defaultLimit: nodeLimit },
  )
    .bothE(edgeLabel)
    .dedup();

  const batch = readBatch()
    .varAs("nodes", nodes.project(nodeFields))
    .varAs("edges", edgeSelection.limit(edgeLimit).project(edgeFields))
    .returning(["nodes", "edges"]);

  return finish(
    batch,
    {
      kind: "graph",
      nodeVariable: "nodes",
      edgeVariable: "edges",
      nodeLimit,
      edgeLimit,
    },
    `graph of ${describeSelection(selection)} (≤${nodeLimit} nodes, ≤${edgeLimit} edges)`,
    "hql_graph",
  );
}

function compileShow(statement: ShowStatement): CompiledQuery {
  const sample = Math.max(1, statement.sample);

  if (statement.target === "stats") {
    const batch = readBatch()
      .varAs("nodeCount", g().n(NodeRef.all()).count())
      .varAs("edgeCount", g().e(EdgeRef.all()).count())
      .varAs("nodeLabels", g().n(NodeRef.all()).limit(sample).groupCount("$label"))
      .varAs("edgeLabels", g().e(EdgeRef.all()).limit(sample).groupCount("$label"))
      .returning(["nodeCount", "edgeCount", "nodeLabels", "edgeLabels"]);
    return finish(
      batch,
      {
        kind: "stats",
        variables: {
          nodeCount: "nodeCount",
          edgeCount: "edgeCount",
          nodeLabels: "nodeLabels",
          edgeLabels: "edgeLabels",
        },
      },
      `instance totals, with labels sampled over ${sample} entities`,
      "hql_stats",
    );
  }

  // HelixDB has no catalog to read, so labels are derived by counting the
  // `$label` of a bounded sample. The UI surfaces the sample size for honesty.
  const wantNodes = statement.target !== "edgeLabels";
  const wantEdges = statement.target !== "nodeLabels";

  let batch = readBatch();
  const returns: string[] = [];
  const variables: { nodes?: string; edges?: string } = {};
  if (wantNodes) {
    batch = batch.varAs("nodeLabels", g().n(NodeRef.all()).limit(sample).groupCount("$label"));
    returns.push("nodeLabels");
    variables.nodes = "nodeLabels";
  }
  if (wantEdges) {
    batch = batch.varAs("edgeLabels", g().e(EdgeRef.all()).limit(sample).groupCount("$label"));
    returns.push("edgeLabels");
    variables.edges = "edgeLabels";
  }

  return finish(
    batch.returning(returns),
    { kind: "labels", variables },
    `labels seen across a sample of ${sample} entities`,
    "hql_labels",
  );
}

const endpointFields = () => [
  HelixProjection.property("$id", GRAPH_FIELDS.id),
  HelixProjection.property("$label", GRAPH_FIELDS.label),
];

function compileDescribe(statement: DescribeStatement): CompiledQuery {
  const id = BigInt(statement.id);
  const limit = Math.max(1, statement.limit);

  if (statement.entity === "edges") {
    const batch = readBatch()
      .varAs("entity", g().e(EdgeRef.id(id)).valueMap(null))
      .varAs("identity", g().e(EdgeRef.id(id)).valueMap(["$id", "$label", "$from.$id", "$to.$id"]))
      .varAs("sourceNode", g().e(EdgeRef.id(id)).outN().project(endpointFields()))
      .varAs("targetNode", g().e(EdgeRef.id(id)).inN().project(endpointFields()))
      .returning(["entity", "identity", "sourceNode", "targetNode"]);
    return finish(
      batch,
      {
        kind: "describe",
        entity: "edges",
        variables: {
          entity: "entity",
          identity: "identity",
          sourceNode: "sourceNode",
          targetNode: "targetNode",
        },
      },
      `edge ${statement.id} with its endpoints`,
      "hql_describe_edge",
    );
  }

  const incident = () => g().n(NodeRef.id(id)).bothE(null);
  const batch = readBatch()
    .varAs("entity", g().n(NodeRef.id(id)).valueMap(null))
    .varAs("identity", g().n(NodeRef.id(id)).valueMap(["$id", "$label"]))
    .varAs(
      "edges",
      incident()
        .limit(limit)
        .project([
          HelixProjection.property("$id", GRAPH_FIELDS.id),
          HelixProjection.property("$label", GRAPH_FIELDS.label),
          HelixProjection.fromEndpoint("$id", GRAPH_FIELDS.source),
          HelixProjection.toEndpoint("$id", GRAPH_FIELDS.target),
        ]),
    )
    .varAs(
      "neighbours",
      g()
        .n(NodeRef.id(id))
        .both(null)
        .dedup()
        .limit(limit)
        .project(endpointFields()),
    )
    .varAs("degree", incident().count())
    .returning(["entity", "identity", "edges", "neighbours", "degree"]);

  return finish(
    batch,
    {
      kind: "describe",
      entity: "nodes",
      variables: {
        entity: "entity",
        identity: "identity",
        edges: "edges",
        neighbours: "neighbours",
        degree: "degree",
      },
    },
    `node ${statement.id} with up to ${limit} incident edges`,
    "hql_describe_node",
  );
}

/** Compiles HelixSQL to the official SDK representation used for validation and mock tests. */
export function compile(statement: Statement): CompiledQuery {
  switch (statement.kind) {
    case "select":
      return compileSelect(statement);
    case "graph":
      return compileGraph(statement);
    case "show":
      return compileShow(statement);
    case "describe":
      return compileDescribe(statement);
  }
}
