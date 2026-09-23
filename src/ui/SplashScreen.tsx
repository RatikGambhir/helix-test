import { useCallback, useEffect, useRef, useState } from "react";

import { ArrowRight } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/kbd";
import { Progress } from "@/components/ui/progress";
import { BrandMark } from "./BrandMark";
import "./SplashScreen.css";

type SplashTokenKind =
  | "keyword"
  | "entity"
  | "label"
  | "property"
  | "direction"
  | "operator"
  | "literal"
  | "number"
  | "punctuation";

interface SplashToken {
  text: string;
  kind: SplashTokenKind;
}

interface SplashLine {
  tokens: SplashToken[];
}

interface Props {
  onComplete: () => void;
  minDuration?: number;
}

/** A valid HelixSQL graph query, tokenized for the animated editor preview. */
const HELIX_SQL_PREVIEW: SplashLine[] = [
  {
    tokens: [
      { text: "QUERY", kind: "keyword" },
      { text: " ", kind: "punctuation" },
      { text: "NODES", kind: "entity" },
      { text: ":", kind: "punctuation" },
      { text: "User", kind: "label" },
    ],
  },
  {
    tokens: [
      { text: "WHERE", kind: "keyword" },
      { text: " active ", kind: "property" },
      { text: "=", kind: "operator" },
      { text: " ", kind: "punctuation" },
      { text: "true", kind: "literal" },
    ],
  },
  {
    tokens: [
      { text: "TRAVERSE", kind: "keyword" },
      { text: " ", kind: "punctuation" },
      { text: "OUT", kind: "direction" },
      { text: " ", kind: "punctuation" },
      { text: "Follows", kind: "label" },
    ],
  },
  {
    tokens: [
      { text: "WITH", kind: "keyword" },
      { text: " name", kind: "property" },
      { text: ", ", kind: "punctuation" },
      { text: "email", kind: "property" },
    ],
  },
  {
    tokens: [
      { text: "ORDER BY", kind: "keyword" },
      { text: " name ", kind: "property" },
      { text: "ASC", kind: "keyword" },
    ],
  },
  {
    tokens: [
      { text: "LIMIT", kind: "keyword" },
      { text: " ", kind: "punctuation" },
      { text: "150", kind: "number" },
      { text: "  ", kind: "punctuation" },
      { text: "EDGE LIMIT", kind: "keyword" },
      { text: " ", kind: "punctuation" },
      { text: "600", kind: "number" },
    ],
  },
];

export const SPLASH_QUERY = HELIX_SQL_PREVIEW
  .map((line) => line.tokens.map((token) => token.text).join(""))
  .join("\n");

const EXIT_DURATION_MS = 560;

export function SplashScreen({ onComplete, minDuration = 2_000 }: Props) {
  const [visibleLines, setVisibleLines] = useState(0);
  const [progress, setProgress] = useState(0);
  const [ready, setReady] = useState(false);
  const [exiting, setExiting] = useState(false);
  const completed = useRef(false);
  const continueButton = useRef<HTMLButtonElement>(null);

  const exit = useCallback(() => {
    if (!ready || completed.current) return;
    completed.current = true;
    setExiting(true);
    window.setTimeout(onComplete, EXIT_DURATION_MS);
  }, [onComplete, ready]);

  useEffect(() => {
    const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const duration = reduceMotion ? 180 : Math.max(minDuration, 900);
    const lineDelay = Math.max(80, Math.floor(duration / (HELIX_SQL_PREVIEW.length + 2)));

    if (reduceMotion) setVisibleLines(HELIX_SQL_PREVIEW.length);
    let lineTimer: number | undefined;
    if (!reduceMotion) {
      lineTimer = window.setInterval(() => {
        setVisibleLines((current) => {
          if (current >= HELIX_SQL_PREVIEW.length) {
            window.clearInterval(lineTimer);
            return current;
          }
          return current + 1;
        });
      }, lineDelay);
    }

    const progressTimer = window.setInterval(() => {
      setProgress((current) => Math.min(94, current + Math.max(1, (94 - current) * 0.09)));
    }, reduceMotion ? 40 : 70);

    const readyTimer = window.setTimeout(() => {
      window.clearInterval(progressTimer);
      window.clearInterval(lineTimer);
      setVisibleLines(HELIX_SQL_PREVIEW.length);
      setProgress(100);
      setReady(true);
    }, duration);

    return () => {
      window.clearInterval(lineTimer);
      window.clearInterval(progressTimer);
      window.clearTimeout(readyTimer);
    };
  }, [minDuration]);

  useEffect(() => {
    if (!ready) return;
    continueButton.current?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Enter") exit();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [exit, ready]);

  const loadingTask =
    progress < 34
      ? "Preparing HelixSQL"
      : progress < 70
        ? "Restoring workspace"
        : "Finalizing visualizer";

  return (
    <section
      className={exiting ? "splash splash-exit" : "splash"}
      aria-label="Helix Visualizer welcome"
    >
      <div className="splash-grid" aria-hidden="true" />

      <div className="splash-layout">
        <header className="splash-intro splash-reveal" style={{ "--reveal-delay": "60ms" } as React.CSSProperties}>
          <BrandMark size={44} />
          <h1 className="splash-title">
            Helix
            <br />
            Visualizer
          </h1>
          <p className="splash-lede">
            Query a HelixDB instance in HelixSQL, then read the answer as rows, counts, or the graph itself.
          </p>

          <div className="splash-progress">
            <Progress
              className="splash-progress-track"
              aria-label="Preparing Helix Visualizer"
              value={Math.round(progress)}
            />
            <div className="splash-progress-row">
              {ready ? (
                <>
                  <Button ref={continueButton} variant="default" size="lg" className="splash-enter" onClick={exit}>
                    Continue
                    <ArrowRight />
                  </Button>
                  <span className="splash-hint" aria-hidden="true">or press <Kbd>↵</Kbd></span>
                </>
              ) : (
                <span className="splash-status" role="status">
                  <span className="splash-status-pct" aria-hidden="true">{String(Math.round(progress)).padStart(2, "0")}%</span>
                  {loadingTask}
                </span>
              )}
            </div>
          </div>
        </header>

        <figure className="splash-preview splash-reveal" style={{ "--reveal-delay": "200ms" } as React.CSSProperties}>
          <figcaption className="splash-preview-head">
            <span>welcome.hsql</span>
            <span>Read-only</span>
          </figcaption>
          <div className="splash-editor" aria-label="Example HelixSQL graph query">
            {HELIX_SQL_PREVIEW.map((line, index) => (
              <div
                className={index < visibleLines ? "splash-code-line visible" : "splash-code-line"}
                key={index}
              >
                <span className="splash-line-number" aria-hidden="true">{index + 1}</span>
                <code>
                  {line.tokens.map((token, tokenIndex) => (
                    <span className={`tok-${token.kind}`} key={`${token.kind}-${tokenIndex}`}>
                      {token.text}
                    </span>
                  ))}
                  {index === visibleLines - 1 && visibleLines < HELIX_SQL_PREVIEW.length ? (
                    <span className="splash-cursor" aria-hidden="true" />
                  ) : null}
                </code>
              </div>
            ))}
          </div>
          <div className="splash-preview-foot" aria-hidden="true">
            <span>{visibleLines >= HELIX_SQL_PREVIEW.length ? "Valid · graph of User nodes" : "Typing…"}</span>
            <span>{visibleLines} / {HELIX_SQL_PREVIEW.length} lines</span>
          </div>
        </figure>
      </div>
    </section>
  );
}
