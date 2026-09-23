import { ChevronRight, CircleAlert, CircleCheck, Cloud, Monitor, X } from "lucide-react";
import { RadioGroup } from "radix-ui";
import { useId, useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { DialogClose, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import type { ConnectionUpdate, ConnectionView } from "../client";
import { Spinner } from "./feedback";

type ConnectionMode = "local" | "remote";

interface Props {
  connection: ConnectionView | null;
  connected: boolean;
  desktop: boolean;
  onClose: () => void;
  onSave: (update: ConnectionUpdate) => Promise<void>;
  onTest: (update: ConnectionUpdate) => Promise<number>;
  onDisconnect: () => void;
}

const MODES: Array<{ id: ConnectionMode; title: string; detail: string; icon: typeof Monitor }> = [
  { id: "local", title: "Local", detail: "Host and port", icon: Monitor },
  { id: "remote", title: "Remote / Cloud", detail: "URL and optional API key", icon: Cloud },
];

export function ConnectionDialog({
  connection,
  connected,
  desktop,
  onClose,
  onSave,
  onTest,
  onDisconnect,
}: Props) {
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
  const ids = useId();

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
      onEscapeKeyDown={(event) => { if (busy !== null) event.preventDefault(); }}
      onPointerDownOutside={(event) => { if (busy !== null) event.preventDefault(); }}
    >
      <form onSubmit={submit} aria-labelledby={`${ids}-title`}>
        <header className="dialog-header">
          <div>
            <DialogTitle id={`${ids}-title`}>Connect to HelixDB</DialogTitle>
            <DialogDescription>A local instance, a server on your network, or a cloud deployment.</DialogDescription>
          </div>
          <DialogClose asChild>
            <Button variant="ghost" size="icon-sm" disabled={busy !== null} aria-label="Close">
              <X />
            </Button>
          </DialogClose>
        </header>

        <div className="dialog-body">
          <RadioGroup.Root
            className="mode-picker"
            value={mode}
            onValueChange={(value) => setMode(value as ConnectionMode)}
            aria-label="Connection type"
          >
            {MODES.map(({ id, title, detail, icon: Icon }) => (
              <RadioGroup.Item key={id} value={id} className="mode-option">
                <Icon aria-hidden="true" strokeWidth={1.75} />
                <span>
                  <strong>{title}</strong>
                  <small>{detail}</small>
                </span>
                <span className="mode-option-radio" aria-hidden="true">
                  <RadioGroup.Indicator className="mode-option-dot" />
                </span>
              </RadioGroup.Item>
            ))}
          </RadioGroup.Root>

          <div className="field-grid">
            {mode === "local" ? (
              <>
                <div className="field-row">
                  <div className="field grow">
                    <Label htmlFor={`${ids}-host`}>Host</Label>
                    <Input id={`${ids}-host`} value={host} onChange={(event) => setHost(event.target.value)} placeholder="127.0.0.1" autoComplete="off" spellCheck={false} />
                  </div>
                  <div className="field port-field">
                    <Label htmlFor={`${ids}-port`}>Host port</Label>
                    <Input id={`${ids}-port`} value={port} inputMode="numeric" onChange={(event) => setPort(event.target.value)} placeholder="6969" className="font-mono" aria-describedby={`${ids}-port-hint`} />
                  </div>
                </div>
                <p className="field-hint" id={`${ids}-port-hint`}>
                  For a Docker mapping such as <code>6969:8080</code>, enter the host-side port <code>6969</code>.
                </p>
              </>
            ) : (
              <div className="field">
                <Label htmlFor={`${ids}-url`}>Server URL</Label>
                <Input id={`${ids}-url`} value={remoteUrl} onChange={(event) => setRemoteUrl(event.target.value)} placeholder="https://your-instance.example.com" autoComplete="url" spellCheck={false} className="font-mono" />
              </div>
            )}

            <div className="field">
              <Label htmlFor={`${ids}-key`}>
                API key <span className="field-optional">optional</span>
              </Label>
              <Input
                id={`${ids}-key`}
                type="password"
                value={apiKey}
                onChange={(event) => setApiKey(event.target.value)}
                placeholder={connection?.hasApiKey ? "Saved key in use" : "Only if the server requires one"}
                disabled={clearKey}
                autoComplete="off"
              />
            </div>
            {connection?.hasApiKey ? (
              <Label className="check-field">
                <Checkbox checked={clearKey} onCheckedChange={(checked) => setClearKey(checked === true)} />
                Remove the saved API key
              </Label>
            ) : null}

            <div className="disclosure">
              <button
                type="button"
                className="disclosure-toggle"
                onClick={() => setAdvanced((value) => !value)}
                aria-expanded={advanced}
                aria-controls={`${ids}-advanced`}
              >
                <ChevronRight aria-hidden="true" />
                Advanced
              </button>
              {advanced ? (
                <div className="disclosure-body" id={`${ids}-advanced`}>
                  <div className="field">
                    <Label htmlFor={`${ids}-timeout`}>Request timeout (ms)</Label>
                    <Input id={`${ids}-timeout`} type="number" min={1000} max={600000} step={1000} value={timeout} onChange={(event) => setTimeoutMs(Number(event.target.value))} className="font-mono" />
                  </div>
                  <Label className="check-field">
                    <Checkbox checked={writerOnly} onCheckedChange={(checked) => setWriterOnly(checked === true)} />
                    Require a writer node
                  </Label>
                </div>
              ) : null}
            </div>

            {!desktop ? (
              <p className="dialog-note">
                Connecting needs the Rust process. Launch the full app with <code>npm run app</code>.
              </p>
            ) : null}

            {feedback ? (
              <div className="dialog-feedback" data-kind={feedback.kind} role={feedback.kind === "error" ? "alert" : "status"}>
                {feedback.kind === "success" ? <CircleCheck aria-hidden="true" /> : <CircleAlert aria-hidden="true" />}
                <span>{feedback.message}</span>
              </div>
            ) : null}
          </div>
        </div>

        <footer className="dialog-footer">
          {connected ? (
            <Button variant="destructive" onClick={onDisconnect} disabled={busy !== null}>Disconnect</Button>
          ) : <span />}
          <div className="dialog-footer-actions">
            <Button variant="outline" onClick={test} disabled={busy !== null || !desktop}>
              {busy === "test" ? <Spinner /> : null}
              {busy === "test" ? "Testing…" : "Test"}
            </Button>
            <Button type="submit" variant="default" disabled={busy !== null || !desktop}>
              {busy === "connect" ? <Spinner /> : null}
              {busy === "connect" ? "Connecting…" : connected ? "Reconnect" : "Connect"}
            </Button>
          </div>
        </footer>
      </form>
    </DialogContent>
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
