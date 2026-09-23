import { ListTree, Moon, SquareTerminal, Sun, Waypoints, type LucideIcon } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import type { AppView } from "../App";
import type { ConnectionUpdate, ConnectionView } from "../client";
import { BrandMark } from "./BrandMark";
import { ConnectionDialog } from "./ConnectionDialog";

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
  /** View-specific controls, such as the drawer toggles on narrow windows. */
  tools?: ReactNode;
  onSelectView: (view: AppView) => void;
  onToggleTheme: () => void;
  onSave: (update: ConnectionUpdate) => Promise<void>;
  onTest: (update: ConnectionUpdate) => Promise<number>;
  onDisconnect: () => void;
}

const NAV_ITEMS: Array<{ id: AppView; label: string; icon: LucideIcon }> = [
  { id: "query", label: "Query", icon: SquareTerminal },
  { id: "schema", label: "Schema", icon: ListTree },
  { id: "graph", label: "Graph", icon: Waypoints },
];

export function ConnectionBar({
  connection,
  status,
  desktop,
  theme,
  activeView,
  openRequest,
  tools,
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
      <header className="topbar" data-tauri-drag-region>
        <div className="topbar-brand" data-tauri-drag-region>
          <BrandMark size={20} />
          <span className="topbar-wordmark">
            Helix<span> Visualizer</span>
          </span>
        </div>

        <nav className="topnav" aria-label="Workspaces">
          {NAV_ITEMS.map(({ id, label, icon: Icon }) => (
            <button
              type="button"
              key={id}
              className="topnav-item"
              aria-current={activeView === id ? "page" : undefined}
              onClick={() => onSelectView(id)}
              title={label}
            >
              <Icon aria-hidden="true" strokeWidth={1.75} />
              <span>{label}</span>
            </button>
          ))}
        </nav>

        <div className="topbar-spacer" data-tauri-drag-region />

        {tools}

        <button
          type="button"
          className="connection-button"
          data-status={status.kind}
          onClick={() => setOpen(true)}
          aria-haspopup="dialog"
          title={connected ? `Connected to ${connection?.url ?? "HelixDB"}` : "Configure a connection"}
        >
          <span className="status-dot" aria-hidden="true" />
          <span className="connection-button-text">{describeStatus(status, connection)}</span>
          {status.kind === "connected" ? (
            <span className="connection-button-meta">{status.durationMs} ms</span>
          ) : null}
        </button>

        <Button
          variant="ghost"
          size="icon"
          onClick={onToggleTheme}
          title={theme === "dark" ? "Use light theme" : "Use dark theme"}
          aria-label={theme === "dark" ? "Use light theme" : "Use dark theme"}
        >
          {theme === "dark" ? <Sun strokeWidth={1.75} /> : <Moon strokeWidth={1.75} />}
        </Button>
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
      return "Connect";
    case "checking":
      return "Connecting…";
    case "connected":
      return shortHost(connection?.url);
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
