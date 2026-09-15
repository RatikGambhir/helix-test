import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  getConnection,
  isDesktop,
  parseResponseBody,
  runQuery,
  setConnection,
  testConnection,
  TransportError,
  type ConnectionView,
} from "./client";
import type { GraphSelectionEvent } from "./graph/GraphCanvas";
import type { Theme } from "./graph/palette";
import { HqlError, type Statement } from "./hql/ast";
import { compile, type CompiledQuery } from "./hql/compiler";
import { compileLegacy } from "./hql/legacy";
import { parse } from "./hql/parser";
import { readResult, ResultError, type GraphData, type LabelCount, type QueryResult } from "./results";
import { inferSchemaFields, schemaQueryFor, type Schema, type SchemaLabel } from "./schema";
import { ConnectionBar, type ConnectionStatus } from "./ui/ConnectionBar";
import { Inspector } from "./ui/Inspector";
import { QueryEditor } from "./ui/QueryEditor";
import { ResultsPanel } from "./ui/ResultsPanel";
import { SchemaView } from "./ui/SchemaView";
import { Sidebar } from "./ui/Sidebar";
import { SplashScreen } from "./ui/SplashScreen";

const GraphCanvas = lazy(() =>
  import("./graph/GraphCanvas").then((module) => ({ default: module.GraphCanvas })),
);

export type AppView = "query" | "schema" | "graph";
type OutputTab = "results" | "wire";

const INITIAL_QUERY = "GRAPH LIMIT 300";
/** Cheapest way to learn the labels; HelixDB has no catalog to read. */
const SCHEMA_QUERY = "SHOW LABELS SAMPLE 5000";
const SCHEMA_FIELD_SAMPLE = 25;
const SCHEMA_FIELD_CONCURRENCY = 6;
/** Probe used to verify a connection without depending on any schema. */
const PROBE_QUERY = "SELECT COUNT(*) FROM NODES LIMIT 1";
const THEME_KEY = "helix-visualizer.theme";
const HISTORY_LIMIT = 12;

interface RunState {
  running: boolean;
  result: QueryResult | null;
  compiled: ExecutableQuery | null;
  durationMs: number | null;
  error: { message: string; detail: string | null } | null;
}

type ExecutableQuery = CompiledQuery & { transportJson: string };

const IDLE: RunState = {
  running: false,
  result: null,
  compiled: null,
  durationMs: null,
  error: null,
};

export function App() {
  const desktop = useMemo(isDesktop, []);
  const [theme, setTheme] = useState<Theme>(readInitialTheme);
  const [showSplash, setShowSplash] = useState(true);
  const completeSplash = useCallback(() => setShowSplash(false), []);
  const [query, setQuery] = useState(INITIAL_QUERY);
  const [view, setView] = useState<AppView>("query");
  const [outputTab, setOutputTab] = useState<OutputTab>("results");
  const [run, setRun] = useState<RunState>(IDLE);
  const [history, setHistory] = useState<string[]>([]);

  const [connection, setConnectionView] = useState<ConnectionView | null>(null);
  const [status, setStatus] = useState<ConnectionStatus>({ kind: "disconnected" });
  const [connectionOpenRequest, setConnectionOpenRequest] = useState(0);

  const [schema, setSchema] = useState<Schema | null>(null);
  const [schemaError, setSchemaError] = useState<string | null>(null);
  const [schemaLoading, setSchemaLoading] = useState(false);
  const schemaLoadId = useRef(0);

  const [detail, setDetail] = useState<Extract<QueryResult, { kind: "describe" }> | null>(null);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  /** The last graph produced, kept so the canvas survives a non-graph query. */
  const [graph, setGraph] = useState<GraphData | null>(null);
  const [graphLoading, setGraphLoading] = useState(false);
  const [graphError, setGraphError] = useState<string | null>(null);
  const graphLoadAttempted = useRef(false);
  const graphLoadId = useRef(0);

  // Compile as the user types so mistakes surface before anything is sent.
  const compileState = useMemo(() => {
    const trimmed = query.trim();
    if (trimmed.length === 0) return { compiled: null, error: null };
    try {
      return { compiled: compileForTransport(trimmed), error: null };
    } catch (error) {
      if (error instanceof HqlError) return { compiled: null, error };
      return { compiled: null, error: new HqlError(String(error)) };
    }
  }, [query]);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    window.localStorage.setItem(THEME_KEY, theme);
  }, [theme]);

  // ---- transport helpers --------------------------------------------------

  /** Sends a compiled query and decodes it, normalising every failure mode. */
  const execute = useCallback(async (compiled: ExecutableQuery) => {
    const response = await runQuery(compiled.transportJson);
    const body = parseResponseBody(response);
    return { result: readResult(body, compiled.shape), durationMs: response.durationMs };
  }, []);

  const describeFailure = (error: unknown): { message: string; detail: string | null } => {
    if (error instanceof TransportError) return { message: error.message, detail: error.detail };
    if (error instanceof ResultError) {
      return { message: error.message, detail: previewBody(error.body) };
    }
    if (error instanceof HqlError) return { message: error.message, detail: error.hint };
    return { message: error instanceof Error ? error.message : String(error), detail: null };
  };

  // ---- actions ------------------------------------------------------------

  const onRun = useCallback(async () => {
    const compiled = compileState.compiled;
    if (!compiled) return;
    if (status.kind !== "connected") {
      setRun({
        running: false,
        result: null,
        compiled,
        durationMs: null,
        error: { message: "Connect to a HelixDB instance before running a query.", detail: null },
      });
      return;
    }

    setRun({ running: true, result: null, compiled, durationMs: null, error: null });
    try {
      const { result, durationMs } = await execute(compiled);
      setRun({ running: false, result, compiled, durationMs, error: null });
      setStatus({ kind: "connected", durationMs });

      if (result.kind === "graph") {
        setGraph(result.graph);
        setGraphError(null);
        graphLoadAttempted.current = true;
        setView("graph");
      } else if (result.kind === "describe") {
        setDetail(result);
        setDetailError(null);
        if (result.id) setSelectedId(result.id);
        setOutputTab("results");
        setView("query");
      } else {
        setOutputTab("results");
        setView("query");
      }

      const trimmed = query.trim();
      setHistory((current) =>
        [trimmed, ...current.filter((entry) => entry !== trimmed)].slice(0, HISTORY_LIMIT),
      );
    } catch (error) {
      const failure = describeFailure(error);
      setRun({ running: false, result: null, compiled, durationMs: null, error: failure });
      if (error instanceof TransportError) setStatus({ kind: "failed", message: failure.message });
    }
  }, [compileState.compiled, execute, query, status.kind]);

  /** Loads a safe snapshot for the Graph workspace without changing the editor. */
  const loadCurrentGraph = useCallback(async () => {
    const requestId = ++graphLoadId.current;
    graphLoadAttempted.current = true;
    setGraphLoading(true);
    setGraphError(null);

    try {
      const compiled = compileForTransport(INITIAL_QUERY);
      const { result, durationMs } = await execute(compiled);
      if (result.kind !== "graph") throw new Error("HelixDB returned a non-graph result.");
      if (requestId !== graphLoadId.current) return;

      setGraph(result.graph);
      setRun({ running: false, result, compiled, durationMs, error: null });
      setStatus({ kind: "connected", durationMs });
    } catch (error) {
      if (requestId !== graphLoadId.current) return;
      const failure = describeFailure(error);
      setGraphError(`${failure.message}${failure.detail ? ` — ${failure.detail}` : ""}`);
      if (error instanceof TransportError) setStatus({ kind: "failed", message: failure.message });
    } finally {
      if (requestId === graphLoadId.current) setGraphLoading(false);
    }
  }, [execute]);

  const refreshCurrentGraph = useCallback(() => {
    graphLoadAttempted.current = false;
    void loadCurrentGraph();
  }, [loadCurrentGraph]);

  const inspect = useCallback(
    async (kind: "node" | "edge", id: string) => {
      setSelectedId(id);
      setDetailLoading(true);
      setDetailError(null);
      try {
        const compiled = compileForTransport(`DESCRIBE ${kind === "node" ? "NODE" : "EDGE"} ${id}`);
        const { result } = await execute(compiled);
        setDetail(result.kind === "describe" ? result : null);
      } catch (error) {
        setDetail(null);
        setDetailError(describeFailure(error).message);
      } finally {
        setDetailLoading(false);
      }
    },
    [execute],
  );

  const loadSchemaLabel = useCallback(async (
    category: "nodes" | "edges",
    entry: LabelCount,
  ): Promise<SchemaLabel> => {
    try {
      const compiled = compileForTransport(schemaQueryFor(category, entry.label, SCHEMA_FIELD_SAMPLE));
      const { result } = await execute(compiled);
      if (result.kind !== "rows") throw new Error("unexpected property sample response");
      return {
        ...entry,
        fields: inferSchemaFields(result.rows),
        fieldSample: result.rows.length,
      };
    } catch (error) {
      return {
        ...entry,
        fields: [],
        fieldSample: 0,
        fieldError: describeFailure(error).message,
      };
    }
  }, [execute]);

  const refreshSchema = useCallback(async () => {
    const requestId = ++schemaLoadId.current;
    setSchemaLoading(true);
    setSchemaError(null);
    try {
      const statement = parse(SCHEMA_QUERY);
      const compiled = compileForStatement(statement);
      const { result, durationMs } = await execute(compiled);
      if (result.kind !== "labels" || statement.kind !== "show") {
        throw new Error("unexpected schema response");
      }
      if (requestId !== schemaLoadId.current) return;

      const nodeLabels = (result.nodes ?? []).map(emptySchemaLabel);
      const edgeLabels = (result.edges ?? []).map(emptySchemaLabel);
      setSchema({ nodeLabels, edgeLabels, sample: statement.sample });
      setStatus({ kind: "connected", durationMs });

      const [enrichedNodes, enrichedEdges] = await Promise.all([
        mapWithConcurrency(nodeLabels, SCHEMA_FIELD_CONCURRENCY, (entry) =>
          loadSchemaLabel("nodes", entry)),
        mapWithConcurrency(edgeLabels, SCHEMA_FIELD_CONCURRENCY, (entry) =>
          loadSchemaLabel("edges", entry)),
      ]);
      if (requestId !== schemaLoadId.current) return;
      setSchema({
        nodeLabels: enrichedNodes,
        edgeLabels: enrichedEdges,
        sample: statement.sample,
      });
    } catch (error) {
      if (requestId !== schemaLoadId.current) return;
      const failure = describeFailure(error);
      setSchemaError(failure.message);
      if (error instanceof TransportError) setStatus({ kind: "failed", message: failure.message });
    } finally {
      if (requestId === schemaLoadId.current) setSchemaLoading(false);
    }
  }, [execute, loadSchemaLabel]);

  const testCandidate = useCallback(async (
    update: { url: string; apiKey?: string; timeoutMs: number; writerOnly: boolean },
  ) => {
    try {
      const probe = compileForTransport(PROBE_QUERY);
      const response = await testConnection(update, probe.transportJson);
      parseResponseBody(response);
      return response.durationMs;
    } catch (error) {
      const failure = describeFailure(error);
      throw new Error(`${failure.message}${failure.detail ? ` — ${failure.detail}` : ""}`);
    }
  }, []);

  const saveConnection = useCallback(
    async (update: { url: string; apiKey?: string; timeoutMs: number; writerOnly: boolean }) => {
      // Verify before persisting, so a bad URL never becomes the saved one.
      setStatus({ kind: "checking" });
      try {
        const durationMs = await testCandidate(update);
        setConnectionView(await setConnection(update));
        graphLoadId.current += 1;
        graphLoadAttempted.current = false;
        setGraph(null);
        setGraphError(null);
        setGraphLoading(false);
        setStatus({ kind: "connected", durationMs });
      } catch (error) {
        const failure = describeFailure(error);
        setStatus({ kind: "failed", message: failure.message });
        throw new Error(`${failure.message}${failure.detail ? ` — ${failure.detail}` : ""}`);
      }
      await refreshSchema();
    },
    [refreshSchema, testCandidate],
  );

  const disconnect = useCallback(() => {
    setStatus({ kind: "disconnected" });
    schemaLoadId.current += 1;
    setSchema(null);
    setSchemaError(null);
    setDetail(null);
    setDetailError(null);
    setSelectedId(null);
    graphLoadId.current += 1;
    graphLoadAttempted.current = false;
    setGraph(null);
    setGraphError(null);
    setGraphLoading(false);
    setRun(IDLE);
    setView("query");
  }, []);

  const onGraphSelect = useCallback(
    (selection: GraphSelectionEvent | null) => {
      if (!selection) {
        setSelectedId(null);
        return;
      }
      void inspect(selection.kind, selection.id);
    },
    [inspect],
  );

  const expandNode = useCallback((nodeId: string) => {
    setQuery(`GRAPH NODES\nWHERE id = ${nodeId}\nTRAVERSE BOTH\nLIMIT 200`);
  }, []);

  // ---- startup ------------------------------------------------------------

  const started = useRef(false);
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    void (async () => {
      try {
        setConnectionView(await getConnection());
      } catch {
        setConnectionView(null);
      }
    })();
  }, []);

  // Match HelixDB Explorer: entering Graph fetches the current graph snapshot.
  useEffect(() => {
    if (
      view !== "graph" ||
      status.kind !== "connected" ||
      graph !== null ||
      graphLoading ||
      graphLoadAttempted.current
    ) return;
    void loadCurrentGraph();
  }, [graph, graphLoading, loadCurrentGraph, status.kind, view]);

  // ---- render -------------------------------------------------------------

  if (showSplash) {
    return <SplashScreen minDuration={2_000} onComplete={completeSplash} />;
  }

  return (
    <div className="app">
      <ConnectionBar
        connection={connection}
        status={status}
        desktop={desktop}
        theme={theme}
        activeView={view}
        openRequest={connectionOpenRequest}
        onSelectView={setView}
        onToggleTheme={() => setTheme((value) => (value === "dark" ? "light" : "dark"))}
        onSave={saveConnection}
        onTest={testCandidate}
        onDisconnect={disconnect}
      />

      <main className="app-main">
        {view === "query" ? (
          <div className="app-body query-view">
            <Sidebar
              schema={schema}
              schemaError={schemaError}
              refreshing={schemaLoading}
              history={history}
              onRefreshSchema={refreshSchema}
              onUseQuery={setQuery}
            />

            <section className="workspace">
              <QueryEditor
                value={query}
                onChange={setQuery}
                onRun={onRun}
                running={run.running}
                error={compileState.error}
              />

              <section className="output">
                <nav className="tabs" role="tablist">
                  <TabButton id="results" active={outputTab} onSelect={setOutputTab}>
                    Results
                  </TabButton>
                  <TabButton id="wire" active={outputTab} onSelect={setOutputTab} disabled={!compileState.compiled}>
                    Wire format
                  </TabButton>

                  <span className="tab-status">
                    {run.running && "running…"}
                    {!run.running && run.durationMs !== null && `${run.durationMs} ms`}
                    {!run.running && run.compiled && ` · ${run.compiled.summary}`}
                  </span>
                </nav>

                <div className="tab-body" role="tabpanel">
                  {run.error && (
                    <div className="panel-error" role="alert">
                      <strong>{run.error.message}</strong>
                      {run.error.detail && <pre>{run.error.detail}</pre>}
                    </div>
                  )}

                  {outputTab === "results" &&
                    !run.error &&
                    (run.result ? (
                      <ResultsPanel result={run.result} theme={theme} onInspect={inspect} />
                    ) : (
                      <EmptyQueryState connected={status.kind === "connected"} />
                    ))}

                  {outputTab === "wire" && compileState.compiled && (
                    <div className="wire-view">
                      <p className="hint-text">
                        Sent to <code>POST /v1/query</code> using the HelixDB Explorer-compatible dynamic query format.
                      </p>
                      <pre>{compileState.compiled.transportJson}</pre>
                    </div>
                  )}
                </div>
              </section>
            </section>

            <InspectorPane
              detail={detail}
              loading={detailLoading}
              error={detailError}
              onInspect={inspect}
              onFocusInGraph={expandNode}
            />
          </div>
        ) : view === "schema" ? (
          <SchemaView
            schema={schema}
            error={schemaError}
            loading={schemaLoading}
            connected={status.kind === "connected"}
            onRefresh={refreshSchema}
            onConnect={() => setConnectionOpenRequest((value) => value + 1)}
            onUseQuery={(nextQuery) => {
              setQuery(nextQuery);
              setView("query");
            }}
          />
        ) : (
          <div className="graph-view">
            <section className="graph-workspace">
              <header className="view-toolbar">
                <div>
                  <h1>Graph</h1>
                  <p>{graph ? `${graph.nodes.length.toLocaleString()} nodes · ${graph.edges.length.toLocaleString()} edges` : "Explore the current database as a network"}</p>
                </div>
                <div className="view-toolbar-actions">
                  {status.kind !== "disconnected" ? (
                    <button type="button" onClick={refreshCurrentGraph} disabled={graphLoading}>
                      {graphLoading ? "Refreshing…" : "Refresh"}
                    </button>
                  ) : null}
                  <button type="button" onClick={() => setView("query")}>Edit graph query</button>
                </div>
              </header>
              <div className="graph-stage">
                {graph && graph.nodes.length > 0 ? (
                  <Suspense fallback={<div className="empty-state"><span className="loading-ring" />Loading graph…</div>}>
                    <GraphCanvas
                      graph={graph}
                      theme={theme}
                      selectedId={selectedId}
                      onSelect={onGraphSelect}
                      onExpand={expandNode}
                    />
                  </Suspense>
                ) : graphLoading ? (
                  <div className="empty-state graph-empty" role="status">
                    <span className="loading-ring" aria-hidden="true" />
                    <h2>Syncing graph data</h2>
                    <p>Loading the current nodes and relationships from HelixDB…</p>
                  </div>
                ) : graphError ? (
                  <div className="empty-state graph-empty graph-error" role="alert">
                    <span className="empty-icon" aria-hidden="true">!</span>
                    <h2>Couldn’t load the graph</h2>
                    <p>{graphError}</p>
                    <button type="button" className="primary" onClick={refreshCurrentGraph}>Retry</button>
                  </div>
                ) : status.kind !== "connected" ? (
                  <div className="empty-state graph-empty">
                    <span className="empty-icon" aria-hidden="true">⌘</span>
                    <h2>Connect to explore your graph</h2>
                    <p>Choose a local or cloud HelixDB instance. The current graph will load automatically.</p>
                    <button type="button" className="primary" onClick={() => setConnectionOpenRequest((value) => value + 1)}>Connect Now</button>
                  </div>
                ) : (
                  <div className="empty-state graph-empty">
                    <span className="empty-icon" aria-hidden="true">⌘</span>
                    <h2>No graph data found</h2>
                    <p>The connected instance did not return any nodes or relationships.</p>
                    <button type="button" className="primary" onClick={refreshCurrentGraph}>Refresh</button>
                  </div>
                )}
              </div>
              {graph && <GraphCaveats graph={graph} />}
            </section>

            <InspectorPane
              detail={detail}
              loading={detailLoading}
              error={detailError}
              onInspect={inspect}
              onFocusInGraph={expandNode}
            />
          </div>
        )}
      </main>
    </div>
  );
}

function TabButton({
  id,
  active,
  onSelect,
  disabled,
  children,
}: {
  id: OutputTab;
  active: OutputTab;
  onSelect: (tab: OutputTab) => void;
  disabled?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active === id}
      className={active === id ? "tab active" : "tab"}
      onClick={() => onSelect(id)}
      disabled={disabled}
    >
      {children}
    </button>
  );
}

function InspectorPane(props: React.ComponentProps<typeof Inspector>) {
  return (
    <aside className="detail-pane">
      <header className="section-header">
        <h2>Inspector</h2>
      </header>
      <Inspector {...props} />
    </aside>
  );
}

function EmptyQueryState({ connected }: { connected: boolean }) {
  return (
    <div className="empty-state">
      <span className="empty-icon" aria-hidden="true">›_</span>
      <h2>{connected ? "Ready to explore" : "Connect to get started"}</h2>
      <p>{connected ? "Run the query above to see structured results." : "Use Connection in the toolbar to choose a local or cloud instance."}</p>
    </div>
  );
}

function compileForTransport(source: string): ExecutableQuery {
  return compileForStatement(parse(source));
}

function compileForStatement(statement: Statement): ExecutableQuery {
  const compiled = compile(statement);
  return { ...compiled, transportJson: compileLegacy(statement, compiled.shape) };
}

function emptySchemaLabel(entry: LabelCount): SchemaLabel {
  return { ...entry, fields: [], fieldSample: 0 };
}

async function mapWithConcurrency<T, U>(
  items: T[],
  concurrency: number,
  worker: (item: T) => Promise<U>,
): Promise<U[]> {
  const results = new Array<U>(items.length);
  let cursor = 0;
  const runners = Array.from({ length: Math.min(concurrency, items.length) }, async () => {
    while (cursor < items.length) {
      const index = cursor;
      cursor += 1;
      results[index] = await worker(items[index]);
    }
  });
  await Promise.all(runners);
  return results;
}

/** States plainly where the drawn graph is not the whole truth. */
function GraphCaveats({ graph }: { graph: GraphData }) {
  const notes: string[] = [];
  if (graph.truncatedNodes) notes.push("the node limit was reached — raise it with LIMIT");
  if (graph.truncatedEdges) notes.push("the edge limit was reached — raise it with EDGE LIMIT");
  if (graph.danglingEdges > 0) {
    notes.push(
      `${graph.danglingEdges.toLocaleString()} edge${graph.danglingEdges === 1 ? "" : "s"} left the selection and ${graph.danglingEdges === 1 ? "is" : "are"} not drawn`,
    );
  }
  if (notes.length === 0) return null;
  return <p className="graph-caveats">Note: {notes.join("; ")}.</p>;
}

function readInitialTheme(): Theme {
  const saved = window.localStorage.getItem(THEME_KEY);
  if (saved === "light" || saved === "dark") return saved;
  return window.matchMedia?.("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

/** A short excerpt of an unexpected response, for the error panel. */
function previewBody(body: unknown): string {
  try {
    const text = JSON.stringify(body, (_key, value) =>
      typeof value === "bigint" ? value.toString() : value,
    );
    return text.length > 600 ? `${text.slice(0, 600)}…` : text;
  } catch {
    return String(body);
  }
}
