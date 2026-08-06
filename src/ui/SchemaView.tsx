import { useMemo, useState } from "react";

import {
  filterSchemaEntries,
  formatSchemaValue,
  schemaQueryFor,
  schemaValueText,
  type Schema,
  type SchemaLabel,
} from "../schema";

type SchemaCategory = "nodes" | "edges" | "vectors";

interface Props {
  schema: Schema | null;
  error: string | null;
  loading: boolean;
  connected: boolean;
  onRefresh: () => void;
  onConnect: () => void;
  onUseQuery: (query: string) => void;
}

const CATEGORIES: Array<{ id: SchemaCategory; label: string }> = [
  { id: "nodes", label: "Nodes" },
  { id: "edges", label: "Edges" },
  { id: "vectors", label: "Vectors" },
];

export function SchemaView({
  schema,
  error,
  loading,
  connected,
  onRefresh,
  onConnect,
  onUseQuery,
}: Props) {
  const [category, setCategory] = useState<SchemaCategory>("nodes");
  const [search, setSearch] = useState("");
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());

  const allEntries = category === "nodes"
    ? schema?.nodeLabels ?? []
    : category === "edges"
      ? schema?.edgeLabels ?? []
      : [];
  const entries = useMemo(() => filterSchemaEntries(allEntries, search), [allEntries, search]);
  const allVisibleExpanded = entries.length > 0 && entries.every((entry) => expanded.has(entry.label));

  const countFor = (id: SchemaCategory) => {
    if (id === "nodes") return schema?.nodeLabels.length ?? 0;
    if (id === "edges") return schema?.edgeLabels.length ?? 0;
    return 0;
  };

  const toggleExpanded = (label: string) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(label)) next.delete(label);
      else next.add(label);
      return next;
    });
  };

  const toggleAll = () => {
    setExpanded((current) => {
      const next = new Set(current);
      if (allVisibleExpanded) entries.forEach((entry) => next.delete(entry.label));
      else entries.forEach((entry) => next.add(entry.label));
      return next;
    });
  };

  return (
    <section className="schema-view" aria-labelledby="schema-page-title">
      <h1 id="schema-page-title" className="visually-hidden">Schema</h1>

      <header className="schema-toolbar">
        <label className="schema-search">
          <SchemaIcon name="search" />
          <span className="visually-hidden">Search schema labels</span>
          <input
            type="search"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder={`Search ${category}…`}
          />
          {search ? (
            <button type="button" onClick={() => setSearch("")} aria-label="Clear search">×</button>
          ) : null}
        </label>

        <nav className="schema-categories" aria-label="Schema categories">
          {CATEGORIES.map((item) => (
            <button
              type="button"
              key={item.id}
              className={category === item.id ? `schema-category ${item.id} active` : `schema-category ${item.id}`}
              aria-current={category === item.id ? "page" : undefined}
              onClick={() => {
                setCategory(item.id);
                setSearch("");
              }}
            >
              <SchemaIcon name={item.id} />
              <span>{item.label}</span>
              <strong>{countFor(item.id)}</strong>
            </button>
          ))}
        </nav>

        <div className="schema-toolbar-spacer" />
        <div className="schema-actions">
          <button type="button" onClick={toggleAll} disabled={entries.length === 0}>
            <SchemaIcon name="expand" />
            {allVisibleExpanded ? "Collapse" : "Expand"}
          </button>
          <span aria-hidden="true" />
          <button type="button" onClick={onRefresh} disabled={!connected || loading}>
            <SchemaIcon name="refresh" spinning={loading} />
            {loading ? "Refreshing…" : "Refresh"}
          </button>
        </div>
      </header>

      <div className="schema-content">
        {schema ? (
          <p className="schema-sample-note">
            Labels are derived from a sample of up to {schema.sample.toLocaleString()} {category === "vectors" ? "entities" : category}.
          </p>
        ) : null}

        {error ? (
          <div className="schema-page-state error" role="alert">
            <span className="empty-icon" aria-hidden="true">!</span>
            <h2>Couldn’t load the schema</h2>
            <p>{error}</p>
            <button type="button" className="primary" onClick={onRefresh}>Try again</button>
          </div>
        ) : !connected ? (
          <div className="schema-page-state">
            <span className="empty-icon" aria-hidden="true"><SchemaIcon name="nodes" /></span>
            <h2>Connect to inspect your schema</h2>
            <p>Choose a local or cloud HelixDB instance to discover its node and edge labels.</p>
            <button type="button" className="primary" onClick={onConnect}>Connect Now</button>
          </div>
        ) : loading && !schema ? (
          <div className="schema-page-state" role="status">
            <span className="loading-ring" aria-hidden="true" />
            <h2>Reading schema</h2>
            <p>Sampling labels from the connected instance…</p>
          </div>
        ) : category === "vectors" ? (
          <div className="schema-page-state">
            <span className="empty-icon vector" aria-hidden="true"><SchemaIcon name="vectors" /></span>
            <h2>No vector indexes discovered</h2>
            <p>Vector index metadata is not exposed by the current Explorer query interface.</p>
          </div>
        ) : entries.length > 0 ? (
          <div className="schema-card-grid" aria-live="polite">
            {entries.map((entry) => (
              <SchemaCard
                key={entry.label}
                category={category}
                entry={entry}
                sample={schema?.sample ?? 0}
                expanded={expanded.has(entry.label)}
                onToggle={() => toggleExpanded(entry.label)}
                onUseQuery={onUseQuery}
              />
            ))}
          </div>
        ) : (
          <div className="schema-page-state compact">
            <span className="empty-icon" aria-hidden="true"><SchemaIcon name="search" /></span>
            <h2>{search ? "No matching labels" : `No ${category} discovered`}</h2>
            <p>{search ? `Nothing in this schema matches “${search.trim()}”.` : "Refresh to sample the connected instance again."}</p>
          </div>
        )}

        {loading && schema ? <div className="schema-loading-overlay" role="status"><span className="loading-ring" />Refreshing schema…</div> : null}
      </div>
    </section>
  );
}

function SchemaCard({
  category,
  entry,
  sample,
  expanded,
  onToggle,
  onUseQuery,
}: {
  category: Exclude<SchemaCategory, "vectors">;
  entry: SchemaLabel;
  sample: number;
  expanded: boolean;
  onToggle: () => void;
  onUseQuery: (query: string) => void;
}) {
  const query = schemaQueryFor(category, entry.label);
  return (
    <article className={expanded ? `schema-card ${category} expanded` : `schema-card ${category}`}>
      <button type="button" className="schema-card-summary" onClick={onToggle} aria-expanded={expanded}>
        <span className="schema-card-icon" aria-hidden="true"><SchemaIcon name={category} /></span>
        <strong title={entry.label}>{entry.label}</strong>
        <span className="schema-card-count" title="Observed entities">{entry.count.toLocaleString()}</span>
        <SchemaIcon name="chevron" />
      </button>
      {expanded ? (
        <div className="schema-card-detail">
          <dl>
            <div><dt>Kind</dt><dd>{category === "nodes" ? "Node label" : "Edge label"}</dd></div>
            <div><dt>Observed</dt><dd>{entry.count.toLocaleString()} in sample</dd></div>
            <div><dt>Sample cap</dt><dd>{sample.toLocaleString()}</dd></div>
          </dl>
          <section className="schema-property-section">
            <header>
              <strong>Properties</strong>
              <span>{entry.fields.length} field{entry.fields.length === 1 ? "" : "s"} · {entry.fieldSample} sampled</span>
            </header>
            {entry.fieldError ? (
              <p className="schema-field-message error">Couldn’t sample property values: {entry.fieldError}</p>
            ) : entry.fields.length === 0 ? (
              <p className="schema-field-message">No stored properties found in the sampled {category}.</p>
            ) : (
              <div className="schema-property-scroll">
                <table className="schema-property-table">
                  <thead>
                    <tr><th>Property</th><th>Sample value</th></tr>
                  </thead>
                  <tbody>
                    {entry.fields.map((field) => (
                      <tr key={field.name}>
                        <th scope="row">
                          <code title={field.name}>{field.name}</code>
                          <small>{field.types.join(" | ")}</small>
                        </th>
                        <td>
                          <div className="schema-field-values">
                            {field.values.map((value, index) => {
                              const formatted = formatSchemaValue(value);
                              return <code key={`${index}-${formatted}`} title={schemaValueText(value)}>{formatted}</code>;
                            })}
                          </div>
                          <small>{field.presentOn}/{entry.fieldSample} sampled</small>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </section>
          <button type="button" onClick={() => onUseQuery(query)}>Open in Query</button>
        </div>
      ) : null}
    </article>
  );
}

function SchemaIcon({
  name,
  spinning = false,
}: {
  name: SchemaCategory | "search" | "expand" | "refresh" | "chevron";
  spinning?: boolean;
}) {
  const common = {
    width: 20,
    height: 20,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.9,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
    className: spinning ? "spinning" : undefined,
  };
  if (name === "nodes") return <svg {...common}><circle cx="12" cy="12" r="7"/><circle cx="12" cy="12" r="1.5" fill="currentColor" stroke="none"/></svg>;
  if (name === "edges") return <svg {...common}><circle cx="6" cy="12" r="2"/><circle cx="18" cy="6" r="2"/><circle cx="18" cy="18" r="2"/><path d="m7.8 11.1 8.4-4.2m-8.4 6 8.4 4.2"/></svg>;
  if (name === "vectors") return <svg {...common}><path d="m13 2-7 12h6l-1 8 7-12h-6l1-8Z"/></svg>;
  if (name === "search") return <svg {...common}><circle cx="10.5" cy="10.5" r="6.5"/><path d="m15.5 15.5 4 4"/></svg>;
  if (name === "expand") return <svg {...common}><path d="m8 3-4 4 4 4M4 7h7m5 6 4 4-4 4m4-4h-7"/></svg>;
  if (name === "refresh") return <svg {...common}><path d="M20 6v5h-5M4 18v-5h5"/><path d="M18.2 10A7 7 0 0 0 6.1 6.1L4 8m2 6a7 7 0 0 0 11.9 3.9L20 16"/></svg>;
  return <svg {...common}><path d="m9 6 6 6-6 6"/></svg>;
}
