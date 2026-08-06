import { useCallback, useEffect, useMemo, useRef, useState } from "react";

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
import { GraphCanvas, type GraphSelectionEvent } from "./graph/GraphCanvas";
import type { Theme } from "./graph/palette";
import { HqlError } from "./hql/ast";
import { compile, type CompiledQuery } from "./hql/compiler";
import { parse } from "./hql/parser";
import { readResult, ResultError, type GraphData, type QueryResult } from "./results";
import { ConnectionBar, type ConnectionStatus } from "./ui/ConnectionBar";
import { Inspector } from "./ui/Inspector";
import { QueryEditor } from "./ui/QueryEditor";
import { ResultsPanel } from "./ui/ResultsPanel";
import { Sidebar, type Schema } from "./ui/Sidebar";

type Tab = "results" | "graph" | "wire";

const INITIAL_QUERY = "GRAPH LIMIT 300";
/** Cheapest way to learn the labels; HelixDB has no catalog to read. */
const SCHEMA_QUERY = "SHOW LABELS SAMPLE 5000";
/** Probe used to verify a connection without depending on any schema. */
const PROBE_QUERY = "SELECT COUNT(*) FROM NODES LIMIT 1";
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
  const [query, setQuery] = useState(INITIAL_QUERY);
  const [tab, setTab] = useState<Tab>("graph");
  const [run, setRun] = useState<RunState>(IDLE);
  const [history, setHistory] = useState<string[]>([]);

  const [connection, setConnectionView] = useState<ConnectionView | null>(null);
  const [status, setStatus] = useState<ConnectionStatus>({ kind: "unknown" });

  const [schema, setSchema] = useState<Schema | null>(null);
  const [schemaError, setSchemaError] = useState<string | null>(null);
  const [schemaLoading, setSchemaLoading] = useState(false);

  const [detail, setDetail] = useState<Extract<QueryResult, { kind: "describe" }> | null>(null);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  /** The last graph produced, kept so the canvas survives a non-graph query. */
  const [graph, setGraph] = useState<GraphData | null>(null);

  // Compile as the user types so mistakes surface before anything is sent.
  const compileState = useMemo(() => {
    const trimmed = query.trim();
    if (trimmed.length === 0) return { compiled: null, error: null };
    try {
      return { compiled: compile(parse(trimmed)), error: null };
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
  const execute = useCallback(async (compiled: CompiledQuery) => {
    const response = await runQuery(compiled.request.toJsonString());
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

    setRun({ running: true, result: null, compiled, durationMs: null, error: null });
    try {
      const { result, durationMs } = await execute(compiled);
      setRun({ running: false, result, compiled, durationMs, error: null });
      setStatus({ kind: "connected", durationMs });

      if (result.kind === "graph") {
        setGraph(result.graph);
        setTab("graph");
      } else if (result.kind === "describe") {
        setDetail(result);
        setDetailError(null);
        if (result.id) setSelectedId(result.id);
        setTab("results");
      } else {
        setTab("results");
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
  }, [compileState.compiled, execute, query]);

  const inspect = useCallback(
    async (kind: "node" | "edge", id: string) => {
      setSelectedId(id);
      setDetailLoading(true);
      setDetailError(null);
      try {
        const compiled = compile(parse(`DESCRIBE ${kind === "node" ? "NODE" : "EDGE"} ${id}`));
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

  const refreshSchema = useCallback(async () => {
    setSchemaLoading(true);
    setSchemaError(null);
    try {
      const statement = parse(SCHEMA_QUERY);
      const compiled = compile(statement);
      const { result, durationMs } = await execute(compiled);
      if (result.kind !== "labels" || statement.kind !== "show") {
        throw new Error("unexpected schema response");
      }
      setSchema({
        nodeLabels: result.nodes ?? [],
        edgeLabels: result.edges ?? [],
        sample: statement.sample,
      });
      setStatus({ kind: "connected", durationMs });
    } catch (error) {
      const failure = describeFailure(error);
      setSchemaError(failure.message);
      if (error instanceof TransportError) setStatus({ kind: "failed", message: failure.message });
    } finally {
      setSchemaLoading(false);
    }
  }, [execute]);

  const checkConnection = useCallback(async () => {
    setStatus({ kind: "checking" });
    try {
      const compiled = compile(parse(PROBE_QUERY));
      const { durationMs } = await execute(compiled);
      setStatus({ kind: "connected", durationMs });
    } catch (error) {
      setStatus({ kind: "failed", message: describeFailure(error).message });
    }
  }, [execute]);

  const saveConnection = useCallback(
    async (update: { url: string; apiKey?: string; timeoutMs: number; writerOnly: boolean }) => {
      // Verify before persisting, so a bad URL never becomes the saved one.
      setStatus({ kind: "checking" });
      const probe = compile(parse(PROBE_QUERY));
      try {
        const response = await testConnection(update, probe.request.toJsonString());
        parseResponseBody(response);
        setStatus({ kind: "connected", durationMs: response.durationMs });
      } catch (error) {
        const failure = describeFailure(error);
        setStatus({ kind: "failed", message: failure.message });
        throw new Error(`${failure.message}${failure.detail ? ` — ${failure.detail}` : ""}`);
      }
      setConnectionView(await setConnection(update));
      await refreshSchema();
    },
    [refreshSchema],
  );

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
    setTab("graph");
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
      await refreshSchema();
    })();
  }, [refreshSchema]);

  // ---- render -------------------------------------------------------------

  const showGraphTab = graph !== null;

  return (
    <div className="app">
      <ConnectionBar
        connection={connection}
        status={status}
        desktop={desktop}
        theme={theme}
        onToggleTheme={() => setTheme((value) => (value === "dark" ? "light" : "dark"))}
        onSave={saveConnection}
        onTest={checkConnection}
      />

      <div className="app-body">
        <Sidebar
          schema={schema}
          schemaError={schemaError}
          refreshing={schemaLoading}
          history={history}
          onRefreshSchema={refreshSchema}
          onUseQuery={setQuery}
        />

        <main className="workspace">
          <QueryEditor
            value={query}
            onChange={setQuery}
            onRun={onRun}
            running={run.running}
            error={compileState.error}
          />

          <section className="output">
            <nav className="tabs" role="tablist">
              <TabButton id="results" active={tab} onSelect={setTab}>
                Results
              </TabButton>
              <TabButton id="graph" active={tab} onSelect={setTab} disabled={!showGraphTab}>
                Graph
              </TabButton>
              <TabButton id="wire" active={tab} onSelect={setTab} disabled={!compileState.compiled}>
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

              {tab === "results" &&
                !run.error &&
                (run.result ? (
                  <ResultsPanel result={run.result} theme={theme} onInspect={inspect} />
                ) : (
                  <p className="hint-text">Run a query to see results.</p>
                ))}

              {tab === "graph" &&
                (graph ? (
                  <>
                    <GraphCanvas
                      graph={graph}
                      theme={theme}
                      selectedId={selectedId}
                      onSelect={onGraphSelect}
                      onExpand={expandNode}
                    />
                    <GraphCaveats graph={graph} />
                  </>
                ) : (
                  <p className="hint-text">
                    Run a <code>GRAPH</code> query to draw the structure.
                  </p>
                ))}

              {tab === "wire" && compileState.compiled && (
                <div className="wire-view">
                  <p className="hint-text">
                    Sent to <code>POST /v2/query</code>, built with the official HelixDB SDK.
                  </p>
                  <pre>{compileState.compiled.json}</pre>
                </div>
              )}
            </div>
          </section>
        </main>

        <aside className="detail-pane">
          <header className="section-header">
            <h2>Inspector</h2>
          </header>
          <Inspector
            detail={detail}
            loading={detailLoading}
            error={detailError}
            onInspect={inspect}
            onFocusInGraph={expandNode}
          />
        </aside>
      </div>
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
  id: Tab;
  active: Tab;
  onSelect: (tab: Tab) => void;
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
