import { useEffect, useMemo, useRef } from "react";

import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/kbd";
import { Textarea } from "@/components/ui/textarea";
import { HqlError } from "../hql/ast";

interface Props {
  value: string;
  onChange: (value: string) => void;
  onRun: () => void;
  running: boolean;
  /** Parse/compile failure for the current text, if any. */
  error: HqlError | null;
}

/**
 * A textarea with a gutter and an inline error marker.
 *
 * Deliberately not a full code editor: the language is small, and a plain
 * textarea keeps selection, undo and IME behaviour native.
 */
export function QueryEditor({ value, onChange, onRun, running, error }: Props) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const lineCount = useMemo(() => Math.max(value.split("\n").length, 1), [value]);

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
    if (event.key === "Tab") {
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
    <section className="query-editor">
      <header>
        <h2>Query</h2>
        <div className="editor-actions">
          <Kbd>{navigator.platform.includes("Mac") ? "⌘" : "Ctrl"}</Kbd>
          <Kbd>↵</Kbd>
          <Button variant="default" className="primary" onClick={onRun} disabled={running}>
            {running ? "Running…" : "Run"}
          </Button>
        </div>
      </header>

      <div className="editor-body">
        <div className="gutter" aria-hidden="true">
          {Array.from({ length: lineCount }, (_, index) => (
            <span key={index} className={error?.span?.line === index + 1 ? "gutter-error" : undefined}>
              {index + 1}
            </span>
          ))}
        </div>
        <Textarea
          ref={textareaRef}
          value={value}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          onChange={(event) => onChange(event.target.value)}
          onKeyDown={handleKeyDown}
          aria-label="HelixSQL query"
          aria-invalid={error !== null}
        />
      </div>

      {error && (
        <p className="editor-error" role="alert">
          <Button variant="link" onClick={jumpToError}>
            {error.span ? `line ${error.span.line}:${error.span.column}` : "error"}
          </Button>
          <span>{error.message}</span>
          {error.hint && <em>{error.hint}</em>}
        </p>
      )}
    </section>
  );
}
