import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

export function Spinner({ className, label }: { className?: string; label?: string }) {
  return (
    <span
      className={cn("spinner", className)}
      role={label ? "status" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
    />
  );
}

/**
 * The one placard used for every empty, loading and failure state, so the
 * whole app answers "why is nothing here?" in the same voice.
 */
export function EmptyState({
  icon: Icon,
  loading = false,
  tone = "neutral",
  title,
  children,
  action,
  className,
}: {
  icon?: LucideIcon;
  loading?: boolean;
  tone?: "neutral" | "danger";
  title: string;
  children?: ReactNode;
  action?: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn("empty-state", className)}
      data-tone={tone}
      role={tone === "danger" ? "alert" : loading ? "status" : undefined}
    >
      <div className="empty-state-body">
        <span className="empty-state-icon" aria-hidden="true">
          {loading ? <Spinner /> : Icon ? <Icon strokeWidth={1.75} /> : null}
        </span>
        <h2>{title}</h2>
        {children ? <div className="empty-state-text">{children}</div> : null}
        {action ? <div className="empty-state-action">{action}</div> : null}
      </div>
    </div>
  );
}

/** An inline error block for failures inside an otherwise working surface. */
export function ErrorNotice({ message, detail }: { message: string; detail?: string | null }) {
  return (
    <div className="error-notice" role="alert">
      <strong>{message}</strong>
      {detail ? <pre>{detail}</pre> : null}
    </div>
  );
}
