import { CircleCheck, CircleX, Play } from "lucide-react";
import { useEffect, useMemo, useRef } from "react";

import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/kbd";
import { Textarea } from "@/components/ui/textarea";
import type { BackendError } from "../client";
import { Spinner } from "./feedback";

export type CompileStatus =
  | { kind: "empty" }
  | { kind: "checking" }
  | { kind: "valid"; summary: string }
  | { kind: "invalid" };

interface Props {
  value: string;
  onChange: (value: string) => void;
  onRun: () => void;
  running: boolean;
  /** Live validation state of the current text, as reported by Rust. */
  status: CompileStatus;
  /** Parse/compile failure for the current text, if any. */
  error: BackendError | null;
}

const IS_MAC = typeof navigator !== "undefined" && navigator.platform.includes("Mac");

/**
 * A textarea with a gutter and an inline error marker.
 *
 * Deliberately not a full code editor: the language is small, and a plain
 * textarea keeps selection, undo and IME behaviour native.
 */
export function QueryEditor({ value, onChange, onRun, running, status, error }: Props) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const lineCount = useMemo(() => Math.max(value.split("\n").length, 1), [value]);
  const runnable = status.kind !== "empty" && status.kind !== "invalid";

  useEffect(() => {
    textareaRef.current?.focus();
  }, []);

  const handleKeyDown = (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    // Ctrl/Cmd+Enter runs; a bare Enter stays a newline so queries can be typed
    // across several lines the way SQL usually is.
    if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
      event.preventDefault();
      onRun();
      return;
    }
    if (event.key === "Tab" && !event.shiftKey) {
      event.preventDefault();
      const target = event.currentTarget;
      const { selectionStart, selectionEnd } = target;
      const next = `${value.slice(0, selectionStart)}  ${value.slice(selectionEnd)}`;
      onChange(next);
      requestAnimationFrame(() => {
        target.selectionStart = target.selectionEnd = selectionStart + 2;
      });
    }
  };

  /** Moves the caret to the offending token so the fix is one keystroke away. */
  const jumpToError = () => {
    const textarea = textareaRef.current;
    if (!textarea || !error?.span) return;
    textarea.focus();
    textarea.setSelectionRange(error.span.start, Math.max(error.span.end, error.span.start + 1));
  };

  return (
    <section className="editor" aria-labelledby="editor-title">
      <header className="strip">
        <h2 id="editor-title" className="eyebrow">HelixSQL</h2>
        <CompileIndicator status={status} />
        <div className="strip-actions">
          <span className="kbd-hint" aria-hidden="true">
            <Kbd>{IS_MAC ? "⌘" : "Ctrl"}</Kbd>
            <Kbd>↵</Kbd>
          </span>
          <Button
            variant="default"
            size="sm"
            onClick={onRun}
            disabled={running || !runnable}
            title={runnable ? `Run query (${IS_MAC ? "⌘" : "Ctrl"}+Enter)` : "Fix the query before running it"}
          >
            {running ? <Spinner /> : <Play fill="currentColor" />}
            {running ? "Running" : "Run"}
          </Button>
        </div>
      </header>

      <div className="editor-body" data-invalid={error !== null || undefined}>
        <div className="editor-gutter" aria-hidden="true">
          {Array.from({ length: lineCount }, (_, index) => (
            <span key={index} className={error?.span?.line === index + 1 ? "is-error" : undefined}>
              {index + 1}
            </span>
          ))}
        </div>
        <Textarea
          ref={textareaRef}
          className="editor-input"
          value={value}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          onChange={(event) => onChange(event.target.value)}
          onKeyDown={handleKeyDown}
          aria-label="HelixSQL query"
          aria-invalid={error !== null}
          aria-describedby={error ? "editor-error" : undefined}
          placeholder="QUERY NODES:User LIMIT 100"
        />
      </div>

      {error ? (
        <p className="editor-error" id="editor-error" role="alert">
          <Button variant="link" onClick={jumpToError} disabled={!error.span} title="Select the offending text">
            {error.span ? `L${error.span.line}:${error.span.column}` : "error"}
          </Button>
          <span className="editor-error-message">{error.message}</span>
          {error.hint ? <span className="editor-error-hint">{error.hint}</span> : null}
        </p>
      ) : null}
    </section>
  );
}

function CompileIndicator({ status }: { status: CompileStatus }) {
  switch (status.kind) {
    case "empty":
      return <span className="compile-status">Empty</span>;
    case "checking":
      return <span className="compile-status">Checking…</span>;
    case "invalid":
      return (
        <span className="compile-status" data-kind="invalid">
          <CircleX aria-hidden="true" />
          Syntax error
        </span>
      );
    case "valid":
      return (
        <span className="compile-status" data-kind="valid" title={status.summary}>
          <CircleCheck aria-hidden="true" />
          <span className="compile-status-summary">{status.summary}</span>
        </span>
      );
  }
}
