import { useEffect, useState } from "react";

import type { ConnectionView } from "../client";

export type ConnectionStatus =
  | { kind: "unknown" }
  | { kind: "checking" }
  | { kind: "connected"; durationMs: number }
  | { kind: "failed"; message: string };

interface Props {
  connection: ConnectionView | null;
  status: ConnectionStatus;
  desktop: boolean;
  theme: "light" | "dark";
  onToggleTheme: () => void;
  onSave: (update: { url: string; apiKey?: string; timeoutMs: number; writerOnly: boolean }) => Promise<void>;
  onTest: () => void;
}

export function ConnectionBar({
  connection,
  status,
  desktop,
  theme,
  onToggleTheme,
  onSave,
  onTest,
}: Props) {
  const [open, setOpen] = useState(false);

  return (
    <header className="app-bar">
      <div className="brand">
        <span className="brand-mark" aria-hidden="true" />
        <span className="brand-name">Helix Visualizer</span>
      </div>

      <button
        type="button"
        className={`connection-chip status-${status.kind}`}
        onClick={() => setOpen((value) => !value)}
        aria-expanded={open}
      >
        <span className="status-dot" aria-hidden="true" />
        <span className="connection-url">{connection?.url ?? "not configured"}</span>
        <span className="connection-detail">{describeStatus(status)}</span>
      </button>

      <div className="app-bar-actions">
        <button type="button" onClick={onTest} disabled={status.kind === "checking"}>
          Test
        </button>
        <button type="button" onClick={onToggleTheme} title="Switch theme">
          {theme === "dark" ? "Light" : "Dark"}
        </button>
      </div>

      {open && connection && (
        <ConnectionForm
          connection={connection}
          desktop={desktop}
          onClose={() => setOpen(false)}
          onSave={onSave}
        />
      )}
    </header>
  );
}

function describeStatus(status: ConnectionStatus): string {
  switch (status.kind) {
    case "unknown":
      return "not checked";
    case "checking":
      return "checking…";
    case "connected":
      return `connected · ${status.durationMs} ms`;
    case "failed":
      return status.message;
  }
}

function ConnectionForm({
  connection,
  desktop,
  onClose,
  onSave,
}: {
  connection: ConnectionView;
  desktop: boolean;
  onClose: () => void;
  onSave: Props["onSave"];
}) {
  const [url, setUrl] = useState(connection.url);
  const [apiKey, setApiKey] = useState("");
  const [clearKey, setClearKey] = useState(false);
  const [timeout, setTimeoutMs] = useState(connection.timeoutMs);
  const [writerOnly, setWriterOnly] = useState(connection.writerOnly);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setSaving(true);
    setError(null);
    try {
      await onSave({
        url,
        // Leaving the field blank keeps the saved key rather than wiping it.
        apiKey: clearKey ? "" : apiKey.length > 0 ? apiKey : undefined,
        timeoutMs: timeout,
        writerOnly,
      });
      onClose();
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
    } finally {
      setSaving(false);
    }
  };

  return (
    <form className="connection-panel" onSubmit={submit}>
      <label>
        Instance URL
        <input
          value={url}
          onChange={(event) => setUrl(event.target.value)}
          placeholder="http://localhost:6969"
          autoFocus
        />
      </label>

      <label>
        API key
        <input
          type="password"
          value={apiKey}
          onChange={(event) => setApiKey(event.target.value)}
          placeholder={connection.hasApiKey ? "•••••••• (saved)" : "none"}
          disabled={clearKey}
        />
      </label>
      {connection.hasApiKey && (
        <label className="checkbox">
          <input type="checkbox" checked={clearKey} onChange={(e) => setClearKey(e.target.checked)} />
          Remove the saved key
        </label>
      )}

      <label>
        Timeout (ms)
        <input
          type="number"
          min={1000}
          max={600000}
          step={1000}
          value={timeout}
          onChange={(event) => setTimeoutMs(Number(event.target.value))}
        />
      </label>

      <label className="checkbox">
        <input
          type="checkbox"
          checked={writerOnly}
          onChange={(event) => setWriterOnly(event.target.checked)}
        />
        Require a writer node
      </label>

      {!desktop && (
        <p className="hint-text">
          Running in a browser: requests go through the Vite <code>/helix</code> proxy, so the URL
          here is only a label. Change <code>HELIX_URL</code> when starting the dev server instead.
        </p>
      )}

      {error && <p className="panel-error">{error}</p>}

      <div className="panel-actions">
        <button type="button" onClick={onClose}>
          Cancel
        </button>
        <button type="submit" className="primary" disabled={saving}>
          {saving ? "Saving…" : "Save"}
        </button>
      </div>
    </form>
  );
}
