import {
  ChevronRight,
  ChevronsDownUp,
  ChevronsUpDown,
  Plug,
  RotateCw,
  Search,
  SearchX,
  Table2,
  TriangleAlert,
  Waypoints,
  X,
  Zap,
} from "lucide-react";
import { useId, useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  filterSchemaEntries,
  formatSchemaValue,
  schemaValueText,
  type Schema,
  type SchemaLabel,
} from "../schema";
import { EmptyState } from "./feedback";

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
  const searchId = useId();

  const allEntries = useMemo(
    () => (category === "nodes" ? schema?.nodeLabels ?? [] : category === "edges" ? schema?.edgeLabels ?? [] : []),
    [category, schema],
  );
  const entries = useMemo(() => filterSchemaEntries(allEntries, search), [allEntries, search]);
  const allVisibleExpanded = entries.length > 0 && entries.every((entry) => expanded.has(key(category, entry)));
  const maxCount = Math.max(1, ...allEntries.map((entry) => entry.count));

  const countFor = (id: SchemaCategory) => {
    if (id === "nodes") return schema?.nodeLabels.length ?? 0;
    if (id === "edges") return schema?.edgeLabels.length ?? 0;
    return 0;
  };

  const toggleExpanded = (entryKey: string) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(entryKey)) next.delete(entryKey);
      else next.add(entryKey);
      return next;
    });
  };

  const toggleAll = () => {
    setExpanded((current) => {
      const next = new Set(current);
      for (const entry of entries) {
        if (allVisibleExpanded) next.delete(key(category, entry));
        else next.add(key(category, entry));
      }
      return next;
    });
  };

  const showLedger = !error && connected && schema && category !== "vectors" && entries.length > 0;

  return (
    <section className="schema-view view-enter" aria-labelledby="schema-page-title">
      <header className="view-header">
        <div className="view-title">
          <h1 id="schema-page-title">Schema</h1>
          <p>
            {schema
              ? `Labels sampled from up to ${schema.sample.toLocaleString()} ${category === "vectors" ? "entities" : category}`
              : "Node and edge labels discovered on the instance"}
          </p>
        </div>

        <div className="schema-controls">
          <div className="segmented" role="group" aria-label="Schema category">
            {CATEGORIES.map((item) => (
              <button
                type="button"
                key={item.id}
                className="segmented-item"
                aria-pressed={category === item.id}
                onClick={() => {
                  setCategory(item.id);
                  setSearch("");
                }}
              >
                {item.label}
                <span className="segmented-count">{countFor(item.id)}</span>
              </button>
            ))}
          </div>

          <div className="search-field">
            <Search aria-hidden="true" />
            <label htmlFor={searchId} className="sr-only">Search {category}</label>
            <Input
              id={searchId}
              type="search"
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              onKeyDown={(event) => { if (event.key === "Escape" && search) { event.stopPropagation(); setSearch(""); } }}
              placeholder={`Filter ${category}`}
              disabled={category === "vectors"}
            />
            {search ? (
              <Button variant="ghost" size="icon-sm" onClick={() => setSearch("")} aria-label="Clear filter">
                <X />
              </Button>
            ) : null}
          </div>
        </div>

        <div className="view-actions">
          <Button variant="ghost" onClick={toggleAll} disabled={!showLedger}>
            {allVisibleExpanded ? <ChevronsDownUp /> : <ChevronsUpDown />}
            {allVisibleExpanded ? "Collapse all" : "Expand all"}
          </Button>
          <Button variant="outline" onClick={onRefresh} disabled={!connected || loading}>
            <RotateCw className={loading ? "is-spinning" : undefined} />
            {loading ? "Refreshing" : "Refresh"}
          </Button>
        </div>
      </header>

      <div className="schema-body">
        {error ? (
          <EmptyState
            icon={TriangleAlert}
            tone="danger"
            title="Couldn’t load the schema"
            action={<Button variant="default" onClick={onRefresh}>Try again</Button>}
          >
            {error}
          </EmptyState>
        ) : !connected ? (
          <EmptyState
            icon={Plug}
            title="Connect to inspect your schema"
            action={<Button variant="default" onClick={onConnect}>Connect</Button>}
          >
            Choose a local, remote, or cloud HelixDB instance to discover its node and edge labels.
          </EmptyState>
        ) : loading && !schema ? (
          <EmptyState loading title="Reading schema">
            Sampling labels from the connected instance…
          </EmptyState>
        ) : category === "vectors" ? (
          <EmptyState icon={Zap} title="No vector indexes discovered">
            Vector index metadata is not exposed by the current Explorer query interface.
          </EmptyState>
        ) : showLedger ? (
          <div className="ledger" role="list" aria-label={`${category} labels`} aria-busy={loading}>
            <div className="ledger-columns" aria-hidden="true">
              <span>Label</span>
              <span>Observed</span>
              <span className="ledger-share-heading">Share of sample</span>
              <span>Fields</span>
              <span />
            </div>
            {entries.map((entry) => (
              <LedgerRow
                key={entry.label}
                category={category}
                entry={entry}
                share={entry.count / maxCount}
                expanded={expanded.has(key(category, entry))}
                onToggle={() => toggleExpanded(key(category, entry))}
                onUseQuery={onUseQuery}
              />
            ))}
          </div>
        ) : (
          <EmptyState icon={SearchX} title={search ? "No matching labels" : `No ${category} discovered`}>
            {search ? `Nothing in this schema matches “${search.trim()}”.` : "Refresh to sample the connected instance again."}
          </EmptyState>
        )}
      </div>
    </section>
  );
}

function key(category: SchemaCategory, entry: SchemaLabel) {
  return `${category}:${entry.label}`;
}

function LedgerRow({
  category,
  entry,
  share,
  expanded,
  onToggle,
  onUseQuery,
}: {
  category: Exclude<SchemaCategory, "vectors">;
  entry: SchemaLabel;
  share: number;
  expanded: boolean;
  onToggle: () => void;
  onUseQuery: (query: string) => void;
}) {
  const detailId = useId();
  return (
    <div className="ledger-row" role="listitem" data-expanded={expanded || undefined}>
      <button
        type="button"
        className="ledger-summary"
        onClick={onToggle}
        aria-expanded={expanded}
        aria-controls={detailId}
      >
        <span className="ledger-label" title={entry.label}>{entry.label}</span>
        <span className="ledger-count">{entry.count.toLocaleString()}</span>
        <span className="ledger-share" aria-hidden="true">
          <span style={{ width: `${Math.max(share * 100, 1)}%` }} />
        </span>
        <span className="ledger-fields">{entry.fieldError ? "—" : entry.fields.length}</span>
        <ChevronRight className="ledger-chevron" aria-hidden="true" />
      </button>

      {expanded ? (
        <div className="ledger-detail" id={detailId}>
          <div className="ledger-detail-head">
            <p>
              {category === "nodes" ? "Node label" : "Edge label"} · {entry.fields.length} field
              {entry.fields.length === 1 ? "" : "s"} across {entry.fieldSample} sampled{" "}
              {category === "nodes" ? "node" : "edge"}{entry.fieldSample === 1 ? "" : "s"}
            </p>
            <div className="ledger-actions">
              <Button variant="outline" size="sm" onClick={() => onUseQuery(entry.browseQuery)} title={entry.browseQuery}>
                <Table2 />
                Browse rows
              </Button>
              <Button variant="outline" size="sm" onClick={() => onUseQuery(entry.graphQuery)} title={entry.graphQuery}>
                <Waypoints />
                Graph query
              </Button>
            </div>
          </div>

          {entry.fieldError ? (
            <p className="ledger-message" data-tone="danger">Couldn’t sample property values: {entry.fieldError}</p>
          ) : entry.fields.length === 0 ? (
            <p className="ledger-message">No stored properties found in the sampled {category}.</p>
          ) : (
            <div className="table-scroll">
              <table className="data-table field-table">
                <thead>
                  <tr>
                    <th scope="col">Property</th>
                    <th scope="col">Type</th>
                    <th scope="col">Sample values</th>
                    <th scope="col" className="is-numeric">Present</th>
                  </tr>
                </thead>
                <tbody>
                  {entry.fields.map((field) => (
                    <tr key={field.name}>
                      <th scope="row"><code title={field.name}>{field.name}</code></th>
                      <td><span className="field-type">{field.types.join(" | ")}</span></td>
                      <td>
                        <span className="field-values">
                          {field.values.map((value, index) => {
                            const formatted = formatSchemaValue(value);
                            return <code key={`${index}-${formatted}`} title={schemaValueText(value)}>{formatted}</code>;
                          })}
                        </span>
                      </td>
                      <td className="is-numeric">{field.presentOn}/{entry.fieldSample}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      ) : null}
    </div>
  );
}
