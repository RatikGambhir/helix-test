import { RotateCw, Table2, Waypoints } from "lucide-react";

import { Button } from "@/components/ui/button";
import type { Schema, SchemaLabel } from "../schema";
import { ErrorNotice } from "./feedback";

interface Props {
  schema: Schema | null;
  schemaError: string | null;
  refreshing: boolean;
  history: string[];
  onRefreshSchema: () => void;
  onUseQuery: (query: string) => void;
}

const EXAMPLES: { title: string; query: string }[] = [
  { title: "Whole graph", query: "QUERY LIMIT 300" },
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

/** Everything that can be loaded into the editor: labels, examples, history. */
export function Sidebar({
  schema,
  schemaError,
  refreshing,
  history,
  onRefreshSchema,
  onUseQuery,
}: Props) {
  return (
    <div className="library">
      <section className="library-section" aria-labelledby="library-schema">
        <header className="library-heading">
          <h3 id="library-schema">Schema</h3>
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={onRefreshSchema}
            disabled={refreshing}
            aria-label="Refresh schema"
            title="Refresh schema"
          >
            <RotateCw className={refreshing ? "is-spinning" : undefined} />
          </Button>
        </header>

        {schemaError ? <ErrorNotice message={schemaError} /> : null}
        {!schema && !schemaError ? (
          <p className="library-note">Labels appear here once connected.</p>
        ) : null}

        {schema ? (
          <>
            <LabelList
              title="Nodes"
              labels={schema.nodeLabels}
              onUseQuery={onUseQuery}
              primary="graphQuery"
              secondary="browseQuery"
            />
            <LabelList
              title="Edges"
              labels={schema.edgeLabels}
              onUseQuery={onUseQuery}
              primary="browseQuery"
              secondary="graphQuery"
            />
            <p className="library-note">
              Sampled from up to {schema.sample.toLocaleString()} entities per side; rare labels
              may be missing.
            </p>
          </>
        ) : null}
      </section>

      <section className="library-section" aria-labelledby="library-examples">
        <header className="library-heading">
          <h3 id="library-examples">Examples</h3>
        </header>
        <ul className="library-list">
          {EXAMPLES.map((example) => (
            <li key={example.title}>
              <button type="button" className="library-item" onClick={() => onUseQuery(example.query)}>
                <span className="library-item-title">{example.title}</span>
                <code className="library-item-code">{firstLine(example.query)}</code>
              </button>
            </li>
          ))}
        </ul>
      </section>

      {history.length > 0 ? (
        <section className="library-section" aria-labelledby="library-history">
          <header className="library-heading">
            <h3 id="library-history">History</h3>
            <span className="library-count">{history.length}</span>
          </header>
          <ul className="library-list">
            {history.map((query, index) => (
              <li key={`${index}-${query}`}>
                <button type="button" className="library-item is-code" onClick={() => onUseQuery(query)} title={query}>
                  <code className="library-item-code">{query.replace(/\s+/g, " ")}</code>
                </button>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
    </div>
  );
}

function LabelList({
  title,
  labels,
  onUseQuery,
  primary,
  secondary,
}: {
  title: string;
  labels: SchemaLabel[];
  onUseQuery: (query: string) => void;
  primary: "browseQuery" | "graphQuery";
  secondary: "browseQuery" | "graphQuery";
}) {
  const SecondaryIcon = secondary === "graphQuery" ? Waypoints : Table2;
  const secondaryName = secondary === "graphQuery" ? "Draw as graph" : "Browse as rows";

  return (
    <div className="label-group">
      <h4>{title}</h4>
      {labels.length === 0 ? (
        <p className="library-note">None found.</p>
      ) : (
        <ul className="library-list">
          {labels.map((entry) => (
            <li key={entry.label} className="label-row">
              <button
                type="button"
                className="library-item"
                onClick={() => onUseQuery(entry[primary])}
                title={entry[primary]}
              >
                <span className="library-item-title">{entry.label}</span>
                <span className="library-item-count">{entry.count.toLocaleString()}</span>
              </button>
              <Button
                variant="ghost"
                size="icon-sm"
                className="label-row-alt"
                onClick={() => onUseQuery(entry[secondary])}
                aria-label={`${secondaryName}: ${entry.label}`}
                title={`${secondaryName} — ${entry[secondary]}`}
              >
                <SecondaryIcon />
              </Button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function firstLine(query: string): string {
  const [line, ...rest] = query.split("\n");
  return rest.length > 0 ? `${line} …` : line;
}
