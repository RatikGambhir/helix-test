import {
  HqlError,
  VIRTUAL_COLUMNS,
  type ColumnRef,
  type ComparisonOperator,
  type Condition,
  type DescribeStatement,
  type EntityKind,
  type GraphStatement,
  type Hop,
  type HopDirection,
  type Literal,
  type OrderTerm,
  type Projection,
  type Selection,
  type SelectStatement,
  type ShowStatement,
  type Source,
  type Span,
  type Statement,
} from "./ast";
import { tokenize, type Token } from "./lexer";

/** Words the parser treats as syntax; a label spelled the same must be quoted. */
export const KEYWORDS = new Set([
  "SELECT", "DISTINCT", "FROM", "WHERE", "TRAVERSE", "ORDER", "BY", "ASC", "DESC",
  "LIMIT", "SKIP", "OFFSET", "GROUP", "AND", "OR", "NOT", "IN", "IS", "NULL",
  "LIKE", "BETWEEN", "HAS", "COUNT", "TRUE", "FALSE", "NODES", "EDGES", "NODE",
  "EDGE", "GRAPH", "SHOW", "DESCRIBE", "DESC_", "LABELS", "STATS", "SAMPLE",
  "WITH", "VIA", "OUT", "BOTH", "SOURCE", "TARGET", "OTHER",
]);

class Parser {
  private readonly tokens: Token[];
  private position = 0;

  constructor(source: string) {
    this.tokens = tokenize(source);
  }

  // ---- token helpers -------------------------------------------------------

  private peek(offset = 0): Token {
    return this.tokens[Math.min(this.position + offset, this.tokens.length - 1)];
  }

  private next(): Token {
    const token = this.peek();
    if (token.kind !== "eof") this.position++;
    return token;
  }

  private atEnd(): boolean {
    return this.peek().kind === "eof";
  }

  /** True when the next token is the given keyword (never a quoted word). */
  private isKeyword(word: string, offset = 0): boolean {
    const token = this.peek(offset);
    return token.kind === "word" && !token.quoted && token.upper === word;
  }

  private takeKeyword(word: string): boolean {
    if (!this.isKeyword(word)) return false;
    this.position++;
    return true;
  }

  private expectKeyword(word: string, context: string): Token {
    if (!this.isKeyword(word)) {
      throw this.errorAt(this.peek(), `expected ${word} ${context}`);
    }
    return this.next();
  }

  private takePunct(char: string): boolean {
    const token = this.peek();
    if (token.kind === "punct" && token.text === char) {
      this.position++;
      return true;
    }
    return false;
  }

  private expectPunct(char: string, context: string): Token {
    if (!this.takePunct(char)) {
      throw this.errorAt(this.peek(), `expected ${JSON.stringify(char)} ${context}`);
    }
    return this.tokens[this.position - 1];
  }

  private errorAt(token: Token, message: string, hint: string | null = null): HqlError {
    const found = token.kind === "eof" ? "end of query" : JSON.stringify(token.text);
    return new HqlError(`${message}, found ${found}`, token.span, hint);
  }

  // ---- entry point ---------------------------------------------------------

  parse(): Statement {
    if (this.atEnd()) {
      throw new HqlError("empty query", this.peek().span, "try: SELECT * FROM NODES LIMIT 25");
    }

    let statement: Statement;
    if (this.isKeyword("SELECT")) statement = this.parseSelect();
    else if (this.isKeyword("GRAPH")) statement = this.parseGraph();
    else if (this.isKeyword("SHOW")) statement = this.parseShow();
    else if (this.isKeyword("DESCRIBE")) statement = this.parseDescribe();
    else {
      throw this.errorAt(
        this.peek(),
        "expected a statement (SELECT, GRAPH, SHOW or DESCRIBE)",
        "HelixSQL is read-only; writes must go through a HelixDB SDK",
      );
    }

    this.takePunct(";");
    if (!this.atEnd()) {
      throw this.errorAt(this.peek(), "unexpected trailing input", "only one statement per query");
    }
    return statement;
  }

  // ---- SELECT --------------------------------------------------------------

  private parseSelect(): SelectStatement {
    this.expectKeyword("SELECT", "at the start of the statement");
    const distinct = this.takeKeyword("DISTINCT");
    const projection = this.parseProjection();

    this.expectKeyword("FROM", "after the select list");
    const source = this.parseSource();
    const where = this.takeKeyword("WHERE") ? this.parseCondition() : null;
    const hops = this.parseHops();

    let groupBy: ColumnRef | null = null;
    if (this.takeKeyword("GROUP")) {
      this.expectKeyword("BY", "after GROUP");
      groupBy = this.parseColumn();
    }

    const orderBy = this.parseOrderBy();
    const { skip, limit } = this.parsePaging();

    if (groupBy && projection.kind !== "count") {
      throw new HqlError(
        "GROUP BY is only supported with COUNT(*)",
        groupBy.span,
        `try: SELECT COUNT(*) FROM ${source.entity.toUpperCase()} GROUP BY ${groupBy.name}`,
      );
    }
    if (projection.kind === "count" && orderBy.length > 0) {
      throw new HqlError(
        "ORDER BY cannot be combined with COUNT(*)",
        orderBy[0].column.span,
        "a count returns a single row",
      );
    }

    return {
      kind: "select",
      projection,
      groupBy,
      selection: { source, where, hops, orderBy, skip, limit, distinct },
    };
  }

  private parseProjection(): Projection {
    if (this.takePunct("*")) return { kind: "star" };

    if (this.isKeyword("COUNT") && this.peek(1).kind === "punct" && this.peek(1).text === "(") {
      this.next();
      this.expectPunct("(", "after COUNT");
      if (!this.takePunct("*")) {
        throw this.errorAt(
          this.peek(),
          "expected * inside COUNT",
          "only COUNT(*) is supported; use GROUP BY to break the count down",
        );
      }
      this.expectPunct(")", "to close COUNT(*)");
      return { kind: "count" };
    }

    const columns: ColumnRef[] = [this.parseColumn()];
    while (this.takePunct(",")) columns.push(this.parseColumn());
    return { kind: "columns", columns };
  }

  private parseColumn(): ColumnRef {
    const token = this.peek();
    if (token.kind !== "word") {
      throw this.errorAt(token, "expected a column name");
    }
    if (!token.quoted && KEYWORDS.has(token.upper) && !isColumnSafeKeyword(token.upper)) {
      const hint =
        token.upper === "FROM" || token.upper === "TO"
          ? "use source / target for an edge's endpoints"
          : `quote it as "${token.text}" to use it as a property name`;
      throw this.errorAt(token, `expected a column name but ${token.text.toUpperCase()} is a keyword`, hint);
    }
    this.next();

    // Reserved sources can be written in their dotted form, e.g. `$from.$id`.
    if (!token.quoted && token.text.startsWith("$")) {
      let name = token.text;
      while (this.peek().kind === "punct" && this.peek().text === "." && this.peek(1).kind === "word") {
        this.next();
        name += `.${this.next().text}`;
      }
      const span = { ...token.span, end: this.tokens[this.position - 1].span.end };
      return { name, source: name, virtual: true, span };
    }

    return makeColumn(token.text, token.span, token.quoted === true);
  }

  // ---- FROM ----------------------------------------------------------------

  private parseSource(): Source {
    const token = this.peek();
    let entity: EntityKind;
    if (this.isKeyword("NODES") || this.isKeyword("NODE")) entity = "nodes";
    else if (this.isKeyword("EDGES") || this.isKeyword("EDGE")) entity = "edges";
    else {
      throw this.errorAt(
        token,
        "expected NODES or EDGES",
        "try: SELECT * FROM NODES:User LIMIT 25",
      );
    }
    this.next();

    // `NODES:User` and `NODES User` both narrow to a label.
    let label: string | null = null;
    if (this.takePunct(":")) {
      label = this.parseLabel("after ':'");
    } else if (this.isLabelToken()) {
      label = this.parseLabel("after the source");
    }

    return { entity, label, span: { ...token.span, end: this.tokens[this.position - 1].span.end } };
  }

  /** A bare word or string that can only be a label in this position. */
  private isLabelToken(): boolean {
    const token = this.peek();
    if (token.kind === "string") return true;
    if (token.kind !== "word") return false;
    return token.quoted === true || !KEYWORDS.has(token.upper);
  }

  private parseLabel(context: string): string {
    const token = this.peek();
    if (token.kind === "string" || token.kind === "word") {
      if (token.kind === "word" && !token.quoted && KEYWORDS.has(token.upper)) {
        throw this.errorAt(
          token,
          `expected a label ${context} but ${token.text.toUpperCase()} is a keyword`,
          `quote it as "${token.text}"`,
        );
      }
      this.next();
      return token.text;
    }
    throw this.errorAt(token, `expected a label ${context}`);
  }

  // ---- TRAVERSE ------------------------------------------------------------

  private parseHops(): Hop[] {
    const hops: Hop[] = [];
    while (this.isKeyword("TRAVERSE")) {
      const start = this.next().span;
      const direction = this.parseHopDirection();
      const endpointHop = direction === "fromnode" || direction === "tonode" || direction === "othernode";
      const label = !endpointHop && this.isLabelToken() ? this.parseLabel("after the direction") : null;
      const where = this.takeKeyword("WHERE") ? this.parseCondition() : null;
      hops.push({
        direction,
        label,
        where,
        span: { ...start, end: this.tokens[this.position - 1].span.end },
      });
    }
    return hops;
  }

  private parseHopDirection(): HopDirection {
    const token = this.peek();
    if (token.kind !== "word" || token.quoted) {
      throw this.errorAt(token, "expected a traversal direction", DIRECTION_HINT);
    }

    // OUT / IN / BOTH optionally followed by EDGES to stop on the edges.
    if (token.upper === "OUT" || token.upper === "IN" || token.upper === "BOTH") {
      this.next();
      const toEdges = this.isKeyword("EDGES") || this.isKeyword("EDGE");
      if (toEdges) this.next();
      if (token.upper === "OUT") return toEdges ? "oute" : "out";
      if (token.upper === "IN") return toEdges ? "ine" : "in";
      return toEdges ? "bothe" : "both";
    }

    if (token.upper === "SOURCE") {
      this.next();
      return "fromnode";
    }
    if (token.upper === "TARGET") {
      this.next();
      return "tonode";
    }
    if (token.upper === "OTHER") {
      this.next();
      return "othernode";
    }

    throw this.errorAt(token, "expected a traversal direction", DIRECTION_HINT);
  }

  // ---- ORDER BY / paging ---------------------------------------------------

  private parseOrderBy(): OrderTerm[] {
    if (!this.takeKeyword("ORDER")) return [];
    this.expectKeyword("BY", "after ORDER");

    const terms: OrderTerm[] = [];
    do {
      const column = this.parseColumn();
      let descending = false;
      if (this.takeKeyword("DESC")) descending = true;
      else this.takeKeyword("ASC");
      terms.push({ column, descending });
    } while (this.takePunct(","));
    return terms;
  }

  private parsePaging(): { skip: number | null; limit: number | null } {
    let skip: number | null = null;
    let limit: number | null = null;

    // Accept the clauses in either order, the way most SQL dialects do.
    for (let guard = 0; guard < 2; guard++) {
      if ((this.isKeyword("SKIP") || this.isKeyword("OFFSET")) && skip === null) {
        const keyword = this.next();
        skip = this.parseCount(keyword.upper);
        continue;
      }
      if (this.isKeyword("LIMIT") && limit === null) {
        this.next();
        limit = this.parseCount("LIMIT");
        continue;
      }
      break;
    }
    return { skip, limit };
  }

  private parseCount(keyword: string): number {
    const token = this.peek();
    if (token.kind !== "number" || typeof token.numeric !== "number" || !Number.isInteger(token.numeric)) {
      throw this.errorAt(token, `expected a whole number after ${keyword}`);
    }
    if (token.numeric < 0) {
      throw this.errorAt(token, `${keyword} cannot be negative`);
    }
    this.next();
    return token.numeric;
  }

  // ---- WHERE ---------------------------------------------------------------

  private parseCondition(): Condition {
    return this.parseOr();
  }

  private parseOr(): Condition {
    const parts = [this.parseAnd()];
    while (this.takeKeyword("OR")) parts.push(this.parseAnd());
    return parts.length === 1 ? parts[0] : { kind: "or", parts };
  }

  private parseAnd(): Condition {
    const parts = [this.parseNot()];
    while (this.takeKeyword("AND")) parts.push(this.parseNot());
    return parts.length === 1 ? parts[0] : { kind: "and", parts };
  }

  private parseNot(): Condition {
    if (this.takeKeyword("NOT")) return { kind: "not", part: this.parseNot() };
    return this.parsePrimaryCondition();
  }

  private parsePrimaryCondition(): Condition {
    if (this.takePunct("(")) {
      const inner = this.parseCondition();
      this.expectPunct(")", "to close the group");
      return inner;
    }

    // HAS(prop) — true when the property exists on the entity at all.
    if (this.isKeyword("HAS") && this.peek(1).kind === "punct" && this.peek(1).text === "(") {
      this.next();
      this.expectPunct("(", "after HAS");
      const column = this.parseColumn();
      this.expectPunct(")", "to close HAS(...)");
      return { kind: "has", column };
    }

    const column = this.parseColumn();

    if (this.takeKeyword("IS")) {
      const negated = this.takeKeyword("NOT");
      this.expectKeyword("NULL", "after IS");
      return { kind: "isNull", column, negated };
    }

    const negated = this.takeKeyword("NOT");

    if (this.takeKeyword("IN")) {
      this.expectPunct("(", "after IN");
      const values: Literal[] = [];
      if (!this.takePunct(")")) {
        do {
          values.push(this.parseLiteral());
        } while (this.takePunct(","));
        this.expectPunct(")", "to close the IN list");
      }
      if (values.length === 0) {
        throw new HqlError("IN needs at least one value", column.span);
      }
      return { kind: "in", column, values, negated };
    }

    if (this.takeKeyword("LIKE")) {
      const token = this.peek();
      if (token.kind !== "string") {
        throw this.errorAt(token, "expected a quoted pattern after LIKE", "for example: LIKE 'ali%'");
      }
      this.next();
      return { kind: "like", column, pattern: token.text, negated };
    }

    if (this.takeKeyword("BETWEEN")) {
      const low = this.parseLiteral();
      this.expectKeyword("AND", "between the BETWEEN bounds");
      const high = this.parseLiteral();
      if (negated) {
        return { kind: "not", part: { kind: "between", column, low, high } };
      }
      return { kind: "between", column, low, high };
    }

    if (negated) {
      throw this.errorAt(this.peek(), "expected IN, LIKE or BETWEEN after NOT");
    }

    const operatorToken = this.peek();
    if (operatorToken.kind !== "operator") {
      throw this.errorAt(
        operatorToken,
        "expected a comparison operator",
        "one of =, !=, >, >=, <, <=, IN, LIKE, BETWEEN, IS NULL",
      );
    }
    this.next();
    const op: ComparisonOperator = operatorToken.text === "<>" ? "!=" : (operatorToken.text as ComparisonOperator);
    const value = this.parseLiteral();
    return { kind: "compare", column, op, value };
  }

  private parseLiteral(): Literal {
    const token = this.peek();
    if (token.kind === "string") {
      this.next();
      return { kind: "string", value: token.text };
    }
    if (token.kind === "number") {
      this.next();
      return typeof token.numeric === "bigint"
        ? { kind: "bigint", value: token.numeric }
        : { kind: "number", value: token.numeric as number };
    }
    if (token.kind === "word" && !token.quoted) {
      if (token.upper === "TRUE" || token.upper === "FALSE") {
        this.next();
        return { kind: "boolean", value: token.upper === "TRUE" };
      }
      if (token.upper === "NULL") {
        this.next();
        return { kind: "null" };
      }
    }
    throw this.errorAt(
      token,
      "expected a literal value",
      "strings use single quotes: WHERE name = 'Alice'",
    );
  }

  // ---- GRAPH ---------------------------------------------------------------

  private parseGraph(): GraphStatement {
    this.expectKeyword("GRAPH", "at the start of the statement");

    // `GRAPH` on its own means "the whole graph, up to the default caps".
    let source: Source = {
      entity: "nodes",
      label: null,
      span: this.tokens[this.position - 1].span,
    };
    if (this.takeKeyword("FROM")) {
      source = this.parseSource();
    } else if (this.isKeyword("NODES") || this.isKeyword("NODE")) {
      source = this.parseSource();
    }
    if (source.entity !== "nodes") {
      throw new HqlError(
        "GRAPH selects nodes; edges are pulled in around them",
        source.span,
        "use VIA <Label> to restrict which edges are drawn",
      );
    }

    const where = this.takeKeyword("WHERE") ? this.parseCondition() : null;
    const hops = this.parseHops();

    let edgeLabel: string | null = null;
    if (this.takeKeyword("VIA")) edgeLabel = this.parseLabel("after VIA");

    const withProperties: string[] = [];
    if (this.takeKeyword("WITH")) {
      do {
        withProperties.push(this.parseColumn().name);
      } while (this.takePunct(","));
    }

    const orderBy = this.parseOrderBy();
    const { skip, limit } = this.parsePaging();

    let maxEdges: number | null = null;
    if (this.isKeyword("EDGE") || this.isKeyword("EDGES")) {
      this.next();
      this.expectKeyword("LIMIT", "after EDGE");
      maxEdges = this.parseCount("EDGE LIMIT");
    }

    const lastHop = hops.length > 0 ? hops[hops.length - 1] : null;
    if (lastHop && isEdgeLandingHop(lastHop.direction)) {
      throw new HqlError(
        "the last TRAVERSE of a GRAPH must land on nodes",
        lastHop.span,
        "drop the EDGES keyword, or add TRAVERSE TARGET to step back onto nodes",
      );
    }

    return {
      kind: "graph",
      selection: { source, where, hops, orderBy, skip, limit, distinct: true },
      withProperties,
      edgeLabel,
      maxEdges,
    };
  }

  // ---- SHOW / DESCRIBE -----------------------------------------------------

  private parseShow(): ShowStatement {
    this.expectKeyword("SHOW", "at the start of the statement");

    let target: ShowStatement["target"];
    if (this.takeKeyword("STATS")) {
      target = "stats";
    } else if (this.isKeyword("NODE") || this.isKeyword("NODES")) {
      this.next();
      this.expectKeyword("LABELS", "after NODE");
      target = "nodeLabels";
    } else if (this.isKeyword("EDGE") || this.isKeyword("EDGES")) {
      this.next();
      this.expectKeyword("LABELS", "after EDGE");
      target = "edgeLabels";
    } else if (this.takeKeyword("LABELS")) {
      target = "labels";
    } else {
      throw this.errorAt(
        this.peek(),
        "expected LABELS, NODE LABELS, EDGE LABELS or STATS",
        "try: SHOW LABELS",
      );
    }

    const sample = this.takeKeyword("SAMPLE") ? this.parseCount("SAMPLE") : 5000;
    return { kind: "show", target, sample };
  }

  private parseDescribe(): DescribeStatement {
    this.expectKeyword("DESCRIBE", "at the start of the statement");

    let entity: EntityKind;
    if (this.isKeyword("NODE") || this.isKeyword("NODES")) entity = "nodes";
    else if (this.isKeyword("EDGE") || this.isKeyword("EDGES")) entity = "edges";
    else {
      throw this.errorAt(this.peek(), "expected NODE or EDGE", "try: DESCRIBE NODE 42");
    }
    this.next();

    const token = this.peek();
    let id: string;
    if (token.kind === "number" && !token.text.includes(".")) {
      id = token.text;
    } else if (token.kind === "string" && /^-?\d+$/.test(token.text)) {
      id = token.text;
    } else {
      throw this.errorAt(token, "expected a numeric entity id", "HelixDB ids are 64-bit integers");
    }
    this.next();

    const limit = this.takeKeyword("LIMIT") ? this.parseCount("LIMIT") : 200;
    return { kind: "describe", entity, id, limit };
  }
}

const DIRECTION_HINT =
  "one of OUT, IN, BOTH (to nodes), OUT EDGES, IN EDGES, BOTH EDGES (to edges), SOURCE, TARGET, OTHER (edge to node)";

/** Keywords that are unambiguous in a column position and stay usable there. */
function isColumnSafeKeyword(upper: string): boolean {
  return upper === "SOURCE" || upper === "TARGET" || upper === "LABELS" || upper === "STATS";
}

function isEdgeLandingHop(direction: HopDirection): boolean {
  return direction === "oute" || direction === "ine" || direction === "bothe";
}

/** Resolves a written column name to its wire-level source. */
export function makeColumn(name: string, span: Span, quoted: boolean): ColumnRef {
  if (name.startsWith("$")) {
    return { name, source: name, virtual: true, span };
  }
  // Quoting is the escape hatch for a property that shadows a virtual column.
  if (!quoted) {
    const virtual = VIRTUAL_COLUMNS[name.toLowerCase()];
    if (virtual) return { name, source: virtual, virtual: true, span };
  }
  return { name, source: name, virtual: false, span };
}

/** Parses one HelixSQL statement, throwing {@link HqlError} on bad input. */
export function parse(source: string): Statement {
  return new Parser(source).parse();
}

export type { Selection };
