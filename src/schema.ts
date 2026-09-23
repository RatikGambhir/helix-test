import { stringify, type LabelCount } from "./results";

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
  browseQuery: string;
  graphQuery: string;
  fields: SchemaField[];
  fieldSample: number;
  fieldError?: string;
}

const MAX_VALUE_LENGTH = 78;

export function filterSchemaEntries<T extends LabelCount>(entries: T[], search: string): T[] {
  const needle = search.trim().toLocaleLowerCase();
  if (!needle) return entries;
  return entries.filter((entry) => entry.label.toLocaleLowerCase().includes(needle));
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
