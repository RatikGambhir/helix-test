import { X } from "lucide-react";
import type { ReactNode } from "react";

import { Button } from "@/components/ui/button";

/**
 * A side panel that is docked on wide windows and becomes an overlay drawer on
 * narrow ones. Which of the two applies is decided purely by CSS breakpoints;
 * `open` only matters while the pane is in drawer mode.
 */
export function Pane({
  id,
  side,
  title,
  open,
  onClose,
  actions,
  children,
}: {
  id: string;
  side: "library" | "inspector";
  title: string;
  open: boolean;
  onClose: () => void;
  actions?: ReactNode;
  children: ReactNode;
}) {
  return (
    <>
      <aside id={id} className={`pane pane--${side}`} data-open={open} aria-label={title}>
        <header className="strip pane-header">
          <h2 className="eyebrow">{title}</h2>
          <div className="strip-actions">
            {actions}
            <Button variant="ghost" size="icon-sm" className="pane-close" onClick={onClose} aria-label={`Close ${title.toLowerCase()}`}>
              <X />
            </Button>
          </div>
        </header>
        <div className="pane-body">{children}</div>
      </aside>
      {open ? (
        <div className={`pane-scrim pane-scrim--${side}`} aria-hidden="true" onClick={onClose} />
      ) : null}
    </>
  );
}
