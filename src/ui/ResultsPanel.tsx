import { ArrowDown, ArrowUp, ChevronsUpDown, PanelRight, Waypoints } from "lucide-react";
import { useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { legendColour, type Theme } from "../graph/palette";
import { stringify, type LabelCount, type QueryResult, type Row } from "../results";

interface Props {
  result: QueryResult;
  theme: Theme;
  /** Clicking an id in the table focuses that entity. */
  onInspect: (kind: "node" | "edge", id: string) => void;
  onShowGraph: () => void;
  onShowInspector: () => void;
}

/** Renders whichever shape the query came back as. */
export function ResultsPanel({ result, theme, onInspect, onShowGraph, onShowInspector }: Props) {
  switch (result.kind) {
    case "rows":
      return <RowTable columns={result.columns} rows={result.rows} onInspect={onInspect} />;
    case "count":
      return (
        <div className="figures">
          <Figure value={result.value} caption="matching entities" />
        </div>
      );
    case "groupCount":
      return <CountBars title={`Count by ${result.by}`} groups={result.groups} theme={theme} />;
    case "labels":
      return (
        <div className="chart-pair">
          {result.nodes ? <CountBars title="Node labels" groups={result.nodes} theme={theme} /> : null}
          {result.edges ? <CountBars title="Edge labels" groups={result.edges} theme={theme} /> : null}
        </div>
      );
    case "stats":
      return (
        <div className="stats">
          <div className="figures">
            <Figure value={result.nodeCount} caption="nodes" />
            <Figure value={result.edgeCount} caption="edges" />
            <Figure value={result.nodeLabels.length} caption="node labels" />
            <Figure value={result.edgeLabels.length} caption="edge labels" />
          </div>
          <div className="chart-pair">
            <CountBars title="Node labels" groups={result.nodeLabels} theme={theme} />
            <CountBars title="Edge labels" groups={result.edgeLabels} theme={theme} />
          </div>
        </div>
      );
    case "graph":
      return (
        <div className="result-note">
          <p>
            <strong>{result.graph.nodes.length.toLocaleString()}</strong> nodes and{" "}
            <strong>{result.graph.edges.length.toLocaleString()}</strong> edges are drawn in the Graph workspace.
          </p>
          <Button variant="outline" size="sm" onClick={onShowGraph}>
            <Waypoints />
            Open graph
          </Button>
        </div>
      );
    case "describe":
      return (
        <div className="result-note">
          <p>Details for this entity are in the Inspector.</p>
          <Button variant="outline" size="sm" className="show-inspector" onClick={onShowInspector}>
            <PanelRight />
            Open inspector
          </Button>
        </div>
      );
  }
}

function Figure({ value, caption }: { value: number; caption: string }) {
  return (
    <div className="figure">
      <span className="figure-value">{value.toLocaleString()}</span>
      <span className="figure-caption">{caption}</span>
    </div>
  );
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
    return <p className="result-note">No rows matched.</p>;
  }

  const isEdgeTable = columns.includes("source") && columns.includes("target");

  return (
    <div className="row-results">
      <p className="row-results-caption">
        {rows.length.toLocaleString()} row{rows.length === 1 ? "" : "s"}
        {sort ? ` · sorted by ${sort.column} on screen only` : ""}
      </p>
      <div className="table-scroll">
        <Table>
          <TableHeader>
            <TableRow>
              {columns.map((column) => {
                const active = sort?.column === column;
                const SortIcon = !active ? ChevronsUpDown : sort.descending ? ArrowDown : ArrowUp;
                return (
                  <TableHead
                    key={column}
                    aria-sort={active ? (sort.descending ? "descending" : "ascending") : undefined}
                  >
                    <button
                      type="button"
                      className="sort-button"
                      data-active={active || undefined}
                      onClick={() =>
                        setSort((current) =>
                          current?.column === column
                            ? { column, descending: !current.descending }
                            : { column, descending: false },
                        )
                      }
                    >
                      {column}
                      <SortIcon aria-hidden="true" />
                    </button>
                  </TableHead>
                );
              })}
            </TableRow>
          </TableHeader>
          <TableBody>
            {sorted.map((row, index) => (
              <TableRow key={index}>
                {columns.map((column) => {
                  const value = row[column];
                  const text = stringify(value);
                  const linkable =
                    (column === "id" || column === "source" || column === "target") && text.length > 0;
                  const numeric = typeof value === "number" || typeof value === "bigint";
                  return (
                    <TableCell key={column} className={numeric ? "is-numeric" : undefined} title={text.length > 40 ? text : undefined}>
                      {linkable ? (
                        <Button
                          variant="link"
                          onClick={() =>
                            onInspect(column === "id" && isEdgeTable ? "edge" : "node", text)
                          }
                          title="Inspect this entity"
                        >
                          {text}
                        </Button>
                      ) : typeof value === "boolean" ? (
                        <span className="cell-bool">{text}</span>
                      ) : (
                        text
                      )}
                    </TableCell>
                  );
                })}
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>
    </div>
  );
}

/** A horizontal bar chart — the right form for comparing label magnitudes. */
function CountBars({ title, groups, theme }: { title: string; groups: LabelCount[]; theme: Theme }) {
  const max = Math.max(1, ...groups.map((group) => group.count));
  return (
    <section className="count-bars">
      <h3 className="eyebrow">{title}</h3>
      {groups.length === 0 ? (
        <p className="result-note">Nothing to show.</p>
      ) : (
        <ul>
          {groups.map((group, index) => (
            <li key={group.label}>
              <span className="bar-label" title={group.label}>
                {group.label}
              </span>
              <span className="bar-track" aria-hidden="true">
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
      )}
    </section>
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
