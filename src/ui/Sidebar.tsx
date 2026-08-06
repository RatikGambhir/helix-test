import type { LabelCount } from "../results";
import type { Schema } from "../schema";

interface Props {
  schema: Schema | null;
  schemaError: string | null;
  refreshing: boolean;
  history: string[];
  onRefreshSchema: () => void;
  onUseQuery: (query: string) => void;
}

const EXAMPLES: { title: string; query: string }[] = [
  { title: "Whole graph", query: "GRAPH LIMIT 300" },
  { title: "Browse nodes", query: "SELECT * FROM NODES LIMIT 50" },
  {
    title: "Filter by property",
    query: "SELECT id, label, name\nFROM NODES:User\nWHERE name LIKE 'a%'\nORDER BY name\nLIMIT 25",
  },
  {
    title: "Follow a relationship",
    query: "SELECT id, name\nFROM NODES:User\nWHERE name = 'Alice'\nTRAVERSE OUT Follows",
  },
  {
    title: "Read edge properties",
    query: "SELECT id, label, source, target\nFROM NODES:User\nTRAVERSE OUT EDGES Follows\nLIMIT 50",
  },
  {
    title: "Two hops out",
    query: "SELECT DISTINCT id, name\nFROM NODES:User\nWHERE name = 'Alice'\nTRAVERSE OUT Follows\nTRAVERSE OUT Follows",
  },
  { title: "Count by label", query: "SELECT COUNT(*) FROM NODES GROUP BY label" },
  { title: "Instance stats", query: "SHOW STATS" },
];

export function Sidebar({
  schema,
  schemaError,
  refreshing,
  history,
  onRefreshSchema,
  onUseQuery,
}: Props) {
  return (
    <aside className="sidebar">
      <section>
        <header className="section-header">
          <h2>Schema</h2>
          <button type="button" onClick={onRefreshSchema} disabled={refreshing}>
            {refreshing ? "…" : "Refresh"}
          </button>
        </header>

        {schemaError && <p className="panel-error">{schemaError}</p>}
        {!schema && !schemaError && <p className="hint-text">Not loaded yet.</p>}

        {schema && (
          <>
            <LabelList
              title="Nodes"
              labels={schema.nodeLabels}
              onUseQuery={onUseQuery}
              toQuery={(label) => `GRAPH NODES:${quote(label)} LIMIT 300`}
              toAltQuery={(label) => `SELECT * FROM NODES:${quote(label)} LIMIT 50`}
            />
            <LabelList
              title="Edges"
              labels={schema.edgeLabels}
              onUseQuery={onUseQuery}
              toQuery={(label) => `SELECT * FROM EDGES:${quote(label)} LIMIT 50`}
              toAltQuery={(label) => `GRAPH VIA ${quote(label)} LIMIT 300`}
            />
            <p className="hint-text">
              Derived from a sample of {schema.sample.toLocaleString()} entities per side — rare
              labels may be missing.
            </p>
          </>
        )}
      </section>

      <section>
        <header className="section-header">
          <h2>Examples</h2>
        </header>
        <ul className="link-list">
          {EXAMPLES.map((example) => (
            <li key={example.title}>
              <button type="button" onClick={() => onUseQuery(example.query)}>
                {example.title}
              </button>
            </li>
          ))}
        </ul>
      </section>

      {history.length > 0 && (
        <section>
          <header className="section-header">
            <h2>History</h2>
          </header>
          <ul className="link-list">
            {history.map((query, index) => (
              <li key={`${index}-${query}`}>
                <button type="button" onClick={() => onUseQuery(query)} title={query}>
                  {query.replace(/\s+/g, " ").slice(0, 48)}
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
    </aside>
  );
}

function LabelList({
  title,
  labels,
  onUseQuery,
  toQuery,
  toAltQuery,
}: {
  title: string;
  labels: LabelCount[];
  onUseQuery: (query: string) => void;
  toQuery: (label: string) => string;
  toAltQuery: (label: string) => string;
}) {
  if (labels.length === 0) {
    return (
      <div className="label-group">
        <h3>{title}</h3>
        <p className="hint-text">None found.</p>
      </div>
    );
  }
  return (
    <div className="label-group">
      <h3>{title}</h3>
      <ul className="label-list">
        {labels.map((entry) => (
          <li key={entry.label}>
            <button type="button" onClick={() => onUseQuery(toQuery(entry.label))} title={toQuery(entry.label)}>
              {entry.label}
            </button>
            <button
              type="button"
              className="label-alt"
              onClick={() => onUseQuery(toAltQuery(entry.label))}
              title={toAltQuery(entry.label)}
            >
              ⋯
            </button>
            <span className="label-count">{entry.count.toLocaleString()}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

/** Labels that are not bare identifiers have to be quoted in HelixSQL. */
function quote(label: string): string {
  return /^[A-Za-z_][A-Za-z0-9_]*$/.test(label) ? label : `"${label.replace(/"/g, '""')}"`;
}
