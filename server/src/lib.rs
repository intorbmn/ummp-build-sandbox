//! Local, authenticated REST adapter over `AppServices`.
//! All synchronous application work runs on Tokio's blocking pool.

pub mod mcp;

use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Path, State},
    http::{header::AUTHORIZATION, Request, StatusCode},
    middleware::{self, Next},
    response::{
        sse::{Event as SseEvent, KeepAlive},
        IntoResponse, Response, Sse,
    },
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashSet, VecDeque},
    convert::Infallible,
    error::Error,
    net::{IpAddr, SocketAddr},
    path::{Path as FsPath, PathBuf},
    sync::{Arc, Mutex},
};
use subtle::ConstantTimeEq;
use tokio::{net::TcpListener, sync::broadcast};
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use ummp_core::{
    app::{
        helpers::NoIndex, views::JobView, AppError, AppServices, BuildJobRequest, PackRequest,
        PlanRequest, PortRequest,
    },
    events::{Event, EventHandler},
    filesystem::resolve_within,
    modpack::plan::ModIndex,
};
use uuid::Uuid;

const BODY_LIMIT: usize = 1024 * 1024;
const EVENT_BUFFER: usize = 1024;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub data_dir: PathBuf,
    pub allowed_roots: Vec<PathBuf>,
    pub listen: IpAddr,
    pub port: u16,
    pub allow_remote: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            data_dir: default_data_dir(),
            allowed_roots: Vec::new(),
            listen: IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            port: 7432,
            allow_remote: false,
        }
    }
}

fn default_data_dir() -> PathBuf {
    ummp_core::app::credentials::default_data_dir().unwrap_or_else(|_| {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".ummp")
    })
}

const fn validate_bind(config: &ServerConfig) -> Result<(), &'static str> {
    if !config.listen.is_loopback() && !config.allow_remote {
        Err("remote listening requires --i-understand-remote")
    } else {
        Ok(())
    }
}

fn log_server_started(address: SocketAddr, _token: &str) {
    tracing::info!(%address, "UMMP REST listening");
}

/// Starts the local API and prints its bearer token exactly once for the user to copy.
pub fn serve(services: Arc<AppServices>, config: ServerConfig) -> Result<(), Box<dyn Error>> {
    validate_bind(&config).map_err(|error| error.to_owned())?;
    std::fs::create_dir_all(&config.data_dir)?;
    let token = load_or_create_token(&config.data_dir)?;
    println!("UMMP REST bearer token: {token}");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let address = SocketAddr::new(config.listen, config.port);
        let listener = TcpListener::bind(address).await?;
        log_server_started(listener.local_addr()?, &token);
        axum::serve(listener, router(services, token, config.allowed_roots)).await?;
        Ok::<(), Box<dyn Error>>(())
    })?;
    Ok(())
}

fn load_or_create_token(data_dir: &FsPath) -> Result<String, Box<dyn Error>> {
    use std::io::Write;

    let path = data_dir.join("server-token");
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        if meta.file_type().is_symlink() || !meta.is_file() {
            return Err("server-token must be a regular file".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        let token = std::fs::read_to_string(path)?;
        let token = token.trim().to_owned();
        if token.len() < 32 {
            return Err("server-token is invalid; remove it and restart UMMP".into());
        }
        return Ok(token);
    }

    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(token.as_bytes())?;
    file.sync_all()?;
    Ok(token)
}

#[derive(Clone)]
struct ApiState {
    services: Arc<AppServices>,
    token: Arc<String>,
    roots: Arc<Vec<PathBuf>>,
    events: broadcast::Sender<Event>,
    history: Arc<Mutex<VecDeque<Event>>>,
}

struct EventForwarder {
    sender: broadcast::Sender<Event>,
    history: Arc<Mutex<VecDeque<Event>>>,
}

impl EventHandler for EventForwarder {
    fn handle(&self, event: &Event) {
        let mut history = self.history.lock().expect("event history lock");
        history.push_back(event.clone());
        while history.len() > 4096 {
            history.pop_front();
        }
        drop(history);
        let _ = self.sender.send(event.clone());
    }
}

/// Builds the router for embedding and integration tests.
pub fn router(services: Arc<AppServices>, token: String, allowed_roots: Vec<PathBuf>) -> Router {
    let (events, _) = broadcast::channel(EVENT_BUFFER);
    let history = Arc::new(Mutex::new(VecDeque::new()));
    services.subscribe(Arc::new(EventForwarder {
        sender: events.clone(),
        history: Arc::clone(&history),
    }));
    let state = ApiState {
        services,
        token: Arc::new(token),
        roots: Arc::new(canonical_roots(allowed_roots)),
        events,
        history,
    };
    let protected = Router::new()
        .route("/v1/analyze", post(analyze))
        .route("/v1/scan", post(scan))
        .route("/v1/plan", post(plan))
        .route("/v1/port", post(port))
        .route("/v1/migrate", post(migrate))
        .route("/v1/pack", post(pack))
        .route("/v1/build", post(build))
        .route("/v1/jobs/:id", get(job))
        .route("/v1/jobs/:id/events", get(job_events))
        .route("/v1/jobs/:id/cancel", post(cancel))
        .route("/v1/sessions/:project/rollback", post(rollback))
        .route("/v1/openapi.json", get(openapi))
        .route_layer(middleware::from_fn_with_state(state.clone(), authorize));
    Router::new()
        .route("/v1/health", get(health))
        .merge(protected)
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
        .with_state(state)
}

async fn authorize(State(state): State<ApiState>, request: Request<Body>, next: Next) -> Response {
    let supplied = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or("");
    let expected_hash = Sha256::digest(state.token.as_bytes());
    let supplied_hash = Sha256::digest(supplied.as_bytes());
    if bool::from(expected_hash.as_slice().ct_eq(supplied_hash.as_slice())) {
        next.run(request).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"unauthorized"})),
        )
            .into_response()
    }
}

#[derive(Debug)]
struct ApiError(StatusCode, String);

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self(StatusCode::BAD_REQUEST, message.into())
    }
    fn forbidden(message: impl Into<String>) -> Self {
        Self(StatusCode::FORBIDDEN, message.into())
    }
    fn internal() -> Self {
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal server error".into(),
        )
    }
}

impl From<AppError> for ApiError {
    fn from(error: AppError) -> Self {
        let status = match &error {
            AppError::Input(_) => StatusCode::BAD_REQUEST,
            AppError::Refused(_) => StatusCode::FORBIDDEN,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Cancelled => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self(status, error.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({"error": self.1}))).into_response()
    }
}

async fn blocking<T, F>(work: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| ApiError::internal())?
        .map_err(ApiError::from)
}

pub(crate) fn canonical_roots(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let roots = if roots.is_empty() {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
            .into_iter()
            .collect()
    } else {
        roots
    };
    roots
        .into_iter()
        .filter_map(|root| root.canonicalize().ok())
        .collect()
}

fn canonical_candidate(path: &FsPath) -> Result<PathBuf, ApiError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| ApiError::internal())?
            .join(path)
    };
    let mut cursor = absolute.as_path();
    let mut suffix = Vec::new();
    while !cursor.exists() {
        let name = cursor
            .file_name()
            .ok_or_else(|| ApiError::bad_request("invalid path"))?;
        suffix.push(name.to_os_string());
        cursor = cursor
            .parent()
            .ok_or_else(|| ApiError::bad_request("invalid path"))?;
    }
    let mut resolved = cursor
        .canonicalize()
        .map_err(|_| ApiError::bad_request("path cannot be resolved"))?;
    for component in suffix.iter().rev() {
        resolved = resolve_within(&resolved, FsPath::new(component))
            .map_err(|_| ApiError::bad_request("path escapes its root"))?;
    }
    Ok(resolved)
}

/// Shared canonical path policy used by REST and MCP adapters.
pub(crate) fn resolve_allowed_path(path: &FsPath, roots: &[PathBuf]) -> Result<PathBuf, String> {
    let resolved = canonical_candidate(path)
        .map_err(|_| "path cannot be resolved or escapes its canonical parent".to_owned())?;
    if roots.iter().any(|root| path_is_within(&resolved, root)) {
        Ok(resolved)
    } else {
        Err("path is outside configured allowed_roots".into())
    }
}

fn path_is_within(path: &FsPath, root: &FsPath) -> bool {
    #[cfg(windows)]
    {
        let path = path.to_string_lossy().replace('\\', "/").to_lowercase();
        let root = root.to_string_lossy().replace('\\', "/").to_lowercase();
        let root = root.trim_end_matches('/');
        path == root
            || path
                .strip_prefix(root)
                .is_some_and(|tail| tail.starts_with('/'))
    }
    #[cfg(not(windows))]
    {
        path.starts_with(root)
    }
}

fn allowed_path(state: &ApiState, path: &FsPath) -> Result<PathBuf, ApiError> {
    resolve_allowed_path(path, &state.roots).map_err(|message| {
        if message == "path is outside configured allowed_roots" {
            ApiError::forbidden(message)
        } else {
            ApiError::bad_request(message)
        }
    })
}

fn optional_path(state: &ApiState, path: Option<PathBuf>) -> Result<Option<PathBuf>, ApiError> {
    path.map(|p| allowed_path(state, &p)).transpose()
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status":"ok","schema":1}))
}

#[derive(Deserialize)]
struct PathRequest {
    path: PathBuf,
}

async fn analyze(
    State(state): State<ApiState>,
    Json(body): Json<PathRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let path = allowed_path(&state, &body.path)?;
    let services = Arc::clone(&state.services);
    blocking(move || {
        services
            .analyze(&path)
            .and_then(|view| serde_json::to_value(view).map_err(|e| AppError::Input(e.to_string())))
    })
    .await
    .map(Json)
}

async fn scan(
    State(state): State<ApiState>,
    Json(body): Json<PathRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let path = allowed_path(&state, &body.path)?;
    let services = Arc::clone(&state.services);
    blocking(move || {
        services
            .scan(&path)
            .and_then(|view| serde_json::to_value(view).map_err(|e| AppError::Input(e.to_string())))
    })
    .await
    .map(Json)
}

#[derive(Deserialize)]
struct PlanBody {
    path: PathBuf,
    #[serde(default)]
    from: Option<String>,
    to: String,
    #[serde(default)]
    loader: Option<String>,
    #[serde(default = "default_confidence")]
    min_confidence: f64,
    #[serde(default)]
    source_loader: Option<String>,
    #[serde(default)]
    kb: Option<PathBuf>,
    #[serde(default)]
    mappings: Option<PathBuf>,
    #[serde(default)]
    snapshots: Option<PathBuf>,
}

const fn default_confidence() -> f64 {
    0.8
}

impl PlanBody {
    fn into_request(self, state: &ApiState) -> Result<PlanRequest, ApiError> {
        Ok(PlanRequest {
            input: allowed_path(state, &self.path)?,
            from: self.from,
            to: self.to,
            loader: self.loader,
            min_confidence: self.min_confidence,
            source_loader: self.source_loader,
            kb: optional_path(state, self.kb)?,
            mappings: optional_path(state, self.mappings)?,
            snapshots: optional_path(state, self.snapshots)?,
        })
    }
}

async fn plan(
    State(state): State<ApiState>,
    Json(body): Json<PlanBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let request = body.into_request(&state)?;
    let services = Arc::clone(&state.services);
    blocking(move || {
        services
            .plan(&request)
            .and_then(|view| serde_json::to_value(view).map_err(|e| AppError::Input(e.to_string())))
    })
    .await
    .map(Json)
}

#[derive(Deserialize)]
struct MigrateBody {
    #[serde(flatten)]
    plan: PlanBody,
    reviewed: bool,
}

async fn migrate(
    State(state): State<ApiState>,
    Json(body): Json<MigrateBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !body.reviewed {
        return Err(ApiError::forbidden(
            "review the migration plan before applying it",
        ));
    }
    let request = body.plan.into_request(&state)?;
    let services = Arc::clone(&state.services);
    let id = blocking(move || services.start_migrate_in_place(request, true)).await?;
    Ok(Json(serde_json::json!({"job":id})))
}

async fn port(
    State(state): State<ApiState>,
    Json(mut request): Json<PortRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    request.input = allowed_path(&state, &request.input)?;
    request.output = optional_path(&state, request.output)?;
    request.kb = optional_path(&state, request.kb)?;
    request.mappings = optional_path(&state, request.mappings)?;
    let services = Arc::clone(&state.services);
    let id = blocking(move || services.start_port(request)).await?;
    Ok(Json(serde_json::json!({"job":id})))
}

async fn pack(
    State(state): State<ApiState>,
    Json(mut request): Json<PackRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    request.input = allowed_path(&state, &request.input)?;
    request.sources = optional_path(&state, request.sources)?;
    request.output = optional_path(&state, request.output)?;
    request.kb = optional_path(&state, request.kb)?;
    request.mappings = optional_path(&state, request.mappings)?;
    let services = Arc::clone(&state.services);
    let index: Arc<dyn ModIndex + Send + Sync> = if request.online {
        Arc::new(ummp_ai_providers::ModrinthIndex::new())
    } else {
        Arc::new(NoIndex)
    };
    let id = blocking(move || services.start_pack(request, index)).await?;
    Ok(Json(serde_json::json!({"job":id})))
}

#[derive(Deserialize)]
struct BuildBody {
    path: PathBuf,
    #[serde(default)]
    local: bool,
    #[serde(default)]
    repo: Option<String>,
    #[serde(default)]
    stages: Vec<String>,
    #[serde(default = "default_timeout")]
    timeout_secs: u64,
    #[serde(default)]
    java: Option<u32>,
    #[serde(default)]
    workdir: Option<PathBuf>,
    #[serde(default)]
    accept_risk: bool,
    #[serde(default)]
    confirm_medium: bool,
}

const fn default_timeout() -> u64 {
    1800
}

async fn build(
    State(state): State<ApiState>,
    Json(body): Json<BuildBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let request = BuildJobRequest {
        path: allowed_path(&state, &body.path)?,
        local: body.local,
        repo: body.repo,
        stages: if body.stages.is_empty() {
            vec!["build".into(), "test".into(), "server".into()]
        } else {
            body.stages
        },
        timeout_secs: body.timeout_secs,
        java: body.java,
        workdir: optional_path(&state, body.workdir)?,
        accept_high_risk: body.accept_risk,
        confirm_medium: body.confirm_medium,
    };
    let services = Arc::clone(&state.services);
    let id = blocking(move || services.start_build(request)).await?;
    Ok(Json(serde_json::json!({"job":id})))
}

async fn job(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<JobView>, ApiError> {
    let services = Arc::clone(&state.services);
    blocking(move || services.job(id)).await.map(Json)
}

async fn cancel(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let services = Arc::clone(&state.services);
    blocking(move || services.cancel(id)).await?;
    Ok(StatusCode::ACCEPTED)
}

fn event_job_id(event: &Event) -> Option<Uuid> {
    event
        .payload
        .get("job")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
}

fn to_sse(event: Event) -> SseEvent {
    let kind = event.kind.clone();
    SseEvent::default()
        .event(kind)
        .json_data(event)
        .unwrap_or_else(|_| SseEvent::default())
}

async fn job_events(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Sse<impl tokio_stream::Stream<Item = Result<SseEvent, Infallible>>> {
    let receiver = state.events.subscribe();
    let past: Vec<Event> = state
        .history
        .lock()
        .expect("event history lock")
        .iter()
        .filter(|event| event_job_id(event) == Some(id))
        .cloned()
        .collect();
    let seen: HashSet<Uuid> = past.iter().map(|event| event.id).collect();
    let replay = tokio_stream::iter(past.into_iter().map(|event| Ok(to_sse(event))));
    let live = BroadcastStream::new(receiver).filter_map(move |item| match item {
        Ok(event) if event_job_id(&event) == Some(id) && !seen.contains(&event.id) => {
            Some(Ok(to_sse(event)))
        }
        _ => None,
    });
    Sse::new(replay.chain(live)).keep_alive(KeepAlive::default())
}

#[derive(Deserialize)]
struct RollbackBody {
    #[serde(default)]
    checkpoint: Option<Uuid>,
}

async fn rollback(
    State(state): State<ApiState>,
    Path(project): Path<String>,
    Json(body): Json<RollbackBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let project = allowed_path(&state, FsPath::new(&project))?;
    let services = Arc::clone(&state.services);
    let id =
        blocking(move || services.rollback_with_snapshots(&project, body.checkpoint, None)).await?;
    Ok(Json(serde_json::json!({"restored":true,"checkpoint":id})))
}

/// Fuzz-only entry point for the actual REST JSON request DTOs.
#[cfg(feature = "fuzzing")]
#[doc(hidden)]
pub fn fuzz_decode_rest_request(route: u8, input: &[u8]) {
    if input.len() > BODY_LIMIT {
        return;
    }
    match route % 7 {
        0 => {
            let _ = serde_json::from_slice::<PathRequest>(input);
        }
        1 => {
            let _ = serde_json::from_slice::<PlanBody>(input);
        }
        2 => {
            let _ = serde_json::from_slice::<MigrateBody>(input);
        }
        3 => {
            let _ = serde_json::from_slice::<PortRequest>(input);
        }
        4 => {
            let _ = serde_json::from_slice::<PackRequest>(input);
        }
        5 => {
            let _ = serde_json::from_slice::<BuildBody>(input);
        }
        _ => {
            let _ = serde_json::from_slice::<RollbackBody>(input);
        }
    }
}

async fn openapi() -> Response {
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        include_str!("../../docs/openapi.json"),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn windows_root_comparison_is_case_insensitive_and_handles_drive_roots() {
        assert!(path_is_within(
            FsPath::new(r"C:\Users\Alice\mod"),
            FsPath::new(r"c:\users\alice")
        ));
        assert!(!path_is_within(
            FsPath::new(r"C:\Users\Alice2"),
            FsPath::new(r"c:\users\alice")
        ));
        assert!(path_is_within(FsPath::new(r"C:\mods"), FsPath::new(r"C:\")));
    }

    #[test]
    fn token_comparison_is_constant_time_for_fixed_length_hashes() {
        let expected = Sha256::digest(b"correct-token");
        let supplied = Sha256::digest(b"wrong-token");
        assert!(!bool::from(expected.as_slice().ct_eq(supplied.as_slice())));
        assert!(bool::from(expected.as_slice().ct_eq(expected.as_slice())));
    }

    #[test]
    fn remote_bind_requires_explicit_acknowledgement() {
        let config = ServerConfig {
            listen: "0.0.0.0".parse().unwrap(),
            ..ServerConfig::default()
        };
        assert!(validate_bind(&config).is_err());
        assert!(validate_bind(&ServerConfig {
            allow_remote: true,
            ..config
        })
        .is_ok());
    }

    #[test]
    fn token_never_logged() {
        use std::io::{self, Write};
        use tracing_subscriber::fmt::MakeWriter;

        #[derive(Clone)]
        struct Capture(Arc<Mutex<Vec<u8>>>);
        impl Write for Capture {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.0
                    .lock()
                    .expect("capture lock")
                    .extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        impl<'a> MakeWriter<'a> for Capture {
            type Writer = Self;
            fn make_writer(&'a self) -> Self::Writer {
                self.clone()
            }
        }

        let output = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::fmt()
            .with_writer(Capture(Arc::clone(&output)))
            .without_time()
            .with_ansi(false)
            .finish();
        let token = "synthetic-server-token-never-log";
        tracing::subscriber::with_default(subscriber, || {
            log_server_started("127.0.0.1:7432".parse().unwrap(), token);
        });
        let logs = String::from_utf8(output.lock().unwrap().clone()).unwrap();
        assert!(!logs.contains(token), "{logs}");
    }

    #[cfg(unix)]
    #[test]
    fn token_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let token = load_or_create_token(dir.path()).unwrap();
        let mode = std::fs::metadata(dir.path().join("server-token"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(token, load_or_create_token(dir.path()).unwrap());
    }
}
