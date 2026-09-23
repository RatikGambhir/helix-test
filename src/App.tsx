import {
  Network,
  PanelLeft,
  PanelRight,
  Plug,
  RotateCw,
  SquarePen,
  SquareTerminal,
  TriangleAlert,
} from "lucide-react";
import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  BackendError,
  compileQuery,
  getConnection,
  isDesktop,
  loadSchema,
  runQuery,
  setConnection,
  testConnection,
  type CompiledQuery,
  type ConnectionView,
} from "./client";
import type { GraphSelectionEvent } from "./graph/GraphCanvas";
import type { Theme } from "./graph/palette";
import type { GraphData, QueryResult } from "./results";
import type { Schema } from "./schema";
import { ConnectionBar, type ConnectionStatus } from "./ui/ConnectionBar";
import { EmptyState, ErrorNotice } from "./ui/feedback";
import { Inspector } from "./ui/Inspector";
import { Pane } from "./ui/Pane";
import { QueryEditor, type CompileStatus } from "./ui/QueryEditor";
import { ResultsPanel } from "./ui/ResultsPanel";
import { SchemaView } from "./ui/SchemaView";
import { Sidebar } from "./ui/Sidebar";
import { SplashScreen } from "./ui/SplashScreen";

const GraphCanvas = lazy(() =>
  import("./graph/GraphCanvas").then((module) => ({ default: module.GraphCanvas })),
);

export type AppView = "query" | "schema" | "graph";
type OutputTab = "results" | "wire";
/** Side panels that collapse into drawers on narrow windows. */
type Drawer = "library" | "inspector";

const INITIAL_QUERY = "QUERY LIMIT 300";
const THEME_KEY = "helix-visualizer.theme";
const HISTORY_LIMIT = 12;

interface RunState {
  running: boolean;
  result: QueryResult | null;
  compiled: CompiledQuery | null;
  durationMs: number | null;
  error: { message: string; detail: string | null } | null;
}

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
  const [drawer, setDrawer] = useState<Drawer | null>(null);

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

  const [compileState, setCompileState] = useState<{
    source: string;
    compiled: CompiledQuery | null;
    error: BackendError | null;
  }>({ source: "", compiled: null, error: null });

  // Rust is the source of truth even for live editor validation. A short
  // debounce avoids crossing IPC for every key event in a fast typing burst.
  useEffect(() => {
    const source = query.trim();
    if (!source) {
      setCompileState({ source, compiled: null, error: null });
      return;
    }
    let current = true;
    const timer = window.setTimeout(() => {
      void compileQuery(source)
        .then((compiled) => {
          if (current) setCompileState({ source, compiled, error: null });
        })
        .catch((error: unknown) => {
          if (!current) return;
          const failure = error instanceof BackendError ? error : new BackendError(String(error));
          setCompileState({ source, compiled: null, error: failure });
        });
    }, 120);
    return () => {
      current = false;
      window.clearTimeout(timer);
    };
  }, [query]);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    try {
      window.localStorage.setItem(THEME_KEY, theme);
    } catch {
      // Storage can be unavailable; the theme still applies for this session.
    }
  }, [theme]);

  // A drawer belongs to the view it was opened in, and Escape dismisses it.
  useEffect(() => setDrawer(null), [view]);
  useEffect(() => {
    if (!drawer) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !event.defaultPrevented) setDrawer(null);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [drawer]);

  // While the debounced check is in flight the last verdict stays on screen, so
  // the status line does not flicker on every keystroke.
  const compileStatus: CompileStatus = !query.trim()
    ? { kind: "empty" }
    : compileState.compiled
      ? { kind: "valid", summary: compileState.compiled.summary }
      : compileState.error
        ? { kind: "invalid" }
        : { kind: "checking" };

  // ---- transport helpers --------------------------------------------------

  /** Sends source text; Rust owns compilation, transport, and decoding. */
  const execute = useCallback((source: string) => runQuery(source), []);

  const describeFailure = (error: unknown): { message: string; detail: string | null } => {
    if (error instanceof BackendError) {
      return { message: error.message, detail: error.detail ?? error.hint };
    }
    return { message: error instanceof Error ? error.message : String(error), detail: null };
  };

  // ---- actions ------------------------------------------------------------

  const onRun = useCallback(async () => {
    const source = query.trim();
    const compiled = compileState.source === source ? compileState.compiled : null;
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
      const execution = await execute(source);
      const { result, durationMs } = execution;
      setRun({ running: false, result, compiled: execution.compiled, durationMs, error: null });
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

      setHistory((current) =>
        [source, ...current.filter((entry) => entry !== source)].slice(0, HISTORY_LIMIT),
      );
    } catch (error) {
      const failure = describeFailure(error);
      setRun({ running: false, result: null, compiled, durationMs: null, error: failure });
      if (isConnectionFailure(error)) setStatus({ kind: "failed", message: failure.message });
    }
  }, [compileState.compiled, execute, query, status.kind]);

  /** Loads a safe snapshot for the Graph workspace without changing the editor. */
  const loadCurrentGraph = useCallback(async () => {
    const requestId = ++graphLoadId.current;
    graphLoadAttempted.current = true;
    setGraphLoading(true);
    setGraphError(null);

    try {
      const execution = await execute(INITIAL_QUERY);
      const { result, durationMs, compiled } = execution;
      if (result.kind !== "graph") throw new Error("HelixDB returned a non-graph result.");
      if (requestId !== graphLoadId.current) return;

      setGraph(result.graph);
      setRun({ running: false, result, compiled, durationMs, error: null });
      setStatus({ kind: "connected", durationMs });
    } catch (error) {
      if (requestId !== graphLoadId.current) return;
      const failure = describeFailure(error);
      setGraphError(`${failure.message}${failure.detail ? ` — ${failure.detail}` : ""}`);
      if (isConnectionFailure(error)) setStatus({ kind: "failed", message: failure.message });
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
      setDrawer("inspector");
      setDetailLoading(true);
      setDetailError(null);
      try {
        const { result } = await execute(
          `DESCRIBE ${kind === "node" ? "NODE" : "EDGE"} ${id}`,
        );
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

  const refreshSchema = useCallback(async () => {
    const requestId = ++schemaLoadId.current;
    setSchemaLoading(true);
    setSchemaError(null);
    try {
      const { schema: nextSchema, durationMs } = await loadSchema();
      if (requestId !== schemaLoadId.current) return;
      setSchema(nextSchema);
      setStatus({ kind: "connected", durationMs });
    } catch (error) {
      if (requestId !== schemaLoadId.current) return;
      const failure = describeFailure(error);
      setSchemaError(failure.message);
      if (isConnectionFailure(error)) setStatus({ kind: "failed", message: failure.message });
    } finally {
      if (requestId === schemaLoadId.current) setSchemaLoading(false);
    }
  }, []);

  const testCandidate = useCallback(async (
    update: { url: string; apiKey?: string; timeoutMs: number; writerOnly: boolean },
  ) => {
    try {
      const response = await testConnection(update);
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
    setQuery(`QUERY NODES\nWHERE id = ${nodeId}\nTRAVERSE BOTH\nLIMIT 200`);
  }, []);

  const loadQuery = useCallback((nextQuery: string) => {
    setQuery(nextQuery);
    setDrawer(null);
  }, []);

  const openConnection = useCallback(() => setConnectionOpenRequest((value) => value + 1), []);
  const toggleDrawer = (target: Drawer) => setDrawer((current) => (current === target ? null : target));

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

  const connected = status.kind === "connected";
  const inspectorPane = (
    <Pane
      id="inspector-pane"
      side="inspector"
      title="Inspector"
      open={drawer === "inspector"}
      onClose={() => setDrawer(null)}
    >
      <Inspector
        detail={detail}
        loading={detailLoading}
        error={detailError}
        onInspect={inspect}
        onFocusInGraph={expandNode}
      />
    </Pane>
  );

  const drawerTools = (
    <div className="drawer-toggles">
      {view === "query" ? (
        <Button
          variant="ghost"
          size="icon"
          className="drawer-toggle drawer-toggle--library"
          onClick={() => toggleDrawer("library")}
          aria-expanded={drawer === "library"}
          aria-controls="library-pane"
          aria-label="Library"
          title="Library"
        >
          <PanelLeft strokeWidth={1.75} />
        </Button>
      ) : null}
      {view !== "schema" ? (
        <Button
          variant="ghost"
          size="icon"
          className="drawer-toggle drawer-toggle--inspector"
          onClick={() => toggleDrawer("inspector")}
          aria-expanded={drawer === "inspector"}
          aria-controls="inspector-pane"
          aria-label="Inspector"
          title="Inspector"
        >
          <PanelRight strokeWidth={1.75} />
        </Button>
      ) : null}
    </div>
  );

  return (
    <div className="app">
      <ConnectionBar
        connection={connection}
        status={status}
        desktop={desktop}
        theme={theme}
        activeView={view}
        openRequest={connectionOpenRequest}
        tools={drawerTools}
        onSelectView={setView}
        onToggleTheme={() => setTheme((value) => (value === "dark" ? "light" : "dark"))}
        onSave={saveConnection}
        onTest={testCandidate}
        onDisconnect={disconnect}
      />

      <main className="app-main">
        {view === "query" ? (
          <div className="workspace workspace--query view-enter">
            <Pane
              id="library-pane"
              side="library"
              title="Library"
              open={drawer === "library"}
              onClose={() => setDrawer(null)}
            >
              <Sidebar
                schema={schema}
                schemaError={schemaError}
                refreshing={schemaLoading}
                history={history}
                onRefreshSchema={refreshSchema}
                onUseQuery={loadQuery}
              />
            </Pane>

            <section className="query-main" aria-label="Query">
              <QueryEditor
                value={query}
                onChange={setQuery}
                onRun={onRun}
                running={run.running}
                status={compileStatus}
                error={compileState.error}
              />

              <Tabs className="output" value={outputTab} onValueChange={(value) => setOutputTab(value as OutputTab)}>
                <div className="strip output-strip">
                  <TabsList aria-label="Output">
                    <TabsTrigger value="results">Results</TabsTrigger>
                    <TabsTrigger value="wire" disabled={!compileState.compiled}>
                      Wire format
                    </TabsTrigger>
                  </TabsList>

                  <span className="output-meta" aria-live="polite">
                    {run.running ? "Running…" : null}
                    {!run.running && run.durationMs !== null ? <span className="output-meta-time">{run.durationMs} ms</span> : null}
                    {!run.running && run.compiled ? <span className="output-meta-summary">{run.compiled.summary}</span> : null}
                  </span>
                </div>

                <TabsContent value="results" className="output-body">
                  {run.error ? <ErrorNotice message={run.error.message} detail={run.error.detail} /> : null}

                  {!run.error &&
                    (run.running && !run.result ? (
                      <EmptyState loading title="Running query" />
                    ) : run.result ? (
                      <ResultsPanel
                        result={run.result}
                        theme={theme}
                        onInspect={inspect}
                        onShowGraph={() => setView("graph")}
                        onShowInspector={() => setDrawer("inspector")}
                      />
                    ) : (
                      <EmptyQueryState connected={connected} onConnect={openConnection} />
                    ))}
                </TabsContent>

                <TabsContent value="wire" className="output-body">
                  {run.error ? <ErrorNotice message={run.error.message} detail={run.error.detail} /> : null}
                  {compileState.compiled ? (
                    <div className="wire-view">
                      <p>
                        Sent to <code>POST /v1/query</code> in the HelixDB Explorer-compatible dynamic query format.
                      </p>
                      <pre>{compileState.compiled.transportJson}</pre>
                    </div>
                  ) : null}
                </TabsContent>
              </Tabs>
            </section>

            {inspectorPane}
          </div>
        ) : view === "schema" ? (
          <SchemaView
            schema={schema}
            error={schemaError}
            loading={schemaLoading}
            connected={connected}
            onRefresh={refreshSchema}
            onConnect={openConnection}
            onUseQuery={(nextQuery) => {
              setQuery(nextQuery);
              setView("query");
            }}
          />
        ) : (
          <div className="workspace workspace--graph view-enter">
            <section className="graph-main" aria-labelledby="graph-title">
              <header className="view-header">
                <div className="view-title">
                  <h1 id="graph-title">Graph</h1>
                  <p>
                    {graph
                      ? `${graph.nodes.length.toLocaleString()} nodes · ${graph.edges.length.toLocaleString()} edges`
                      : "The connected database, drawn as a network"}
                  </p>
                </div>
                <div className="view-actions">
                  {status.kind !== "disconnected" ? (
                    <Button variant="ghost" onClick={refreshCurrentGraph} disabled={graphLoading}>
                      <RotateCw className={graphLoading ? "is-spinning" : undefined} />
                      {graphLoading ? "Refreshing" : "Refresh"}
                    </Button>
                  ) : null}
                  <Button variant="outline" onClick={() => setView("query")}>
                    <SquarePen />
                    Edit query
                  </Button>
                </div>
              </header>

              <div className="graph-stage">
                {graph && graph.nodes.length > 0 ? (
                  <Suspense fallback={<EmptyState loading title="Preparing canvas" />}>
                    <GraphCanvas
                      graph={graph}
                      theme={theme}
                      selectedId={selectedId}
                      onSelect={onGraphSelect}
                      onExpand={expandNode}
                    />
                  </Suspense>
                ) : graphLoading ? (
                  <EmptyState loading title="Syncing graph data">
                    Loading the current nodes and relationships from HelixDB…
                  </EmptyState>
                ) : graphError ? (
                  <EmptyState
                    icon={TriangleAlert}
                    tone="danger"
                    title="Couldn’t load the graph"
                    action={<Button variant="default" onClick={refreshCurrentGraph}>Retry</Button>}
                  >
                    {graphError}
                  </EmptyState>
                ) : !connected ? (
                  <EmptyState
                    icon={Plug}
                    title="Connect to explore your graph"
                    action={<Button variant="default" onClick={openConnection}>Connect</Button>}
                  >
                    Choose a local, remote, or cloud HelixDB instance. The current graph loads automatically.
                  </EmptyState>
                ) : (
                  <EmptyState
                    icon={Network}
                    title="No graph data found"
                    action={<Button variant="outline" onClick={refreshCurrentGraph}>Refresh</Button>}
                  >
                    The connected instance did not return any nodes or relationships.
                  </EmptyState>
                )}
              </div>
              {graph ? <GraphCaveats graph={graph} /> : null}
            </section>

            {inspectorPane}
          </div>
        )}
      </main>
    </div>
  );
}

function EmptyQueryState({ connected, onConnect }: { connected: boolean; onConnect: () => void }) {
  return connected ? (
    <EmptyState icon={SquareTerminal} title="Ready when you are">
      Write a query above or pick one from the library, then run it with the Run button or{" "}
      <kbd>{navigator.platform.includes("Mac") ? "⌘" : "Ctrl"}+Enter</kbd>.
    </EmptyState>
  ) : (
    <EmptyState
      icon={Plug}
      title="Connect to get started"
      action={
        <Button variant="outline" onClick={onConnect}>
          <Plug />
          Connect
        </Button>
      }
    >
      Point the visualizer at a local, remote, or cloud HelixDB instance. You can write queries in the meantime.
    </EmptyState>
  );
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
  return (
    <p className="graph-caveats">
      <TriangleAlert aria-hidden="true" />
      <span>{notes.join("; ")}.</span>
    </p>
  );
}

function readInitialTheme(): Theme {
  let saved: string | null = null;
  try {
    saved = window.localStorage.getItem(THEME_KEY);
  } catch {
    // Fall through to the system preference.
  }
  if (saved === "light" || saved === "dark") return saved;
  return window.matchMedia?.("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

function isConnectionFailure(error: unknown): boolean {
  return (
    error instanceof BackendError &&
    ["transport", "http", "invalidUrl", "result"].includes(error.kind)
  );
}
