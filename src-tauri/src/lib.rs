//! Backend for the Helix Visualizer desktop app.
//!
//! The webview never parses HelixSQL or talks to HelixDB directly. It hands HQL
//! source to Rust, which lexes, parses, validates, compiles, executes, and
//! decodes the response into UI-facing view models. This boundary also keeps
//! the app free of webview CORS rules and the API key out of webview storage.

mod hql;
mod results;
mod schema;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

use hql::{compile_source, CompiledQuery, CompiledQueryView, HqlError, Span};
use results::{decode_response, decode_rows_for_schema, QueryResult, ResultError};
use schema::{infer_schema_fields, schema_query_for, Schema, SchemaLabel};

/// Path segments appended to the configured base URL. Kept as segments because
/// `PathSegmentsMut::push` percent-encodes anything it is given, so pushing
/// `"v1/query"` in one go would produce `v1%2Fquery`.
const QUERY_PATH_SEGMENTS: [&str; 2] = ["v1", "query"];
const CONFIG_FILE: &str = "connection.json";
const DEFAULT_URL: &str = "http://localhost:6969";
const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const PROBE_QUERY: &str = "SELECT COUNT(*) FROM NODES LIMIT 1";
const SCHEMA_QUERY: &str = "SHOW LABELS SAMPLE 5000";
const SCHEMA_SAMPLE: u64 = 5_000;
const SCHEMA_FIELD_SAMPLE: u64 = 25;
const SCHEMA_FIELD_CONCURRENCY: usize = 6;

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
/// `None` keeps the stored key for the same endpoint, `Some("")` clears it,
/// and `Some(k)` replaces it.
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

/// Raw response retained only inside the Rust process.
#[derive(Debug)]
struct QueryResponse {
    status: u16,
    body: String,
    duration_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryExecution {
    pub result: QueryResult,
    pub duration_ms: u64,
    pub compiled: CompiledQueryView,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResponse {
    pub duration_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaResponse {
    pub schema: Schema,
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
    #[error("{0}")]
    Hql(#[from] HqlError),
    #[error("{0}")]
    Result(#[from] ResultError),
    #[error("HelixDB returned HTTP {status}")]
    HttpStatus { status: u16, detail: Option<String> },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorPayload<'a> {
    kind: &'static str,
    message: String,
    detail: Option<&'a str>,
    span: Option<&'a Span>,
    hint: Option<&'a str>,
}

// Tauri command errors cross IPC as a stable structured contract so the editor
// can retain source spans and the UI can distinguish transport failures.
impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        let (kind, detail, span, hint) = match self {
            Self::InvalidUrl { reason, .. } => ("invalidUrl", Some(reason.as_str()), None, None),
            Self::Unreachable { reason, .. } => ("transport", Some(reason.as_str()), None, None),
            Self::Timeout(_) => ("transport", None, None, None),
            Self::Io(detail) => ("io", Some(detail.as_str()), None, None),
            Self::Hql(error) => ("hql", None, error.span.as_ref(), error.hint.as_deref()),
            Self::Result(error) => ("result", error.detail.as_deref(), None, None),
            Self::HttpStatus { detail, .. } => ("http", detail.as_deref(), None, None),
        };
        ErrorPayload {
            kind,
            message: self.to_string(),
            detail,
            span,
            hint,
        }
        .serialize(serializer)
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
/// already on file. An omitted key is reused only for the same effective query
/// endpoint; changing hosts or gateway paths must never forward an old secret.
fn resolve_api_key(
    update: Option<String>,
    stored: Option<String>,
    same_endpoint: bool,
) -> Option<String> {
    match update {
        None if same_endpoint => stored.filter(|k| !k.is_empty()),
        None => None,
        Some(key) if key.is_empty() => None,
        Some(key) => Some(key),
    }
}

fn is_same_endpoint(left: &str, right: &str) -> bool {
    match (query_endpoint(left), query_endpoint(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
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

async fn post_query(
    http: &reqwest::Client,
    conn: &Connection,
    body: String,
) -> Result<QueryResponse> {
    let endpoint = query_endpoint(&conn.url)?;
    let timeout_ms = conn.timeout_ms.clamp(1_000, 600_000);

    let mut request = http
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

fn require_success(response: QueryResponse) -> Result<QueryResponse> {
    if response.status == 200 {
        return Ok(response);
    }
    Err(AppError::HttpStatus {
        status: response.status,
        detail: (!response.body.trim().is_empty()).then(|| response.body.trim().to_owned()),
    })
}

async fn execute_compiled(
    http: &reqwest::Client,
    connection: &Connection,
    compiled: &CompiledQuery,
) -> Result<(QueryResult, u64)> {
    let response =
        require_success(post_query(http, connection, compiled.transport_json.clone()).await?)?;
    let result = decode_response(&response.body, &compiled.shape)?;
    Ok((result, response.duration_ms))
}

/// Compiles HQL for live editor feedback and the wire-format inspector.
#[tauri::command]
fn compile_query(source: String) -> Result<CompiledQueryView> {
    let compiled = compile_source(&source)?;
    Ok(CompiledQueryView::from(&compiled))
}

/// Owns the full source-to-result pipeline for one user query.
#[tauri::command]
async fn run_query(state: State<'_, AppState>, source: String) -> Result<QueryExecution> {
    let compiled = compile_source(&source)?;
    let conn = state.snapshot();
    let (result, duration_ms) = execute_compiled(&state.http, &conn, &compiled).await?;
    Ok(QueryExecution {
        result,
        duration_ms,
        compiled: CompiledQueryView::from(&compiled),
    })
}

/// Runs a trivial query against a candidate connection without saving it, so
/// the settings panel can verify a URL before committing to it.
#[tauri::command]
async fn test_connection(
    state: State<'_, AppState>,
    connection: ConnectionUpdate,
) -> Result<ProbeResponse> {
    let stored = state.snapshot();
    let same_endpoint = is_same_endpoint(&connection.url, &stored.url);
    let conn = Connection {
        url: connection.url,
        api_key: resolve_api_key(connection.api_key, stored.api_key, same_endpoint),
        timeout_ms: connection.timeout_ms,
        writer_only: connection.writer_only,
    };
    let compiled = compile_source(PROBE_QUERY)?;
    let (_, duration_ms) = execute_compiled(&state.http, &conn, &compiled).await?;
    Ok(ProbeResponse { duration_ms })
}

/// Discovers labels and samples their fields entirely in the backend. Per-label
/// sample failures are retained on that label so one unusual type does not hide
/// the rest of the schema.
#[tauri::command]
async fn load_schema(state: State<'_, AppState>) -> Result<SchemaResponse> {
    let conn = state.snapshot();
    let labels_query = compile_source(SCHEMA_QUERY)?;
    let (labels, duration_ms) = execute_compiled(&state.http, &conn, &labels_query).await?;
    let QueryResult::Labels { nodes, edges } = labels else {
        return Err(AppError::Result(ResultError {
            message: "unexpected schema response".into(),
            detail: None,
        }));
    };

    let node_labels = sample_schema_labels(
        state.http.clone(),
        conn.clone(),
        "nodes",
        nodes.unwrap_or_default(),
    )
    .await?;
    let edge_labels = sample_schema_labels(
        state.http.clone(),
        conn.clone(),
        "edges",
        edges.unwrap_or_default(),
    )
    .await?;

    Ok(SchemaResponse {
        schema: Schema {
            node_labels,
            edge_labels,
            sample: SCHEMA_SAMPLE,
        },
        duration_ms,
    })
}

async fn sample_schema_labels(
    http: reqwest::Client,
    connection: Connection,
    entity: &'static str,
    labels: Vec<results::LabelCount>,
) -> Result<Vec<SchemaLabel>> {
    let semaphore = Arc::new(tokio::sync::Semaphore::new(SCHEMA_FIELD_CONCURRENCY));
    let mut tasks = tokio::task::JoinSet::new();
    let label_count = labels.len();
    for (index, value) in labels.into_iter().enumerate() {
        let http = http.clone();
        let connection = connection.clone();
        let semaphore = Arc::clone(&semaphore);
        tasks.spawn(async move {
            let _permit = semaphore
                .acquire_owned()
                .await
                .expect("schema semaphore remains open");
            let label = SchemaLabel::new(value, entity);
            (
                index,
                sample_schema_label(&http, &connection, entity, label).await,
            )
        });
    }

    let mut sampled = vec![None; label_count];
    while let Some(completed) = tasks.join_next().await {
        let (index, label) = completed.map_err(|error| AppError::Io(error.to_string()))?;
        sampled[index] = Some(label);
    }
    Ok(sampled.into_iter().flatten().collect())
}

async fn sample_schema_label(
    http: &reqwest::Client,
    connection: &Connection,
    entity: &str,
    mut label: SchemaLabel,
) -> SchemaLabel {
    let source = schema_query_for(entity, &label.label, SCHEMA_FIELD_SAMPLE);
    let sampled = async {
        let compiled = compile_source(&source)?;
        let response =
            require_success(post_query(http, connection, compiled.transport_json.clone()).await?)?;
        decode_rows_for_schema(&response.body, &compiled.shape).map_err(AppError::from)
    }
    .await;
    match sampled {
        Ok(rows) => {
            label.field_sample = rows.len();
            label.fields = infer_schema_fields(&rows);
        }
        Err(error) => label.field_error = Some(error.to_string()),
    }
    label
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
        let same_endpoint = is_same_endpoint(&update.url, &guard.url);
        let api_key = resolve_api_key(update.api_key, guard.api_key.take(), same_endpoint);
        guard.url = update.url;
        guard.timeout_ms = update.timeout_ms;
        guard.writer_only = update.writer_only;
        guard.api_key = api_key;
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
            compile_query,
            run_query,
            test_connection,
            load_schema,
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
            resolve_api_key(None, Some("hx_saved".into()), true),
            Some("hx_saved".into())
        );
        assert_eq!(resolve_api_key(None, Some("hx_saved".into()), false), None);
        // An explicit empty string is the "remove the saved key" signal.
        assert_eq!(
            resolve_api_key(Some(String::new()), Some("hx_saved".into()), true),
            None
        );
        assert_eq!(
            resolve_api_key(Some("hx_new".into()), Some("hx_saved".into()), false),
            Some("hx_new".into())
        );
        assert_eq!(resolve_api_key(None, None, true), None);
        // A stored empty string is treated as no key at all.
        assert_eq!(resolve_api_key(None, Some(String::new()), true), None);
    }

    #[test]
    fn saved_keys_are_reused_only_for_the_same_effective_endpoint() {
        assert!(is_same_endpoint(
            "https://helix.example.com",
            "https://helix.example.com/"
        ));
        assert!(is_same_endpoint(
            "https://helix.example.com/v1/query",
            "https://helix.example.com"
        ));
        assert!(!is_same_endpoint(
            "https://helix.example.com",
            "https://other.example.com"
        ));
        assert!(!is_same_endpoint(
            "https://gateway.example.com/team-a",
            "https://gateway.example.com/team-b"
        ));
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
