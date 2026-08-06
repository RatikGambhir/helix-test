/**
 * HelixDB Explorer-compatible dynamic query compiler.
 *
 * Current enterprise-dev images expose dynamic queries at POST /v1/query and
 * use the flat, tagged traversal representation also used by helixdb-explorer.
 * The public HelixDB TypeScript SDK currently emits a newer nested traversal
 * representation, so the desktop transport needs this compatibility wire
 * format while the SDK representation remains useful for validation/tests.
 */
import { stringifyJson } from "@helix-db/helix-db";

import type {
  Condition,
  EntityKind,
  GraphStatement,
  Literal,
  Selection,
  ShowStatement,
  Statement,
} from "./ast";
import {
  DEFAULT_GRAPH_EDGE_LIMIT,
  DEFAULT_GRAPH_NODE_LIMIT,
  DEFAULT_ROW_LIMIT,
  type ResultShape,
} from "./compiler";

type WireValue = string | number | bigint | boolean | null | WireValue[] | { [key: string]: WireValue };
type Step = WireValue;

interface NamedQuery {
  Query: {
    name: string;
    steps: Step[];
    condition: null;
  };
}

/** Compile one parsed HelixSQL statement for the API used by Explorer. */
export function compileLegacy(statement: Statement, shape: ResultShape): string {
  const queries = queriesForStatement(statement, shape);
  const request = {
    request_type: "read",
    query_name: `helix_visualizer_${statement.kind}`,
    query: {
      queries,
      returns: queries.map((query) => query.Query.name),
    },
    parameters: {},
  };
  return stringifyJson(request);
}

function queriesForStatement(statement: Statement, shape: ResultShape): NamedQuery[] {
  switch (statement.kind) {
    case "select": {
      if (shape.kind !== "rows" && shape.kind !== "count" && shape.kind !== "groupCount") {
        throw new Error("unexpected SELECT result shape");
      }
      const terminal =
        statement.projection.kind === "count"
          ? statement.groupBy
            ? { GroupCount: statement.groupBy.source }
            : "Count"
          : terminalFor(landsOn(statement.selection));
      const defaultLimit = statement.projection.kind === "count" ? null : DEFAULT_ROW_LIMIT;
      const queries = [named("rows", [...selectionSteps(statement.selection, defaultLimit), terminal])];
      // SELECT * keeps the existing identity companion contract. ValueMap and
      // EdgeProperties already include identity fields in the v1 response.
      if (shape.kind === "rows" && shape.identityVariable) {
        queries.push(named(shape.identityVariable, [...selectionSteps(statement.selection, DEFAULT_ROW_LIMIT), terminalFor(landsOn(statement.selection))]));
      }
      return queries;
    }

    case "graph":
      return graphQueries(statement);

    case "show":
      return showQueries(statement);

    case "describe": {
      const id = integerId(statement.id);
      if (statement.entity === "edges") {
        const edge = () => [{ E: { Ids: [id] } } as Step];
        return [
          named("entity", [...edge(), "EdgeProperties"]),
          named("identity", [...edge(), "EdgeProperties"]),
          named("sourceNode", [...edge(), { OutN: null }, { ValueMap: null }]),
          named("targetNode", [...edge(), { InN: null }, { ValueMap: null }]),
        ];
      }
      const node = () => [{ N: { Ids: [id] } } as Step];
      const incident = () => [...node(), { BothE: null } as Step];
      return [
        named("entity", [...node(), { ValueMap: null }]),
        named("identity", [...node(), { ValueMap: null }]),
        named("edges", [...incident(), { Limit: statement.limit }, "EdgeProperties"]),
        named("neighbours", [...node(), { Both: null }, "Dedup", { Limit: statement.limit }, { ValueMap: null }]),
        named("degree", [...incident(), "Count"]),
      ];
    }
  }
}

function graphQueries(statement: GraphStatement): NamedQuery[] {
  const nodeLimit = statement.selection.limit ?? DEFAULT_GRAPH_NODE_LIMIT;
  const edgeLimit = statement.maxEdges ?? DEFAULT_GRAPH_EDGE_LIMIT;
  const selection = { ...statement.selection, limit: nodeLimit };
  const base = selectionSteps(selection, nodeLimit);
  return [
    named("nodes", [...base, { ValueMap: null }]),
    named("edges", [
      ...selectionSteps(selection, nodeLimit),
      { BothE: statement.edgeLabel },
      "Dedup",
      { Limit: edgeLimit },
      "EdgeProperties",
    ]),
  ];
}

function showQueries(statement: ShowStatement): NamedQuery[] {
  const sample = Math.max(1, statement.sample);
  const nodeLabels = () => named("nodeLabels", [{ N: "All" }, { Limit: sample }, { GroupCount: "$label" }]);
  const edgeSource = () => [{ N: "All" }, { OutE: null }, "Dedup"] as Step[];
  // groupCount currently rejects an edge stream, so fetch only the bounded
  // edge sample and let the response reader aggregate its $label values.
  const edgeLabels = () => named("edgeLabels", [...edgeSource(), { Limit: sample }, "EdgeProperties"]);

  if (statement.target === "stats") {
    return [
      named("nodeCount", [{ N: "All" }, "Count"]),
      named("edgeCount", [...edgeSource(), "Count"]),
      nodeLabels(),
      edgeLabels(),
    ];
  }

  const queries: NamedQuery[] = [];
  if (statement.target !== "edgeLabels") queries.push(nodeLabels());
  if (statement.target !== "nodeLabels") queries.push(edgeLabels());
  return queries;
}

function selectionSteps(selection: Selection, defaultLimit: number | null): Step[] {
  const steps = sourceSteps(selection.source.entity, selection.source.label, selection.where);
  for (const hop of selection.hops) {
    steps.push(hopStep(hop.direction, hop.label));
    if (hop.where) steps.push({ Where: predicate(hop.where) });
  }
  if (selection.distinct) steps.push("Dedup");
  for (const term of selection.orderBy) {
    steps.push({ OrderBy: [term.column.source, term.descending ? "Desc" : "Asc"] });
  }
  if (selection.skip !== null) steps.push({ Skip: selection.skip });
  const limit = selection.limit ?? defaultLimit;
  if (limit !== null) steps.push({ Limit: limit });
  return steps;
}

function sourceSteps(entity: EntityKind, label: string | null, where: Condition | null): Step[] {
  const filters: WireValue[] = [];
  if (label) filters.push({ Eq: ["$label", { String: label }] });
  if (where) filters.push(predicate(where));
  const combined = filters.length === 1 ? filters[0] : filters.length > 1 ? { And: filters } : null;

  if (combined) return [{ [entity === "nodes" ? "NWhere" : "EWhere"]: combined }];
  if (entity === "nodes") return [{ N: "All" }];
  // E only accepts concrete references in current v1. Fan out from all nodes
  // to obtain the edge stream, then deduplicate self/parallel discovery.
  return [{ N: "All" }, { OutE: null }, "Dedup"];
}

function hopStep(direction: Selection["hops"][number]["direction"], label: string | null): Step {
  const names: Record<typeof direction, string> = {
    out: "Out",
    in: "In",
    both: "Both",
    oute: "OutE",
    ine: "InE",
    bothe: "BothE",
    fromnode: "OutN",
    tonode: "InN",
    othernode: "OtherN",
  };
  return { [names[direction]]: label };
}

function predicate(condition: Condition): WireValue {
  switch (condition.kind) {
    case "and":
      return { And: condition.parts.map(predicate) };
    case "or":
      return { Or: condition.parts.map(predicate) };
    case "not":
      return { Not: predicate(condition.part) };
    case "compare": {
      if (condition.value.kind === "null") {
        return { [condition.op === "=" ? "IsNull" : "IsNotNull"]: condition.column.source };
      }
      const operators = { "=": "Eq", "!=": "Neq", ">": "Gt", ">=": "Gte", "<": "Lt", "<=": "Lte" } as const;
      return { [operators[condition.op]]: [condition.column.source, literal(condition.value)] };
    }
    case "between":
      return { Between: [condition.column.source, literal(condition.low), literal(condition.high)] };
    case "in": {
      const values = condition.values.map(literal);
      const inner = { IsIn: [condition.column.source, arrayLiteral(values)] };
      return condition.negated ? { Not: inner } : inner;
    }
    case "like": {
      const starts = condition.pattern.startsWith("%");
      const ends = condition.pattern.endsWith("%");
      const value = condition.pattern.slice(starts ? 1 : 0, ends ? -1 : undefined);
      const variant = starts && ends ? "Contains" : ends ? "StartsWith" : starts ? "EndsWith" : "Eq";
      const inner = { [variant]: [condition.column.source, { String: value }] };
      return condition.negated ? { Not: inner } : inner;
    }
    case "isNull":
      return { [condition.negated ? "IsNotNull" : "IsNull"]: condition.column.source };
    case "has":
      return { HasKey: condition.column.source };
  }
}

function literal(value: Literal): WireValue {
  switch (value.kind) {
    case "string": return { String: value.value };
    case "number": return Number.isInteger(value.value) ? { I64: value.value } : { F64: value.value };
    case "bigint": return { I64: value.value };
    case "boolean": return { Bool: value.value };
    case "null": return "Null";
  }
}

function arrayLiteral(values: WireValue[]): WireValue {
  if (values.every((value) => typeof value === "object" && value !== null && "String" in value)) {
    return { StringArray: values.map((value) => (value as { String: string }).String) };
  }
  if (values.every((value) => typeof value === "object" && value !== null && "I64" in value)) {
    return { I64Array: values.map((value) => (value as { I64: number | bigint }).I64) };
  }
  return { Array: values };
}

function terminalFor(entity: EntityKind): Step {
  return entity === "nodes" ? { ValueMap: null } : "EdgeProperties";
}

function landsOn(selection: Selection): EntityKind {
  let entity = selection.source.entity;
  for (const hop of selection.hops) {
    if (["out", "in", "both", "fromnode", "tonode", "othernode"].includes(hop.direction)) entity = "nodes";
    else entity = "edges";
  }
  return entity;
}

function integerId(value: string): number | bigint {
  const parsed = BigInt(value);
  return parsed <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(parsed) : parsed;
}

function named(name: string, steps: Step[]): NamedQuery {
  return { Query: { name, steps, condition: null } };
}
