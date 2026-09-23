import { useEffect, useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
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
    <Dialog open={open} onOpenChange={setOpen}>
      <header className="app-header">
        <div className="title-bar" data-tauri-drag-region>
          <div className="title-brand">
            <span className="brand-mark" aria-hidden="true"><span /></span>
            <span>Helix Visualizer</span>
          </div>
        </div>

        <div className="top-toolbar">
          <Button
            variant="ghost"
            className={connected ? "toolbar-item connection-item connected" : "toolbar-item connection-item"}
            onClick={() => setOpen(true)}
            title={connected ? `Connected to ${connection?.url ?? "HelixDB"}` : "Configure a connection"}
          >
            <ToolbarIcon name="connection" connected={connected} />
            <span>Connection</span>
            <i className={`toolbar-status status-${status.kind}`} aria-hidden="true" />
          </Button>

          <Separator orientation="vertical" className="toolbar-divider" />

          <nav className="top-nav" aria-label="Main views">
            {NAV_ITEMS.map((item) => (
              <Button
                variant="ghost"
                key={item.id}
                className={activeView === item.id ? `toolbar-item nav-${item.id} active` : `toolbar-item nav-${item.id}`}
                aria-current={activeView === item.id ? "page" : undefined}
                onClick={() => onSelectView(item.id)}
              >
                <ToolbarIcon name={item.icon} />
                <span>{item.label}</span>
              </Button>
            ))}
          </nav>

          <div className="toolbar-spacer" />

          <div className={`connection-summary status-${status.kind}`}>
            <span className="status-dot" aria-hidden="true" />
            <span>{describeStatus(status, connection)}</span>
          </div>

          <Button variant="ghost" size="icon" className="theme-button" onClick={onToggleTheme} title="Switch theme" aria-label="Switch theme">
            <ToolbarIcon name={theme === "dark" ? "sun" : "moon"} />
          </Button>
        </div>
      </header>

      {open ? (
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
      ) : null}
    </Dialog>
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

type ConnectionMode = "local" | "remote";

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
  const [remoteUrl, setRemoteUrl] = useState(initial.remoteUrl);
  const [apiKey, setApiKey] = useState("");
  const [clearKey, setClearKey] = useState(false);
  const [timeout, setTimeoutMs] = useState(connection?.timeoutMs ?? 30_000);
  const [writerOnly, setWriterOnly] = useState(connection?.writerOnly ?? false);
  const [advanced, setAdvanced] = useState(false);
  const [busy, setBusy] = useState<"test" | "connect" | null>(null);
  const [feedback, setFeedback] = useState<{ kind: "success" | "error"; message: string } | null>(null);

  const candidate = (): ConnectionUpdate => ({
    url: mode === "local" ? buildLocalUrl(host, port) : normalizeRemoteUrl(remoteUrl),
    apiKey: clearKey ? "" : apiKey.length > 0 ? apiKey : undefined,
    timeoutMs: timeout,
    writerOnly,
  });

  const validate = () => {
    if (mode === "local" && !host.trim()) throw new Error("Host address is required.");
    const portNumber = Number(port);
    if (mode === "local" && (!/^\d+$/.test(port.trim()) || portNumber < 1 || portNumber > 65_535)) {
      throw new Error("Enter a port number from 1 to 65535.");
    }
    if (mode === "remote" && !remoteUrl.trim()) throw new Error("Remote server URL is required.");
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
    <DialogContent
      className="connection-dialog"
      showCloseButton={false}
      onEscapeKeyDown={(event) => { if (busy !== null) event.preventDefault(); }}
      onPointerDownOutside={(event) => { if (busy !== null) event.preventDefault(); }}
    >
      <form onSubmit={submit} aria-labelledby="connection-title">
        <DialogHeader className="dialog-header">
          <div>
            <DialogTitle id="connection-title">Connection</DialogTitle>
            <DialogDescription>Connect to HelixDB locally, on a remote server, or in the cloud.</DialogDescription>
          </div>
          <Button variant="ghost" size="icon-sm" className="icon-button" onClick={onClose} disabled={busy !== null} aria-label="Close">×</Button>
        </DialogHeader>

        <div className="mode-picker" role="radiogroup" aria-label="Connection type">
          <ModeButton mode="local" active={mode} onSelect={setMode} title="Local" detail="Host and port" />
          <ModeButton mode="remote" active={mode} onSelect={setMode} title="Remote / Cloud" detail="URL and optional API key" />
        </div>

        <div className="dialog-fields">
          {mode === "local" ? (
            <>
              <div className="field-row">
                <Label className="field-label grow">
                  <span>Host</span>
                  <Input value={host} onChange={(event) => setHost(event.target.value)} placeholder="127.0.0.1" />
                </Label>
                <Label className="field-label port-field">
                  <span>Host port</span>
                  <Input value={port} inputMode="numeric" onChange={(event) => setPort(event.target.value)} placeholder="6969" />
                </Label>
              </div>
              <p className="port-hint">
                For a Docker mapping such as <code>6969:8080</code>, enter the left-side host port: <code>6969</code>.
              </p>
            </>
          ) : (
            <Label className="field-label">
              <span>Remote server URL</span>
              <Input value={remoteUrl} onChange={(event) => setRemoteUrl(event.target.value)} placeholder="https://your-instance.example.com" />
            </Label>
          )}

          <Label className="field-label">
            <span>API key <small>(optional)</small></span>
            <Input
              type="password"
              value={apiKey}
              onChange={(event) => setApiKey(event.target.value)}
              placeholder={connection?.hasApiKey ? "•••••••• (saved)" : "Only if required by the server"}
              disabled={clearKey}
            />
          </Label>
          {connection?.hasApiKey ? (
            <Label className="check-field">
              <Checkbox checked={clearKey} onCheckedChange={(checked) => setClearKey(checked === true)} />
              Remove the saved API key
            </Label>
          ) : null}

          <Button variant="ghost" size="sm" className="advanced-toggle" onClick={() => setAdvanced((value) => !value)} aria-expanded={advanced}>
            <span aria-hidden="true">{advanced ? "⌄" : "›"}</span> Advanced options
          </Button>

          {advanced ? (
            <div className="advanced-fields">
              <Label className="field-label">
                <span>Request timeout (ms)</span>
                <Input type="number" min={1000} max={600000} step={1000} value={timeout} onChange={(event) => setTimeoutMs(Number(event.target.value))} />
              </Label>
              <Label className="check-field">
                <Checkbox checked={writerOnly} onCheckedChange={(checked) => setWriterOnly(checked === true)} />
                Require a writer node
              </Label>
            </div>
          ) : null}

          {!desktop ? (
            <p className="browser-note">
              Connection and query commands require the Rust process. Launch the functional app with <code>npm run app</code>.
            </p>
          ) : null}

          {feedback ? <div className={`connection-feedback ${feedback.kind}`} role="status">{feedback.message}</div> : null}
        </div>

        <footer className="dialog-actions">
          {connected ? <Button variant="destructive" className="danger-button" onClick={onDisconnect} disabled={busy !== null}>Disconnect</Button> : <span />}
          <div>
            <Button variant="outline" onClick={test} disabled={busy !== null || !desktop}>{busy === "test" ? "Testing…" : "Test connection"}</Button>
            <Button type="submit" variant="default" className="primary" disabled={busy !== null || !desktop}>{busy === "connect" ? "Connecting…" : connected ? "Reconnect" : "Connect"}</Button>
          </div>
        </footer>
      </form>
    </DialogContent>
  );
}

function ModeButton({ mode, active, onSelect, title, detail }: { mode: ConnectionMode; active: ConnectionMode; onSelect: (mode: ConnectionMode) => void; title: string; detail: string }) {
  const selected = active === mode;
  return (
    <Button variant="outline" role="radio" aria-checked={selected} className={selected ? "mode-card active h-auto" : "mode-card h-auto"} onClick={() => onSelect(mode)}>
      <ToolbarIcon name={mode === "local" ? "desktop" : "cloud"} />
      <span><strong>{title}</strong><small>{detail}</small></span>
      <i aria-hidden="true">{selected ? "✓" : ""}</i>
    </Button>
  );
}

function parseConnection(connection: ConnectionView | null) {
  const fallback = { mode: "local" as const, host: "127.0.0.1", port: "6969", remoteUrl: "" };
  if (!connection) return fallback;
  const raw = connection.url.replace(" (via the Vite dev proxy)", "");
  try {
    const url = new URL(raw);
    const isLocal = ["localhost", "127.0.0.1", "::1", "[::1]"].includes(url.hostname) && url.protocol === "http:";
    return {
      mode: (isLocal ? "local" : "remote") as ConnectionMode,
      host: isLocal ? (url.hostname === "localhost" ? "127.0.0.1" : url.hostname) : "127.0.0.1",
      port: isLocal ? (url.port || "6969") : "6969",
      remoteUrl: isLocal ? "" : raw,
    };
  } catch {
    return fallback;
  }
}

function buildLocalUrl(host: string, port: string): string {
  const normalizedHost = host.trim().toLowerCase() === "localhost" ? "127.0.0.1" : host.trim();
  const urlHost = normalizedHost.includes(":") && !normalizedHost.startsWith("[")
    ? `[${normalizedHost}]`
    : normalizedHost;
  return `http://${urlHost}:${port.trim()}`;
}

function normalizeRemoteUrl(value: string): string {
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
