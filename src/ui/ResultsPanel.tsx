import { useMemo, useState } from "react";

import { legendColour, type Theme } from "../graph/palette";
import { stringify, type LabelCount, type QueryResult, type Row } from "../results";

interface Props {
  result: QueryResult;
  theme: Theme;
  /** Clicking an id in the table focuses that entity. */
  onInspect: (kind: "node" | "edge", id: string) => void;
}

/** Renders whichever shape the query came back as. */
export function ResultsPanel({ result, theme, onInspect }: Props) {
  switch (result.kind) {
    case "rows":
      return <RowTable columns={result.columns} rows={result.rows} onInspect={onInspect} />;
    case "count":
      return (
        <div className="stat-tile">
          <span className="stat-value">{result.value.toLocaleString()}</span>
          <span className="stat-caption">matching entities</span>
        </div>
      );
    case "groupCount":
      return <CountBars title={`by ${result.by}`} groups={result.groups} theme={theme} />;
    case "labels":
      return (
        <div className="split-panels">
          {result.nodes && <CountBars title="Node labels" groups={result.nodes} theme={theme} />}
          {result.edges && <CountBars title="Edge labels" groups={result.edges} theme={theme} />}
        </div>
      );
    case "stats":
      return (
        <div className="stats-layout">
          <div className="stat-row">
            <div className="stat-tile">
              <span className="stat-value">{result.nodeCount.toLocaleString()}</span>
              <span className="stat-caption">nodes</span>
            </div>
            <div className="stat-tile">
              <span className="stat-value">{result.edgeCount.toLocaleString()}</span>
              <span className="stat-caption">edges</span>
            </div>
          </div>
          <div className="split-panels">
            <CountBars title="Node labels" groups={result.nodeLabels} theme={theme} />
            <CountBars title="Edge labels" groups={result.edgeLabels} theme={theme} />
          </div>
        </div>
      );
    case "graph":
      return (
        <p className="hint-text">
          {result.graph.nodes.length.toLocaleString()} nodes and{" "}
          {result.graph.edges.length.toLocaleString()} edges are drawn in the Graph tab.
        </p>
      );
    case "describe":
      return <p className="hint-text">See the inspector on the right.</p>;
  }
}

function RowTable({
  columns,
  rows,
  onInspect,
}: {
  columns: string[];
  rows: Row[];
  onInspect: (kind: "node" | "edge", id: string) => void;
}) {
  const [sort, setSort] = useState<{ column: string; descending: boolean } | null>(null);

  const sorted = useMemo(() => {
    if (!sort) return rows;
    // Client-side sort over the page already fetched. ORDER BY in the query is
    // what sorts the whole result set; this only reorders what is on screen.
    const factor = sort.descending ? -1 : 1;
    return [...rows].sort((a, b) => factor * compareCells(a[sort.column], b[sort.column]));
  }, [rows, sort]);

  if (rows.length === 0) {
    return <p className="hint-text">No rows matched.</p>;
  }

  const isEdgeTable = columns.includes("source") && columns.includes("target");

  return (
    <div className="table-scroll">
      <table>
        <thead>
          <tr>
            {columns.map((column) => (
              <th key={column}>
                <button
                  type="button"
                  onClick={() =>
                    setSort((current) =>
                      current?.column === column
                        ? { column, descending: !current.descending }
                        : { column, descending: false },
                    )
                  }
                >
                  {column}
                  {sort?.column === column && <span aria-hidden="true">{sort.descending ? " ↓" : " ↑"}</span>}
                </button>
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {sorted.map((row, index) => (
            <tr key={index}>
              {columns.map((column) => {
                const value = row[column];
                const text = stringify(value);
                const linkable =
                  (column === "id" || column === "source" || column === "target") && text.length > 0;
                return (
                  <td key={column} className={typeof value === "number" || typeof value === "bigint" ? "numeric" : undefined}>
                    {linkable ? (
                      <button
                        type="button"
                        className="cell-link"
                        onClick={() =>
                          onInspect(column === "id" && isEdgeTable ? "edge" : "node", text)
                        }
                        title="Inspect this entity"
                      >
                        {text}
                      </button>
                    ) : (
                      text
                    )}
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** A horizontal bar chart — the right form for comparing label magnitudes. */
function CountBars({ title, groups, theme }: { title: string; groups: LabelCount[]; theme: Theme }) {
  if (groups.length === 0) {
    return (
      <div className="count-bars">
        <h3>{title}</h3>
        <p className="hint-text">Nothing to show.</p>
      </div>
    );
  }
  const max = Math.max(...groups.map((group) => group.count));
  return (
    <div className="count-bars">
      <h3>{title}</h3>
      <ul>
        {groups.map((group, index) => (
          <li key={group.label}>
            <span className="bar-label" title={group.label}>
              {group.label}
            </span>
            <span className="bar-track">
              <span
                className="bar-fill"
                style={{
                  width: `${Math.max((group.count / max) * 100, 1.5)}%`,
                  // Colour is decorative here; the value is always written out.
                  background: legendColour({ label: group.label, count: group.count, slot: index < 8 ? index : null }, theme),
                }}
              />
            </span>
            <span className="bar-value">{group.count.toLocaleString()}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

function compareCells(a: unknown, b: unknown): number {
  if (typeof a === "bigint" || typeof b === "bigint") {
    const left = typeof a === "bigint" ? a : BigInt(Math.trunc(Number(a) || 0));
    const right = typeof b === "bigint" ? b : BigInt(Math.trunc(Number(b) || 0));
    return left < right ? -1 : left > right ? 1 : 0;
  }
  if (typeof a === "number" && typeof b === "number") return a - b;
  return stringify(a).localeCompare(stringify(b), undefined, { numeric: true });
}
