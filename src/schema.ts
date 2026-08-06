import { stringify, type LabelCount, type Row } from "./results";

export interface Schema {
  nodeLabels: SchemaLabel[];
  edgeLabels: SchemaLabel[];
  /** How many entities were sampled to derive these labels. */
  sample: number;
}

export interface SchemaField {
  name: string;
  types: string[];
  /** Up to three distinct values observed in the bounded field sample. */
  values: unknown[];
  presentOn: number;
}

export interface SchemaLabel extends LabelCount {
  fields: SchemaField[];
  fieldSample: number;
  fieldError?: string;
}

const ENTITY_FIELDS = new Set(["id", "label", "source", "target"]);
const MAX_FIELD_VALUES = 3;
const MAX_VALUE_LENGTH = 78;

export function filterSchemaEntries<T extends LabelCount>(entries: T[], search: string): T[] {
  const needle = search.trim().toLocaleLowerCase();
  if (!needle) return entries;
  return entries.filter((entry) => entry.label.toLocaleLowerCase().includes(needle));
}

export function schemaQueryFor(category: "nodes" | "edges", label: string, limit = 50): string {
  const quoted = /^[A-Za-z_][A-Za-z0-9_]*$/.test(label)
    ? label
    : `"${label.replace(/"/g, '""')}"`;
  return `SELECT * FROM ${category === "nodes" ? "NODES" : "EDGES"}:${quoted} LIMIT ${Math.max(1, Math.trunc(limit))}`;
}

export function inferSchemaFields(rows: Row[]): SchemaField[] {
  const fields = new Map<string, { types: string[]; values: unknown[]; valueKeys: Set<string>; presentOn: number }>();

  for (const row of rows) {
    for (const [name, value] of Object.entries(row)) {
      if (ENTITY_FIELDS.has(name)) continue;
      let field = fields.get(name);
      if (!field) {
        field = { types: [], values: [], valueKeys: new Set(), presentOn: 0 };
        fields.set(name, field);
      }
      field.presentOn += 1;
      const type = schemaValueType(value);
      if (!field.types.includes(type)) field.types.push(type);

      const valueKey = `${type}:${schemaValueText(value)}`;
      if (field.values.length < MAX_FIELD_VALUES && !field.valueKeys.has(valueKey)) {
        field.valueKeys.add(valueKey);
        field.values.push(value);
      }
    }
  }

  return [...fields.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([name, field]) => ({
      name,
      types: field.types,
      values: field.values,
      presentOn: field.presentOn,
    }));
}

export function formatSchemaValue(value: unknown): string {
  const text = schemaValueText(value);
  return text.length <= MAX_VALUE_LENGTH ? text : `${text.slice(0, MAX_VALUE_LENGTH - 1)}…`;
}

export function schemaValueText(value: unknown): string {
  if (value === null) return "null";
  if (value === undefined) return "undefined";
  if (value === "") return '""';
  return stringify(value);
}

function schemaValueType(value: unknown): string {
  if (value === null) return "null";
  if (Array.isArray(value)) return "array";
  if (typeof value === "bigint") return "integer";
  if (typeof value === "object") return "object";
  return typeof value;
}
