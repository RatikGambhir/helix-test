import { useEffect, useMemo, useState } from "react";

import type { AppView } from "../App";
import type { ConnectionUpdate, ConnectionView } from "../client";

export type ConnectionStatus =
  | { kind: "disconnected" }
  | { kind: "checking" }
  | { kind: "connected"; durationMs: number }
  | { kind: "failed"; message: string };

interface Props {
  connection: ConnectionView | null;
  status: ConnectionStatus;
  desktop: boolean;
  theme: "light" | "dark";
  activeView: AppView;
  openRequest: number;
  onSelectView: (view: AppView) => void;
  onToggleTheme: () => void;
  onSave: (update: ConnectionUpdate) => Promise<void>;
  onTest: (update: ConnectionUpdate) => Promise<number>;
  onDisconnect: () => void;
}

const NAV_ITEMS: Array<{ id: AppView; label: string; icon: "query" | "schema" | "graph" }> = [
  { id: "query", label: "Query", icon: "query" },
  { id: "schema", label: "Schema", icon: "schema" },
  { id: "graph", label: "Graph", icon: "graph" },
];

export function ConnectionBar({
  connection,
  status,
  desktop,
  theme,
  activeView,
  openRequest,
  onSelectView,
  onToggleTheme,
  onSave,
  onTest,
  onDisconnect,
}: Props) {
  const [open, setOpen] = useState(false);
  const connected = status.kind === "connected";

  useEffect(() => {
    if (openRequest > 0) setOpen(true);
  }, [openRequest]);

  return (
    <>
      <header className="app-header">
        <div className="title-bar" data-tauri-drag-region>
          <div className="title-brand">
            <span className="brand-mark" aria-hidden="true"><span /></span>
            <span>Helix Visualizer</span>
          </div>
        </div>

        <div className="top-toolbar">
          <button
            type="button"
            className={connected ? "toolbar-item connection-item connected" : "toolbar-item connection-item"}
            onClick={() => setOpen(true)}
            title={connected ? `Connected to ${connection?.url ?? "HelixDB"}` : "Configure a connection"}
          >
            <ToolbarIcon name="connection" connected={connected} />
            <span>Connection</span>
            <i className={`toolbar-status status-${status.kind}`} aria-hidden="true" />
          </button>

          <span className="toolbar-divider" aria-hidden="true" />

          <nav className="top-nav" aria-label="Main views">
            {NAV_ITEMS.map((item) => (
              <button
                type="button"
                key={item.id}
                className={activeView === item.id ? `toolbar-item nav-${item.id} active` : `toolbar-item nav-${item.id}`}
                aria-current={activeView === item.id ? "page" : undefined}
                onClick={() => onSelectView(item.id)}
              >
                <ToolbarIcon name={item.icon} />
                <span>{item.label}</span>
              </button>
            ))}
          </nav>

          <div className="toolbar-spacer" />

          <div className={`connection-summary status-${status.kind}`}>
            <span className="status-dot" aria-hidden="true" />
            <span>{describeStatus(status, connection)}</span>
          </div>

          <button type="button" className="theme-button" onClick={onToggleTheme} title="Switch theme" aria-label="Switch theme">
            <ToolbarIcon name={theme === "dark" ? "sun" : "moon"} />
          </button>
        </div>
      </header>

      {open && (
        <ConnectionDialog
          connection={connection}
          connected={connected}
          desktop={desktop}
          onClose={() => setOpen(false)}
          onSave={onSave}
          onTest={onTest}
          onDisconnect={() => {
            onDisconnect();
            setOpen(false);
          }}
        />
      )}
    </>
  );
}

function describeStatus(status: ConnectionStatus, connection: ConnectionView | null): string {
  switch (status.kind) {
    case "disconnected":
      return "Not connected";
    case "checking":
      return "Connecting…";
    case "connected":
      return `${shortHost(connection?.url)} · ${status.durationMs} ms`;
    case "failed":
      return "Connection failed";
  }
}

function shortHost(url: string | undefined): string {
  if (!url) return "Connected";
  try {
    return new URL(url.replace(" (via the Vite dev proxy)", "")).host;
  } catch {
    return url;
  }
}

type ConnectionMode = "local" | "cloud";

function ConnectionDialog({
  connection,
  connected,
  desktop,
  onClose,
  onSave,
  onTest,
  onDisconnect,
}: {
  connection: ConnectionView | null;
  connected: boolean;
  desktop: boolean;
  onClose: () => void;
  onSave: Props["onSave"];
  onTest: Props["onTest"];
  onDisconnect: () => void;
}) {
  const initial = useMemo(() => parseConnection(connection), [connection]);
  const [mode, setMode] = useState<ConnectionMode>(initial.mode);
  const [host, setHost] = useState(initial.host);
  const [port, setPort] = useState(initial.port);
  const [cloudUrl, setCloudUrl] = useState(initial.cloudUrl);
  const [apiKey, setApiKey] = useState("");
  const [clearKey, setClearKey] = useState(false);
  const [timeout, setTimeoutMs] = useState(connection?.timeoutMs ?? 30_000);
  const [writerOnly, setWriterOnly] = useState(connection?.writerOnly ?? false);
  const [advanced, setAdvanced] = useState(false);
  const [busy, setBusy] = useState<"test" | "connect" | null>(null);
  const [feedback, setFeedback] = useState<{ kind: "success" | "error"; message: string } | null>(null);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && busy === null) onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [busy, onClose]);

  const candidate = (): ConnectionUpdate => ({
    url: mode === "local" ? buildLocalUrl(host, port) : normalizeCloudUrl(cloudUrl),
    apiKey: clearKey ? "" : apiKey.length > 0 ? apiKey : undefined,
    timeoutMs: timeout,
    writerOnly,
  });

  const validate = () => {
    if (mode === "local" && !host.trim()) throw new Error("Host address is required.");
    if (mode === "local" && !/^\d+$/.test(port.trim())) throw new Error("Enter a valid port number.");
    if (mode === "cloud" && !cloudUrl.trim()) throw new Error("Cloud instance URL is required.");
    if (mode === "cloud" && !connection?.hasApiKey && !apiKey.trim()) throw new Error("An API key is required for a new cloud connection.");
  };

  const test = async () => {
    setBusy("test");
    setFeedback(null);
    try {
      validate();
      const durationMs = await onTest(candidate());
      setFeedback({ kind: "success", message: `Connection succeeded in ${durationMs} ms.` });
    } catch (error) {
      setFeedback({ kind: "error", message: error instanceof Error ? error.message : String(error) });
    } finally {
      setBusy(null);
    }
  };

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setBusy("connect");
    setFeedback(null);
    try {
      validate();
      await onSave(candidate());
      onClose();
    } catch (error) {
      setFeedback({ kind: "error", message: error instanceof Error ? error.message : String(error) });
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(event) => {
      if (event.target === event.currentTarget && busy === null) onClose();
    }}>
      <form className="connection-dialog" onSubmit={submit} aria-labelledby="connection-title">
        <header className="dialog-header">
          <div>
            <h2 id="connection-title">Connection</h2>
            <p>Connect Helix Visualizer to a local or cloud HelixDB instance.</p>
          </div>
          <button type="button" className="icon-button" onClick={onClose} disabled={busy !== null} aria-label="Close">×</button>
        </header>

        <div className="mode-picker" role="radiogroup" aria-label="Connection type">
          <ModeButton mode="local" active={mode} onSelect={setMode} title="Local" detail="Host and port" />
          <ModeButton mode="cloud" active={mode} onSelect={setMode} title="Cloud" detail="URL and API key" />
        </div>

        <div className="dialog-fields">
          {mode === "local" ? (
            <>
              <div className="field-row">
                <label className="field-label grow">
                  <span>Host</span>
                  <input value={host} onChange={(event) => setHost(event.target.value)} placeholder="127.0.0.1" />
                </label>
                <label className="field-label port-field">
                  <span>Host port</span>
                  <input value={port} inputMode="numeric" onChange={(event) => setPort(event.target.value)} placeholder="6969" />
                </label>
              </div>
              <p className="port-hint">
                For a Docker mapping such as <code>6969:8080</code>, enter the left-side host port: <code>6969</code>.
              </p>
            </>
          ) : (
            <>
              <label className="field-label">
                <span>Cloud instance URL</span>
                <input value={cloudUrl} onChange={(event) => setCloudUrl(event.target.value)} placeholder="https://your-instance.example.com" />
              </label>
              <label className="field-label">
                <span>Cluster API key</span>
                <input
                  type="password"
                  value={apiKey}
                  onChange={(event) => setApiKey(event.target.value)}
                  placeholder={connection?.hasApiKey ? "•••••••• (saved)" : "Enter API key"}
                  disabled={clearKey}
                />
              </label>
              {connection?.hasApiKey ? (
                <label className="check-field">
                  <input type="checkbox" checked={clearKey} onChange={(event) => setClearKey(event.target.checked)} />
                  Remove the saved API key
                </label>
              ) : null}
            </>
          )}

          <button type="button" className="advanced-toggle" onClick={() => setAdvanced((value) => !value)} aria-expanded={advanced}>
            <span aria-hidden="true">{advanced ? "⌄" : "›"}</span> Advanced options
          </button>

          {advanced ? (
            <div className="advanced-fields">
              <label className="field-label">
                <span>Request timeout (ms)</span>
                <input type="number" min={1000} max={600000} step={1000} value={timeout} onChange={(event) => setTimeoutMs(Number(event.target.value))} />
              </label>
              <label className="check-field">
                <input type="checkbox" checked={writerOnly} onChange={(event) => setWriterOnly(event.target.checked)} />
                Require a writer node
              </label>
            </div>
          ) : null}

          {!desktop ? (
            <p className="browser-note">Browser preview uses the Vite <code>/helix</code> proxy. Set <code>HELIX_URL</code> when starting Vite to change the actual target.</p>
          ) : null}

          {feedback ? <div className={`connection-feedback ${feedback.kind}`} role="status">{feedback.message}</div> : null}
        </div>

        <footer className="dialog-actions">
          {connected ? <button type="button" className="danger-button" onClick={onDisconnect} disabled={busy !== null}>Disconnect</button> : <span />}
          <div>
            <button type="button" onClick={test} disabled={busy !== null}>{busy === "test" ? "Testing…" : "Test connection"}</button>
            <button type="submit" className="primary" disabled={busy !== null}>{busy === "connect" ? "Connecting…" : connected ? "Reconnect" : "Connect"}</button>
          </div>
        </footer>
      </form>
    </div>
  );
}

function ModeButton({ mode, active, onSelect, title, detail }: { mode: ConnectionMode; active: ConnectionMode; onSelect: (mode: ConnectionMode) => void; title: string; detail: string }) {
  const selected = active === mode;
  return (
    <button type="button" role="radio" aria-checked={selected} className={selected ? "mode-card active" : "mode-card"} onClick={() => onSelect(mode)}>
      <ToolbarIcon name={mode === "local" ? "desktop" : "cloud"} />
      <span><strong>{title}</strong><small>{detail}</small></span>
      <i aria-hidden="true">{selected ? "✓" : ""}</i>
    </button>
  );
}

function parseConnection(connection: ConnectionView | null) {
  const fallback = { mode: "local" as const, host: "127.0.0.1", port: "6969", cloudUrl: "" };
  if (!connection) return fallback;
  const raw = connection.url.replace(" (via the Vite dev proxy)", "");
  try {
    const url = new URL(raw);
    const isLocal = ["localhost", "127.0.0.1", "::1"].includes(url.hostname) && url.protocol === "http:";
    return {
      mode: (isLocal ? "local" : "cloud") as ConnectionMode,
      host: isLocal ? (url.hostname === "localhost" ? "127.0.0.1" : url.hostname) : "127.0.0.1",
      port: isLocal ? (url.port || "6969") : "6969",
      cloudUrl: isLocal ? "" : raw,
    };
  } catch {
    return fallback;
  }
}

function buildLocalUrl(host: string, port: string): string {
  const normalizedHost = host.trim().toLowerCase() === "localhost" ? "127.0.0.1" : host.trim();
  return `http://${normalizedHost}:${port.trim()}`;
}

function normalizeCloudUrl(value: string): string {
  const trimmed = value.trim().replace(/\/+$/, "");
  return trimmed.includes("://") ? trimmed : `https://${trimmed}`;
}

function ToolbarIcon({ name, connected = false }: { name: "connection" | "query" | "schema" | "graph" | "sun" | "moon" | "desktop" | "cloud"; connected?: boolean }) {
  const common = { width: 27, height: 27, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 1.7, strokeLinecap: "round" as const, strokeLinejoin: "round" as const, "aria-hidden": true };
  if (name === "connection") return <svg {...common}><path d="M8 7V4m8 3V4M6 7h12v4a6 6 0 0 1-12 0V7Z"/><path d="M12 17v3"/>{connected ? <circle cx="12" cy="11" r="2" fill="currentColor" stroke="none"/> : null}</svg>;
  if (name === "query") return <svg {...common}><rect x="4" y="4" width="16" height="16" rx="3"/><path d="m8 9 2.5 2.5L8 14m5 0h3"/></svg>;
  if (name === "schema") return <svg {...common}><rect x="4" y="4" width="16" height="16" rx="4"/><circle cx="12" cy="12" r="3"/><path d="M12 4v5m0 6v5M4 12h5m6 0h5"/></svg>;
  if (name === "graph") return <svg {...common}><circle cx="6" cy="12" r="2.4"/><circle cx="17" cy="6" r="2.4"/><circle cx="18" cy="17" r="2.4"/><path d="m8.1 10.9 6.7-3.7m-6.4 6 7.3 2.8m1.4-7.6.6 6.2"/></svg>;
  if (name === "sun") return <svg {...common}><circle cx="12" cy="12" r="3.5"/><path d="M12 2v2m0 16v2M4.9 4.9l1.4 1.4m11.4 11.4 1.4 1.4M2 12h2m16 0h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/></svg>;
  if (name === "moon") return <svg {...common}><path d="M20 15.2A8.3 8.3 0 0 1 8.8 4a8.4 8.4 0 1 0 11.2 11.2Z"/></svg>;
  if (name === "desktop") return <svg {...common}><rect x="3" y="4" width="18" height="13" rx="2"/><path d="M8 21h8m-4-4v4"/></svg>;
  return <svg {...common}><path d="M7 18h11a4 4 0 0 0 .7-7.9A7 7 0 0 0 5.3 8.7 4.7 4.7 0 0 0 7 18Z"/></svg>;
}
