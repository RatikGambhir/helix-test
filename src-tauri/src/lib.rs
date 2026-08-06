//! Backend for the Helix Visualizer desktop app.
//!
//! The webview never talks to HelixDB directly: it hands a serialized query AST
//! to [`run_query`], which posts it to `POST {url}/v1/query`. Going through Rust
//! keeps the app free of webview CORS rules, lets it reach plain-HTTP local
//! instances from an `https`-origin webview, and keeps the API key in the
//! backend's config file instead of webview storage.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

/// Path segments appended to the configured base URL. Kept as segments because
/// `PathSegmentsMut::push` percent-encodes anything it is given, so pushing
/// `"v1/query"` in one go would produce `v1%2Fquery`.
const QUERY_PATH_SEGMENTS: [&str; 2] = ["v1", "query"];
const CONFIG_FILE: &str = "connection.json";
const DEFAULT_URL: &str = "http://localhost:6969";
const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// Everything needed to reach one Helix instance.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    /// Sends `x-helix-require-writer: true`, forcing the request onto a writer
    /// node. Reads are happy on replicas, so this stays off by default.
    #[serde(default)]
    pub writer_only: bool,
}

fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT_MS
}

impl Default for Connection {
    fn default() -> Self {
        Self {
            url: DEFAULT_URL.to_string(),
            api_key: None,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            writer_only: false,
        }
    }
}

/// The connection as the frontend is allowed to see it — the key itself never
/// crosses back over the IPC boundary, only whether one is set.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionView {
    pub url: String,
    pub has_api_key: bool,
    pub timeout_ms: u64,
    pub writer_only: bool,
}

impl From<&Connection> for ConnectionView {
    fn from(c: &Connection) -> Self {
        Self {
            url: c.url.clone(),
            has_api_key: c.api_key.as_deref().is_some_and(|k| !k.is_empty()),
            timeout_ms: c.timeout_ms,
            writer_only: c.writer_only,
        }
    }
}

/// What the frontend sends when saving settings. `api_key` is tri-state:
/// `None` keeps the stored key, `Some("")` clears it, `Some(k)` replaces it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionUpdate {
    pub url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub writer_only: bool,
}

/// One executed query, successful or not.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResponse {
    pub status: u16,
    /// Raw response text. Parsed on the frontend so that i64 values outside
    /// JavaScript's safe range survive as `bigint` instead of losing precision.
    pub body: String,
    pub duration_ms: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("invalid instance URL {url:?}: {reason}")]
    InvalidUrl { url: String, reason: String },
    #[error("could not reach {url}: {reason}")]
    Unreachable { url: String, reason: String },
    #[error("request timed out after {0}ms")]
    Timeout(u64),
    #[error("{0}")]
    Io(String),
}

// Tauri requires command errors to be serializable; the message is what the
// frontend shows, so the whole error collapses to its Display form.
impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

type Result<T> = std::result::Result<T, AppError>;

pub struct AppState {
    connection: Mutex<Connection>,
    config_path: Mutex<Option<PathBuf>>,
    http: reqwest::Client,
}

impl AppState {
    fn new() -> Self {
        Self {
            connection: Mutex::new(Connection::default()),
            config_path: Mutex::new(None),
            http: reqwest::Client::new(),
        }
    }

    fn snapshot(&self) -> Connection {
        self.connection
            .lock()
            .expect("connection mutex poisoned")
            .clone()
    }
}

/// Joins the query path onto the configured base URL, tolerating a trailing
/// slash or an inherited path prefix (`https://gateway/helix` -> `.../helix/v1/query`).
fn query_endpoint(base: &str) -> Result<reqwest::Url> {
    let trimmed = base.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidUrl {
            url: base.to_string(),
            reason: "the URL is empty".into(),
        });
    }
    // A bare `localhost:6969` is what people type; assume plain HTTP for it.
    let normalized = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };

    let mut url = reqwest::Url::parse(&normalized).map_err(|e| AppError::InvalidUrl {
        url: base.to_string(),
        reason: e.to_string(),
    })?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AppError::InvalidUrl {
            url: base.to_string(),
            reason: format!(
                "unsupported scheme {:?}, expected http or https",
                url.scheme()
            ),
        });
    }
    // Pasting the full endpoint is an easy mistake to make, since that is the
    // path the docs and error messages name; appending to it again would give
    // `/v1/query/v1/query` and an opaque 404.
    let already_endpoint = url
        .path_segments()
        .map(|s| {
            let kept: Vec<&str> = s.filter(|part| !part.is_empty()).collect();
            kept.ends_with(&QUERY_PATH_SEGMENTS)
        })
        .unwrap_or(false);

    if !already_endpoint {
        let mut segments = url.path_segments_mut().map_err(|_| AppError::InvalidUrl {
            url: base.to_string(),
            reason: "the URL cannot have a path".into(),
        })?;
        // `pop_if_empty` stops `http://host/` from producing `//v1/query`.
        segments.pop_if_empty().extend(QUERY_PATH_SEGMENTS);
    }
    Ok(url)
}

/// Resolves the tri-state `api_key` of a [`ConnectionUpdate`] against the key
/// already on file: `None` keeps it, `Some("")` clears it, `Some(k)` replaces
/// it. Shared by `set_connection` and `test_connection` so a probe and the save
/// that follows it can never disagree about which key is in play.
fn resolve_api_key(update: Option<String>, stored: Option<String>) -> Option<String> {
    match update {
        None => stored.filter(|k| !k.is_empty()),
        Some(key) if key.is_empty() => None,
        Some(key) => Some(key),
    }
}

fn describe_request_error(err: &reqwest::Error) -> String {
    // reqwest nests the interesting cause (connection refused, DNS failure),
    // and the outer Display is usually just "error sending request".
    let mut source: Option<&(dyn std::error::Error + 'static)> = std::error::Error::source(err);
    let mut deepest = err.to_string();
    while let Some(cause) = source {
        deepest = cause.to_string();
        source = cause.source();
    }
    deepest
}

async fn post_query(state: &AppState, conn: &Connection, body: String) -> Result<QueryResponse> {
    let endpoint = query_endpoint(&conn.url)?;
    let timeout_ms = conn.timeout_ms.clamp(1_000, 600_000);

    let mut request = state
        .http
        .post(endpoint)
        .timeout(Duration::from_millis(timeout_ms))
        .header("content-type", "application/json")
        .body(body);
    if let Some(key) = conn.api_key.as_deref().filter(|k| !k.is_empty()) {
        request = request.header("Authorization", format!("Bearer {key}"));
    }
    if conn.writer_only {
        request = request.header("x-helix-require-writer", "true");
    }

    let started = Instant::now();
    let response = request.send().await.map_err(|e| {
        if e.is_timeout() {
            AppError::Timeout(timeout_ms)
        } else {
            AppError::Unreachable {
                url: conn.url.clone(),
                reason: describe_request_error(&e),
            }
        }
    })?;

    let status = response.status().as_u16();
    let text = response.text().await.map_err(|e| AppError::Unreachable {
        url: conn.url.clone(),
        reason: describe_request_error(&e),
    })?;

    Ok(QueryResponse {
        status,
        body: text,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

/// Sends one already-serialized query AST to the configured instance.
///
/// The AST is built in the frontend by `@helix-db/helix-db`, so this stays a
/// transport: it does not inspect or rewrite the query.
#[tauri::command]
async fn run_query(state: State<'_, AppState>, query_json: String) -> Result<QueryResponse> {
    let conn = state.snapshot();
    post_query(&state, &conn, query_json).await
}

/// Runs a trivial query against a candidate connection without saving it, so
/// the settings panel can verify a URL before committing to it.
#[tauri::command]
async fn test_connection(
    state: State<'_, AppState>,
    connection: ConnectionUpdate,
    query_json: String,
) -> Result<QueryResponse> {
    let conn = Connection {
        url: connection.url,
        // The settings form only sends a key when the user typed one, so a
        // probe of an otherwise-unchanged connection has to reuse the saved
        // key — otherwise editing just the URL would fail against any instance
        // that requires auth, and the key is write-only in the UI.
        api_key: resolve_api_key(connection.api_key, state.snapshot().api_key),
        timeout_ms: connection.timeout_ms,
        writer_only: connection.writer_only,
    };
    post_query(&state, &conn, query_json).await
}

#[tauri::command]
fn get_connection(state: State<'_, AppState>) -> ConnectionView {
    ConnectionView::from(&state.snapshot())
}

#[tauri::command]
fn set_connection(state: State<'_, AppState>, update: ConnectionUpdate) -> Result<ConnectionView> {
    // Validate before storing so a typo can't leave the app unusable.
    query_endpoint(&update.url)?;

    let stored = {
        let mut guard = state.connection.lock().expect("connection mutex poisoned");
        guard.url = update.url;
        guard.timeout_ms = update.timeout_ms;
        guard.writer_only = update.writer_only;
        guard.api_key = resolve_api_key(update.api_key, guard.api_key.take());
        guard.clone()
    };

    save_connection(state.inner(), &stored)?;
    Ok(ConnectionView::from(&stored))
}

fn save_connection(state: &AppState, conn: &Connection) -> Result<()> {
    let path = state
        .config_path
        .lock()
        .expect("config path mutex poisoned")
        .clone();
    let Some(path) = path else { return Ok(()) };

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AppError::Io(e.to_string()))?;
    }
    let json = serde_json::to_string_pretty(conn).map_err(|e| AppError::Io(e.to_string()))?;
    std::fs::write(&path, json).map_err(|e| AppError::Io(e.to_string()))?;
    restrict_to_owner(&path)
}

/// The config file holds the API key in plain text, so on Unix it is narrowed
/// to `0600` after writing. Best-effort: a filesystem that cannot represent
/// the mode is not a reason to fail the save.
#[cfg(unix)]
fn restrict_to_owner(path: &PathBuf) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    Ok(())
}

#[cfg(not(unix))]
fn restrict_to_owner(_path: &PathBuf) -> Result<()> {
    Ok(())
}

fn load_connection(path: &PathBuf) -> Option<Connection> {
    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::new())
        .setup(|app| {
            let path = app
                .path()
                .app_config_dir()
                .ok()
                .map(|d| d.join(CONFIG_FILE));
            if let Some(path) = path {
                let state = app.state::<AppState>();
                if let Some(saved) = load_connection(&path) {
                    *state.connection.lock().expect("connection mutex poisoned") = saved;
                }
                *state
                    .config_path
                    .lock()
                    .expect("config path mutex poisoned") = Some(path);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            run_query,
            test_connection,
            get_connection,
            set_connection
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Helix Visualizer");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(base: &str) -> String {
        query_endpoint(base).unwrap().to_string()
    }

    #[test]
    fn appends_the_query_path_to_a_bare_origin() {
        assert_eq!(
            endpoint("http://localhost:6969"),
            "http://localhost:6969/v1/query"
        );
    }

    #[test]
    fn a_trailing_slash_does_not_double_up() {
        assert_eq!(
            endpoint("http://localhost:6969/"),
            "http://localhost:6969/v1/query"
        );
    }

    #[test]
    fn a_gateway_path_prefix_is_preserved() {
        assert_eq!(
            endpoint("https://gateway.example.com/helix"),
            "https://gateway.example.com/helix/v1/query"
        );
    }

    #[test]
    fn a_scheme_less_host_is_assumed_to_be_http() {
        assert_eq!(endpoint("localhost:6969"), "http://localhost:6969/v1/query");
    }

    #[test]
    fn a_pasted_full_endpoint_is_not_doubled_up() {
        assert_eq!(
            endpoint("http://localhost:6969/v1/query"),
            "http://localhost:6969/v1/query"
        );
        assert_eq!(
            endpoint("https://gateway.example.com/helix/v1/query/"),
            "https://gateway.example.com/helix/v1/query/"
        );
        // A path that merely contains the segments elsewhere still gets them.
        assert_eq!(
            endpoint("https://gateway.example.com/v1/query/helix"),
            "https://gateway.example.com/v1/query/helix/v1/query"
        );
    }

    #[test]
    fn an_omitted_api_key_keeps_the_saved_one() {
        // The settings form omits the key unless the user typed one, so a probe
        // of an unchanged connection has to reuse what is already on file.
        assert_eq!(
            resolve_api_key(None, Some("hx_saved".into())),
            Some("hx_saved".into())
        );
        // An explicit empty string is the "remove the saved key" signal.
        assert_eq!(
            resolve_api_key(Some(String::new()), Some("hx_saved".into())),
            None
        );
        assert_eq!(
            resolve_api_key(Some("hx_new".into()), Some("hx_saved".into())),
            Some("hx_new".into())
        );
        assert_eq!(resolve_api_key(None, None), None);
        // A stored empty string is treated as no key at all.
        assert_eq!(resolve_api_key(None, Some(String::new())), None);
    }

    #[test]
    fn non_http_schemes_are_rejected() {
        assert!(query_endpoint("ftp://localhost:6969").is_err());
        assert!(query_endpoint("   ").is_err());
    }

    #[test]
    fn the_connection_view_reports_key_presence_without_leaking_it() {
        let with_key = Connection {
            api_key: Some("hx_secret".into()),
            ..Connection::default()
        };
        let view = ConnectionView::from(&with_key);
        assert!(view.has_api_key);

        let serialized = serde_json::to_string(&view).unwrap();
        assert!(!serialized.contains("hx_secret"));

        let empty_key = Connection {
            api_key: Some(String::new()),
            ..Connection::default()
        };
        assert!(!ConnectionView::from(&empty_key).has_api_key);
    }
}
