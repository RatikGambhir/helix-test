/**
 * Transport to the HelixDB instance.
 *
 * In the packaged app every request goes through the Rust backend
 * (`src-tauri/src/lib.rs`), which owns the connection settings and the API key.
 * When the same frontend is opened in a plain browser — `npm run dev` without
 * Tauri — it falls back to Vite's `/helix` proxy so the UI stays workable
 * during development against a local instance or the bundled mock server.
 */
import { invoke } from "@tauri-apps/api/core";
import { parseJson } from "@helix-db/helix-db";

export interface ConnectionView {
  url: string;
  hasApiKey: boolean;
  timeoutMs: number;
  writerOnly: boolean;
}

export interface ConnectionUpdate {
  url: string;
  /** `undefined` keeps the saved key, `""` clears it, anything else replaces it. */
  apiKey?: string;
  timeoutMs: number;
  writerOnly: boolean;
}

export interface QueryResponse {
  status: number;
  body: string;
  durationMs: number;
}

/** A failure that already carries a message worth showing the user verbatim. */
export class TransportError extends Error {
  readonly detail: string | null;

  constructor(message: string, detail: string | null = null) {
    super(message);
    this.name = "TransportError";
    this.detail = detail;
  }
}

/** True when running inside the Tauri webview rather than a browser tab. */
export function isDesktop(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

const BROWSER_FALLBACK_URL = "/helix/v2/query";
const BROWSER_CONNECTION_KEY = "helix-visualizer.browser-connection";

function readBrowserConnection(): ConnectionView {
  const fallback: ConnectionView = {
    url: "http://localhost:6969 (via the Vite dev proxy)",
    hasApiKey: false,
    timeoutMs: 30_000,
    writerOnly: false,
  };
  try {
    const saved = window.localStorage.getItem(BROWSER_CONNECTION_KEY);
    return saved ? { ...fallback, ...JSON.parse(saved) } : fallback;
  } catch {
    return fallback;
  }
}

async function browserQuery(queryJson: string): Promise<QueryResponse> {
  const started = performance.now();
  let response: Response;
  try {
    response = await fetch(BROWSER_FALLBACK_URL, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: queryJson,
    });
  } catch (error) {
    throw new TransportError(
      "could not reach the instance through the dev proxy",
      error instanceof Error ? error.message : String(error),
    );
  }
  return {
    status: response.status,
    body: await response.text(),
    durationMs: Math.round(performance.now() - started),
  };
}

function asTransportError(error: unknown): TransportError {
  // Errors thrown by a Tauri command arrive as the serialized string.
  if (typeof error === "string") return new TransportError(error);
  if (error instanceof Error) return new TransportError(error.message);
  return new TransportError(String(error));
}

export async function getConnection(): Promise<ConnectionView> {
  if (!isDesktop()) return readBrowserConnection();
  try {
    return await invoke<ConnectionView>("get_connection");
  } catch (error) {
    throw asTransportError(error);
  }
}

export async function setConnection(update: ConnectionUpdate): Promise<ConnectionView> {
  if (!isDesktop()) {
    const view: ConnectionView = {
      url: update.url,
      hasApiKey: (update.apiKey ?? "").length > 0,
      timeoutMs: update.timeoutMs,
      writerOnly: update.writerOnly,
    };
    window.localStorage.setItem(BROWSER_CONNECTION_KEY, JSON.stringify(view));
    return view;
  }
  try {
    return await invoke<ConnectionView>("set_connection", { update });
  } catch (error) {
    throw asTransportError(error);
  }
}

/** Runs a compiled query against the saved connection. */
export async function runQuery(queryJson: string): Promise<QueryResponse> {
  if (!isDesktop()) return browserQuery(queryJson);
  try {
    return await invoke<QueryResponse>("run_query", { queryJson });
  } catch (error) {
    throw asTransportError(error);
  }
}

/** Runs a query against a candidate connection without saving it. */
export async function testConnection(
  connection: ConnectionUpdate,
  queryJson: string,
): Promise<QueryResponse> {
  if (!isDesktop()) return browserQuery(queryJson);
  try {
    return await invoke<QueryResponse>("test_connection", { connection, queryJson });
  } catch (error) {
    throw asTransportError(error);
  }
}

/**
 * Parses a successful response body.
 *
 * `parseJson` comes from the HelixDB SDK and keeps i64 values outside the
 * JavaScript safe range as `bigint`, which matters because entity ids are i64
 * and are used as identity throughout the app.
 */
export function parseResponseBody(response: QueryResponse): unknown {
  if (response.status !== 200) {
    throw new TransportError(
      `HelixDB returned HTTP ${response.status}`,
      response.body.trim() || null,
    );
  }
  try {
    return parseJson(response.body);
  } catch (error) {
    throw new TransportError(
      "the instance returned a body that is not JSON",
      error instanceof Error ? error.message : String(error),
    );
  }
}
