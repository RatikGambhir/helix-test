/**
 * AST for HelixSQL — the SQL-flavoured surface language this app exposes.
 *
 * HelixDB itself has no SQL dialect; queries are built as a JSON traversal AST.
 * HelixSQL is a thin, read-only front end over that AST: every construct here
 * maps onto one or more traversal steps in `compiler.ts`. Nothing in this file
 * is HelixDB-specific, which keeps the parser testable on its own.
 */

/** A span into the source text, used to point at the offending token. */
export interface Span {
  start: number;
  end: number;
  line: number;
  column: number;
}

export type Literal =
  | { kind: "string"; value: string }
  | { kind: "number"; value: number }
  | { kind: "bigint"; value: bigint }
  | { kind: "boolean"; value: boolean }
  | { kind: "null" };

/** A column reference. `virtual` marks the reserved `$…` fields. */
export interface ColumnRef {
  /** As written by the user, e.g. `name`, `id`, `label`. */
  name: string;
  /** Resolved wire name: a property name, or `$id`/`$label`/`$from`/`$to`. */
  source: string;
  virtual: boolean;
  span: Span;
}

export type ComparisonOperator = "=" | "!=" | ">" | ">=" | "<" | "<=";

export type Condition =
  | { kind: "and"; parts: Condition[] }
  | { kind: "or"; parts: Condition[] }
  | { kind: "not"; part: Condition }
  | { kind: "compare"; column: ColumnRef; op: ComparisonOperator; value: Literal }
  | { kind: "between"; column: ColumnRef; low: Literal; high: Literal }
  | { kind: "in"; column: ColumnRef; values: Literal[]; negated: boolean }
  | { kind: "like"; column: ColumnRef; pattern: string; negated: boolean }
  | { kind: "isNull"; column: ColumnRef; negated: boolean }
  | { kind: "has"; column: ColumnRef };

/** `TRAVERSE <direction> [label] [WHERE …]` — one hop along the graph. */
export type HopDirection =
  /** Node -> neighbouring node across an outgoing / incoming / any edge. */
  | "out"
  | "in"
  | "both"
  /** Node -> the edges themselves, so you can read edge properties. */
  | "oute"
  | "ine"
  | "bothe"
  /** Edge -> its endpoints. */
  | "fromnode"
  | "tonode"
  | "othernode";

export interface Hop {
  direction: HopDirection;
  /** Edge label to restrict the hop to; `null` means any label. */
  label: string | null;
  where: Condition | null;
  span: Span;
}

export type EntityKind = "nodes" | "edges";

export interface Source {
  entity: EntityKind;
  /** Label written as `NODES:User`; also settable via `WHERE label = 'User'`. */
  label: string | null;
  span: Span;
}

export type Projection =
  | { kind: "star" }
  | { kind: "columns"; columns: ColumnRef[] }
  | { kind: "count" };

export interface OrderTerm {
  column: ColumnRef;
  descending: boolean;
}

/** The shared shape of a node/edge selection: source, filter, hops, paging. */
export interface Selection {
  source: Source;
  where: Condition | null;
  hops: Hop[];
  orderBy: OrderTerm[];
  skip: number | null;
  limit: number | null;
  distinct: boolean;
}

/** `SELECT … FROM …` — returns rows for the results table. */
export interface SelectStatement {
  kind: "select";
  projection: Projection;
  /** `GROUP BY <col>` with `COUNT(*)`, compiled to a groupCount step. */
  groupBy: ColumnRef | null;
  selection: Selection;
}

/** `GRAPH …` — returns nodes plus the edges among them, for the canvas. */
export interface GraphStatement {
  kind: "graph";
  selection: Selection;
  /** Node properties to fetch alongside `$id`/`$label`, for labelling. */
  withProperties: string[];
  /** Edge-label filter applied to the edges pulled in around the nodes. */
  edgeLabel: string | null;
  maxEdges: number | null;
}

/** `SHOW LABELS | NODE LABELS | EDGE LABELS | STATS` — schema discovery. */
export interface ShowStatement {
  kind: "show";
  target: "labels" | "nodeLabels" | "edgeLabels" | "stats";
  /** How many entities to sample when deriving labels. */
  sample: number;
}

/** `DESCRIBE NODE <id>` — one entity plus everything attached to it. */
export interface DescribeStatement {
  kind: "describe";
  entity: EntityKind;
  id: string;
  /** Cap on the neighbours pulled back for a node. */
  limit: number;
}

export type Statement =
  | SelectStatement
  | GraphStatement
  | ShowStatement
  | DescribeStatement;

/** A parse or compile failure carrying the span to underline in the editor. */
export class HqlError extends Error {
  readonly span: Span | null;
  /** Short suggestion shown under the message, when there is an obvious fix. */
  readonly hint: string | null;

  constructor(message: string, span: Span | null = null, hint: string | null = null) {
    super(message);
    this.name = "HqlError";
    this.span = span;
    this.hint = hint;
  }
}

/**
 * Reserved sources understood by HelixDB, keyed by the SQL-side column name.
 *
 * The edge endpoints are exposed as `source`/`target` rather than `from`/`to`:
 * a bare `from` in a select list is indistinguishable from the FROM clause.
 * `$from.$id` and `$to.$id` can still be written out in full.
 */
export const VIRTUAL_COLUMNS: Readonly<Record<string, string>> = Object.freeze({
  id: "$id",
  label: "$label",
  source: "$from.$id",
  target: "$to.$id",
  score: "$score",
  distance: "$distance",
});
