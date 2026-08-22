mod i18n;
mod models;
mod web;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::{Arc, Mutex, RwLock},
    time::Instant,
};

use anyhow::{anyhow, Context, Result};
use axum::{
    body::Bytes,
    extract::Request,
    extract::{
        ConnectInfo, DefaultBodyLimit, Extension, Multipart, Path as AxumPath, Query, State,
    },
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post, put},
    Form, Json, Router,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use i18n::{material_fact_label, ui_text, Language};
use models::{
    delete_mineral_records, execute_admin_sql, init_minerals_database,
    is_valid_mineral_folder_name, load_minerals, major_elements_to_text, mineral_slug_exists,
    parse_major_elements, save_localized_mineral_records, Mineral, MineralDiskRecord,
    MineralFormData, NewImageRecord,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    fs,
    net::TcpListener,
    sync::{OwnedSemaphorePermit, Semaphore},
};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use crate::web::{
    AdminIngestionAnomalyView, AdminIngestionBatchView, AdminIngestionCountView,
    AdminIngestionDecisionView, AdminIngestionReviewSampleView, AdminIngestionTemplate,
    AdminReviewCandidateView, AdminReviewEvidenceView, AdminReviewFactView, AdminTemplate,
    ReviewQueueTemplate, TemplateResponse,
};
use minerals::registry;
use minerals::registry::{
    approve_mineral_ingestion_batch, approve_mineral_review, canonical_mineral_chunk_hash,
    create_mineral_ingestion_batch, finalize_mineral_ingestion_batch, get_material_detail,
    get_mineral_ingestion_batch, import_material_batch, import_provider, init_registry_database,
    list_mineral_ingestion_batches, list_pending_mineral_reviews, put_mineral_ingestion_chunk,
    registry_is_ready, registry_stats, reject_mineral_ingestion_batch, reject_mineral_review,
    validate_registry_configuration, withdraw_mineral, MaterialImport, MineralBatchDecisionRequest,
    MineralDatasetManifest, MineralIngestionBatchDetail, MineralIngestionBatchStatus,
    MineralIngestionChunk, MineralIngestionClassification, MineralIngestionProblem,
    MineralIngestionProblemKind, PendingMineralReview, ProviderImport,
};

const ADMIN_REVIEW_PAGE_SIZE: usize = 20;
const ADMIN_REVIEW_BODY_MAX_BYTES: usize = 16 * 1024;
const ADMIN_INGESTION_PAGE_SIZE: usize = 20;
const ADMIN_INGESTION_MANIFEST_MAX_BYTES: usize = 256 * 1024;
// Enough headroom for 500 strict identity skeletons while bounding memory
// after authentication and writer admission run.
const ADMIN_INGESTION_CHUNK_MAX_BYTES: usize = 8 * 1024 * 1024;
const ADMIN_INGESTION_ACTION_MAX_BYTES: usize = 16 * 1024;
const ADMIN_LOGIN_BURST: usize = 10;
const ADMIN_LOGIN_THROTTLED_INTERVAL_SECS: u64 = 5;

#[derive(Clone)]
struct AppState {
    catalogs_by_lang: Arc<RwLock<HashMap<String, MineralCatalog>>>,
    admin_sessions: Arc<Mutex<HashMap<String, Instant>>>,
    admin_login_failures: Arc<Mutex<HashMap<IpAddr, Vec<Instant>>>>,
    admin_drafts: Arc<Mutex<HashMap<String, AdminDraft>>>,
    data_root: Arc<PathBuf>,
    admin_password: Arc<String>,
    openai_api_key: Arc<Option<String>>,
    openai_model: Arc<String>,
    openai_translation_model: Arc<String>,
    default_language: Language,
    http_client: Arc<Client>,
    ingestion_writer: Arc<Semaphore>,
    admin_reviewer_id: Arc<String>,
    ingestion_api_token: Arc<Option<String>>,
    ingestion_adapter_id: Arc<String>,
    trusted_proxy_ips: Arc<BTreeSet<IpAddr>>,
    public_catalog_base_url: Arc<Option<String>>,
    secure_cookies: bool,
    admin_sql_enabled: bool,
}

#[derive(Debug, Clone)]
struct AdminDraft {
    image_bytes: Vec<u8>,
    image_ext: String,
    owner_session: String,
    created_at: Instant,
}

#[derive(Debug, Clone, Default)]
struct MineralCatalog {
    by_slug: HashMap<String, Mineral>,
}

impl MineralCatalog {
    fn new(minerals: Vec<Mineral>) -> Self {
        let by_slug = minerals
            .iter()
            .cloned()
            .map(|mineral| (mineral.slug.clone(), mineral))
            .collect::<HashMap<_, _>>();

        Self { by_slug }
    }
}

#[derive(Debug, Error)]
enum AppError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    TooManyRequests(String),
    #[error("{0}")]
    ServiceUnavailable(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::NotFound(message) => {
                warn!("not found: {message}");
                (StatusCode::NOT_FOUND, message).into_response()
            }
            AppError::Unauthorized(message) => {
                warn!("unauthorized: {message}");
                (StatusCode::UNAUTHORIZED, message).into_response()
            }
            AppError::BadRequest(message) => {
                warn!("bad request: {message}");
                (StatusCode::BAD_REQUEST, message).into_response()
            }
            AppError::Conflict(message) => {
                warn!("conflict: {message}");
                (StatusCode::CONFLICT, message).into_response()
            }
            AppError::TooManyRequests(message) => {
                warn!("rate limited: {message}");
                (StatusCode::TOO_MANY_REQUESTS, message).into_response()
            }
            AppError::ServiceUnavailable(message) => {
                warn!("service unavailable: {message}");
                let mut response = (StatusCode::SERVICE_UNAVAILABLE, message).into_response();
                response
                    .headers_mut()
                    .insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
                response
            }
            AppError::Internal(error) => {
                error!("internal error: {error:#}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_string(),
                )
                    .into_response()
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct AdminLoginRequest {
    password: String,
}

#[derive(Debug, Deserialize)]
struct PublishMineralRequest {
    draft_id: String,
    common_name: String,
    description: String,
    mineral_family: String,
    formula: String,
    hardness_mohs: String,
    density_g_cm3: String,
    crystal_system: String,
    color: String,
    streak: String,
    luster: String,
    major_elements_pct_text: String,
    notes: String,
}

#[derive(Debug, Deserialize)]
struct DeleteMineralRequest {
    slug: String,
}

#[derive(Debug, Deserialize)]
struct WithdrawMineralRequest {
    slug: String,
    operator_note: String,
}

#[derive(Debug, Deserialize)]
struct AdminDbQueryRequest {
    sql: String,
}

#[derive(Debug, Serialize)]
struct AdminDbQueryResponse {
    statement_type: String,
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
    row_count: usize,
    affected_rows: usize,
    truncated: bool,
    message: String,
}

#[derive(Debug, Default, Deserialize)]
struct AdminReviewQuery {
    page: Option<usize>,
    notice: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct AdminIngestionQuery {
    page: Option<usize>,
    notice: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AdminReviewRequest {
    review_id: i64,
    action: String,
    operator_note: String,
}

#[derive(Debug, Deserialize)]
struct AdminIngestionDecisionRequest {
    action: String,
    manifest_hash: String,
    report_hash: String,
    #[serde(default)]
    base_batch_id: String,
    release_version: String,
    release_confirmation: String,
    operator_note: String,
    warning_acknowledged: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyIngestionAction {}

#[derive(Debug, Serialize)]
struct IngestionBatchMutationResponse {
    batch_id: String,
    status: &'static str,
    manifest_hash: String,
    report_hash: Option<String>,
    received_chunk_count: usize,
    expected_chunk_count: usize,
    received_record_count: usize,
    expected_record_count: usize,
}

#[derive(Debug, Serialize)]
struct IngestionChunkMutationResponse {
    batch_id: String,
    chunk_index: usize,
    content_hash: String,
    item_count: usize,
    stored: bool,
    received_chunk_count: usize,
    received_record_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IngestionAuthKind {
    Admin,
    Adapter,
}

impl IngestionAuthKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Adapter => "adapter",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IngestionActor {
    id: String,
    kind: IngestionAuthKind,
}

struct IngestionWriteAdmission {
    actor: IngestionActor,
    _permit: OwnedSemaphorePermit,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum MineralImportPayload {
    One(Box<MaterialImport>),
    Many(Vec<MaterialImport>),
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

#[derive(Debug, Default)]
struct NewMineralDraft {
    common_name: String,
    description: String,
    mineral_family: String,
    formula: String,
    hardness_mohs: f32,
    density_g_cm3: f32,
    crystal_system: String,
    color: String,
    streak: String,
    luster: String,
    notes: String,
    major_elements_pct: BTreeMap<String, f32>,
    image_bytes: Vec<u8>,
    image_ext: String,
}

#[derive(Debug)]
struct SuggestInput {
    suggestion_context: String,
    image_bytes: Vec<u8>,
    image_ext: String,
}

const ADMIN_UPLOAD_MAX_MB: usize = 20;
const ADMIN_UPLOAD_MAX_BYTES: usize = ADMIN_UPLOAD_MAX_MB * 1024 * 1024;
const ADMIN_IMPORT_MAX_BYTES: usize = 50 * 1024 * 1024;
#[derive(Debug, Deserialize)]
struct AiMineralSuggestion {
    common_name: String,
    description: String,
    mineral_family: String,
    formula: String,
    hardness_mohs: f32,
    density_g_cm3: f32,
    crystal_system: String,
    color: String,
    streak: String,
    luster: String,
    major_elements: Vec<AiMajorElement>,
    notes: String,
}

#[derive(Debug, Deserialize)]
struct AiMineralTranslation {
    common_name: String,
    description: String,
    mineral_family: String,
    crystal_system: String,
    color: String,
    streak: String,
    luster: String,
    notes: String,
}

#[derive(Debug, Default)]
struct TranslationStats {
    translated_count: usize,
    fallback_lang_codes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct AiMajorElement {
    element: String,
    percent: f32,
}

#[derive(Debug, Serialize)]
struct ChatCompletionsRequest {
    model: String,
    messages: Vec<ChatMessage>,
    response_format: ResponseFormat,
    temperature: f32,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: Vec<MessagePart>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum MessagePart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrlContent },
}

#[derive(Debug, Serialize)]
struct ImageUrlContent {
    url: String,
}

#[derive(Debug, Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    kind: String,
    json_schema: JsonSchemaSpec,
}

#[derive(Debug, Serialize)]
struct JsonSchemaSpec {
    name: String,
    strict: bool,
    schema: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionsResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChatChoiceMessage {
    content: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("minerals=info"));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    let data_root = std::env::var_os("DATA_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data"));

    let admin_password = std::env::var("ADMIN_PASSWORD")
        .context("ADMIN_PASSWORD is required in the process environment")?;
    if admin_password.trim().is_empty() {
        return Err(anyhow!("ADMIN_PASSWORD cannot be empty"));
    }
    if admin_password.chars().count() < 12 {
        return Err(anyhow!(
            "ADMIN_PASSWORD must contain at least 12 characters"
        ));
    }

    let default_language = match std::env::var("DEFAULT_LANG") {
        Ok(value) => Language::from_code(&value).unwrap_or_else(|| {
            warn!(
                "invalid DEFAULT_LANG='{}'; falling back to '{}'",
                value,
                Language::En.code()
            );
            Language::En
        }),
        Err(_) => Language::En,
    };

    let openai_model = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());
    let openai_translation_model =
        std::env::var("OPENAI_TRANSLATION_MODEL").unwrap_or_else(|_| openai_model.clone());
    let admin_reviewer_id = configured_actor_id("ADMIN_REVIEWER_ID", "local-admin")?;
    let ingestion_api_token = configured_ingestion_api_token()?;
    let ingestion_adapter_id = match std::env::var("INGESTION_ADAPTER_ID") {
        Ok(value) => validate_actor_id("INGESTION_ADAPTER_ID", &value)?,
        Err(std::env::VarError::NotPresent) if ingestion_api_token.is_none() => {
            "local-adapter".to_string()
        }
        Err(std::env::VarError::NotPresent) => {
            return Err(anyhow!(
                "INGESTION_ADAPTER_ID is required when INGESTION_API_TOKEN is configured"
            ))
        }
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(anyhow!("INGESTION_ADAPTER_ID must be valid Unicode"))
        }
    };
    let trusted_proxy_ips = configured_trusted_proxy_ips()?;
    let port = match std::env::var("PORT") {
        Ok(value) => value
            .parse::<u16>()
            .context("PORT must be an integer between 0 and 65535")?,
        Err(std::env::VarError::NotPresent) => 7979,
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(anyhow!("PORT must be valid Unicode"))
        }
    };
    let bind_address = match std::env::var("BIND_ADDRESS") {
        Ok(value) => value
            .parse::<IpAddr>()
            .context("BIND_ADDRESS must be a valid IP address")?,
        Err(std::env::VarError::NotPresent) => "127.0.0.1"
            .parse::<IpAddr>()
            .expect("hard-coded loopback address is valid"),
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(anyhow!("BIND_ADDRESS must be valid Unicode"))
        }
    };
    let secure_cookies = configured_env_flag("COOKIE_SECURE", false)?;
    let admin_sql_enabled = configured_env_flag("ADMIN_SQL_ENABLED", false)?;
    let public_catalog_base_url = configured_public_catalog_base_url()?;
    validate_registry_configuration()?;
    let http_client = Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(90))
        .build()
        .context("failed to initialize HTTP client")?;

    // All required and fallible process configuration is validated before any
    // directories, databases, or migrations are created or changed.
    fs::create_dir_all(data_root.join("minerals"))
        .await
        .context("failed to create data/minerals directory")?;
    init_minerals_database(&data_root).context("failed to initialize data/minerals.db")?;
    init_registry_database(&data_root).context("failed to initialize material registry schema")?;

    let state = AppState {
        catalogs_by_lang: Arc::new(RwLock::new(HashMap::new())),
        admin_sessions: Arc::new(Mutex::new(HashMap::new())),
        admin_login_failures: Arc::new(Mutex::new(HashMap::new())),
        admin_drafts: Arc::new(Mutex::new(HashMap::new())),
        data_root: Arc::new(data_root),
        admin_password: Arc::new(admin_password),
        openai_api_key: Arc::new(std::env::var("OPENAI_API_KEY").ok()),
        openai_model: Arc::new(openai_model),
        openai_translation_model: Arc::new(openai_translation_model),
        default_language,
        http_client: Arc::new(http_client),
        ingestion_writer: Arc::new(Semaphore::new(1)),
        admin_reviewer_id: Arc::new(admin_reviewer_id),
        ingestion_api_token: Arc::new(ingestion_api_token),
        ingestion_adapter_id: Arc::new(ingestion_adapter_id),
        trusted_proxy_ips: Arc::new(trusted_proxy_ips),
        public_catalog_base_url: Arc::new(public_catalog_base_url),
        secure_cookies,
        admin_sql_enabled,
    };

    let app = Router::new()
        .route("/", get(admin_root))
        .route("/livez", get(livez))
        .route("/readyz", get(readyz))
        .route("/healthz", get(healthz))
        .route("/static/:asset", get(admin_static_asset))
        .route("/admin", get(admin_page))
        .route("/admin/reviews", get(admin_review_queue))
        .route("/admin/ingestion", get(admin_ingestion_page))
        .route(
            "/admin/ingestion/batches",
            post(admin_create_ingestion_batch)
                .layer(DefaultBodyLimit::max(ADMIN_INGESTION_MANIFEST_MAX_BYTES))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    admit_ingestion_write_request,
                )),
        )
        .route(
            "/admin/ingestion/batches/:batch_id",
            get(admin_ingestion_batch_status),
        )
        .route(
            "/admin/ingestion/batches/:batch_id/chunks/:chunk_index",
            put(admin_put_ingestion_chunk)
                .layer(DefaultBodyLimit::max(ADMIN_INGESTION_CHUNK_MAX_BYTES))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    admit_ingestion_write_request,
                )),
        )
        .route(
            "/admin/ingestion/batches/:batch_id/finalize",
            post(admin_finalize_ingestion_batch)
                .layer(DefaultBodyLimit::max(ADMIN_INGESTION_ACTION_MAX_BYTES))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    admit_ingestion_write_request,
                )),
        )
        .route(
            "/admin/ingestion/batches/:batch_id/decision",
            post(admin_decide_ingestion_batch)
                .layer(DefaultBodyLimit::max(ADMIN_INGESTION_ACTION_MAX_BYTES)),
        )
        .route("/admin/login", post(admin_login))
        .route("/admin/logout", post(admin_logout))
        .route(
            "/admin/minerals/suggest",
            post(admin_suggest_mineral).layer(DefaultBodyLimit::max(ADMIN_UPLOAD_MAX_BYTES)),
        )
        .route("/admin/minerals/publish", post(admin_publish_mineral))
        .route("/admin/minerals/delete", post(admin_delete_mineral))
        .route(
            "/admin/minerals/withdraw",
            post(admin_withdraw_mineral).layer(DefaultBodyLimit::max(ADMIN_REVIEW_BODY_MAX_BYTES)),
        )
        .route("/admin/db/query", post(admin_db_query))
        .route(
            "/admin/minerals/import",
            post(admin_import_minerals)
                .layer(DefaultBodyLimit::max(ADMIN_IMPORT_MAX_BYTES))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    admit_admin_write_request,
                )),
        )
        .route(
            "/admin/minerals/review",
            post(admin_review_mineral).layer(DefaultBodyLimit::max(ADMIN_REVIEW_BODY_MAX_BYTES)),
        )
        .route(
            "/admin/providers/import",
            post(admin_import_provider)
                .layer(DefaultBodyLimit::max(ADMIN_IMPORT_MAX_BYTES))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    admit_admin_write_request,
                )),
        )
        .layer(middleware::from_fn(security_headers))
        .with_state(state);

    let address = SocketAddr::new(bind_address, port);
    let listener = TcpListener::bind(address)
        .await
        .with_context(|| format!("failed to bind to {address}"))?;

    info!("minerals server listening on http://{address}");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("server failed unexpectedly")?;

    Ok(())
}

async fn run_blocking<T, F>(task: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(task)
        .await
        .context("blocking worker failed")?
}

async fn admin_root() -> Redirect {
    Redirect::to("/admin")
}

async fn livez() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "waajacu-minerals",
    })
}

async fn readyz(State(state): State<AppState>) -> Result<Json<HealthResponse>, AppError> {
    let data_root = state.data_root.clone();
    run_blocking(move || registry_is_ready(data_root.as_path()))
        .await
        .map_err(|err| {
            warn!(error = %err, "readiness check failed");
            AppError::ServiceUnavailable("service is not ready".to_string())
        })?;
    Ok(Json(HealthResponse {
        status: "ok",
        service: "waajacu-minerals",
    }))
}

async fn healthz(State(state): State<AppState>) -> Result<Json<HealthResponse>, AppError> {
    readyz(State(state)).await
}

async fn admin_static_asset(AxumPath(asset): AxumPath<String>) -> Result<Response, AppError> {
    let (body, content_type) = match asset.as_str() {
        "app.css" => (
            include_bytes!("../static/app.css").as_slice(),
            "text/css; charset=utf-8",
        ),
        "theme.js" => (
            include_bytes!("../static/theme.js").as_slice(),
            "text/javascript; charset=utf-8",
        ),
        "favicon.ico" => (
            include_bytes!("../static/favicon.ico").as_slice(),
            "image/x-icon",
        ),
        "logo_transparent.png" => (
            include_bytes!("../static/logo_transparent.png").as_slice(),
            "image/png",
        ),
        "logo_transparent_dark.png" => (
            include_bytes!("../static/logo_transparent_dark.png").as_slice(),
            "image/png",
        ),
        "loading_1.png" => (
            include_bytes!("../static/loading_1.png").as_slice(),
            "image/png",
        ),
        "loading_2.png" => (
            include_bytes!("../static/loading_2.png").as_slice(),
            "image/png",
        ),
        _ => return Err(AppError::NotFound("admin asset not found".to_string())),
    };
    let mut response = (StatusCode::OK, Bytes::from_static(body)).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, no-cache, max-age=0, must-revalidate"),
    );
    Ok(response)
}

async fn security_headers(request: Request, next: Next) -> Response {
    let path = request.uri().path();
    let private_admin_response = is_admin_path(path);
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    if private_admin_response {
        headers.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store, max-age=0"),
        );
    }
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'; connect-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'none'; form-action 'self'"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    headers.insert(
        header::HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    response
}

fn is_admin_path(path: &str) -> bool {
    path == "/admin" || path.starts_with("/admin/")
}

fn mineral_status_display<'a>(status: &str, text: &'a i18n::RegistryText) -> &'a str {
    match status.trim().to_ascii_lowercase().as_str() {
        "verified" => text.status_verified,
        "reviewed" => text.status_reviewed,
        "sourced" => text.status_sourced,
        "disputed" => text.status_disputed,
        _ => text.status_preliminary,
    }
}

fn review_claim_scope_display(language: Language, scope: &str) -> String {
    let normalized = scope.trim().to_ascii_lowercase();
    let ui = ui_text(language);
    let text = ui.registry;
    let key = normalized.rsplit('.').next().unwrap_or(&normalized);
    if let Some(label) = material_fact_label(language, key) {
        return label.to_string();
    }

    match normalized.as_str() {
        "identity" | "identity.canonical_name" => text.identity.to_string(),
        "identity.description" => ui.label_description.to_string(),
        "identity.formula" | "formula" => ui.label_formula.to_string(),
        "identity.mineral_family" | "mineral_family" => ui.label_family.to_string(),
        "identifiers.cas_number" | "cas_number" => "CAS".to_string(),
        "identifiers" => text.identifiers.to_string(),
        "properties" => text.properties.to_string(),
        "safety" => text.safety.to_string(),
        _ => key
            .split('_')
            .filter(|part| !part.is_empty())
            .map(|part| {
                let mut chars = part.chars();
                chars
                    .next()
                    .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn review_value_display(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            serde_json::to_string(value).unwrap_or_default()
        }
    }
}

fn admin_review_facts_for_ui(
    language: Language,
    values: &serde_json::Value,
) -> Vec<AdminReviewFactView> {
    let Some(values) = values.as_object() else {
        return Vec::new();
    };
    values
        .iter()
        .map(|(key, value)| AdminReviewFactView {
            name: material_fact_label(language, key)
                .map(str::to_string)
                .unwrap_or_else(|| key.replace('_', " ")),
            value: review_value_display(value),
        })
        .collect()
}

fn admin_review_candidate_for_ui(
    review: PendingMineralReview,
    language: Language,
    is_update: bool,
    public_catalog_base_url: Option<&str>,
) -> AdminReviewCandidateView {
    let registry_text = ui_text(language).registry;
    let quality = if review.record.data_quality_score.is_finite() {
        review.record.data_quality_score.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let evidence = review
        .record
        .sources
        .iter()
        .map(|source| {
            let confidence = if source.confidence.is_finite() {
                source.confidence.clamp(0.0, 1.0)
            } else {
                0.0
            };
            let mut claim_value_display = source
                .claim
                .get("value")
                .map(review_value_display)
                .unwrap_or_else(|| review_value_display(&source.claim));
            if let Some(unit) = source
                .claim
                .get("unit")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|unit| !unit.is_empty())
            {
                if !claim_value_display.is_empty() {
                    claim_value_display.push(' ');
                }
                claim_value_display.push_str(unit);
            }
            AdminReviewEvidenceView {
                title: source.title.clone(),
                publisher: source.publisher.clone(),
                claim_scope_display: review_claim_scope_display(language, &source.claim_scope),
                claim_value_display,
                claim_locator: source
                    .claim
                    .get("source_locator")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                claim_note: source
                    .claim
                    .get("note")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                claim_json: serde_json::to_string_pretty(&source.claim).unwrap_or_default(),
                confidence_display: format!("{:.0}%", confidence * 100.0),
                review_status_display: mineral_status_display(
                    &source.review_status,
                    &registry_text,
                )
                .to_string(),
                canonical_url: source.url.clone(),
                source_license: source.license_spdx.clone(),
                retrieved_at_display: source.retrieved_at.clone(),
                content_hash: source.content_hash.clone(),
            }
        })
        .collect();

    let identifiers = admin_review_facts_for_ui(language, &review.record.identifiers);
    let properties = admin_review_facts_for_ui(language, &review.record.properties);
    let safety = admin_review_facts_for_ui(language, &review.record.safety);
    let payload_json = serde_json::to_string_pretty(&review.record).unwrap_or_default();
    let current_profile_path = if is_update {
        public_catalog_base_url
            .map(|base| format!("{base}/#/minerals/{}", review.record.slug))
            .unwrap_or_default()
    } else {
        String::new()
    };

    AdminReviewCandidateView {
        review_id: review.review_id,
        revision: review.revision,
        slug: review.record.slug,
        is_update,
        current_profile_path,
        canonical_name: review.record.canonical_name,
        formula: review.record.formula,
        mineral_family: review.record.mineral_family,
        description: review.record.description,
        cas_number: review.record.cas_number.unwrap_or_default(),
        synonyms: review.record.synonyms,
        identifiers,
        properties,
        safety,
        record_license: review.record.license_spdx,
        payload_json,
        scientific_status_display: mineral_status_display(
            &review.record.verification_status,
            &registry_text,
        )
        .to_string(),
        quality_display: format!("{:.0}%", quality * 100.0),
        source_display: review.source_label,
        submitted_at_display: review.submitted_at,
        evidence,
    }
}

fn client_ip_for_rate_limit(state: &AppState, peer_ip: IpAddr, headers: &HeaderMap) -> IpAddr {
    resolve_client_ip(peer_ip, headers, state.trusted_proxy_ips.as_ref())
}

fn resolve_client_ip(
    peer_ip: IpAddr,
    headers: &HeaderMap,
    trusted_proxy_ips: &BTreeSet<IpAddr>,
) -> IpAddr {
    if !trusted_proxy_ips.contains(&peer_ip) {
        return peer_ip;
    }

    let forwarded_values: Vec<_> = headers.get_all("x-forwarded-for").iter().collect();
    let mut inspected_hops = 0_usize;
    for value in forwarded_values.into_iter().rev() {
        let Ok(value) = value.to_str() else {
            return peer_ip;
        };
        for hop in value.rsplit(',') {
            let hop = hop.trim();
            if hop.is_empty() || inspected_hops >= 32 {
                return peer_ip;
            }
            inspected_hops += 1;
            let Ok(ip) = hop.parse::<IpAddr>() else {
                return peer_ip;
            };
            if !trusted_proxy_ips.contains(&ip) {
                return ip;
            }
        }
    }

    // A trusted proxy appends or overwrites X-Forwarded-For. Walk from the
    // nearest hop toward the client and stop at the first address outside the
    // configured trust boundary. Do not parse anything farther left: that
    // prefix came from outside the trust boundary and may be attacker supplied.
    peer_ip
}

async fn admin_page(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> TemplateResponse<AdminTemplate> {
    let language = resolve_language(&state, &headers);
    TemplateResponse(AdminTemplate {
        lang_code: language.code().to_string(),
        lang_dir: language.dir().to_string(),
        txt: ui_text(language),
        public_catalog_url: public_catalog_minerals_url(&state),
        has_admin_session: has_admin_session(&state, &headers),
        error_message: None,
        success_message: None,
        draft_form: MineralFormData::default(),
        has_suggestion: false,
    })
}

fn ingestion_status_code(status: MineralIngestionBatchStatus) -> &'static str {
    match status {
        MineralIngestionBatchStatus::Receiving => "receiving",
        MineralIngestionBatchStatus::Ready => "ready",
        MineralIngestionBatchStatus::NeedsAttention => "needs_attention",
        MineralIngestionBatchStatus::Approved => "approved",
        MineralIngestionBatchStatus::Rejected => "rejected",
    }
}

fn ingestion_batch_mutation_response(
    detail: &MineralIngestionBatchDetail,
) -> IngestionBatchMutationResponse {
    IngestionBatchMutationResponse {
        batch_id: detail.batch_id.clone(),
        status: ingestion_status_code(detail.status),
        manifest_hash: detail.manifest_hash.clone(),
        report_hash: detail.report_hash.clone(),
        received_chunk_count: detail.received_chunk_count,
        expected_chunk_count: detail.manifest.expected_chunk_count,
        received_record_count: detail.received_record_count,
        expected_record_count: detail.manifest.expected_record_count,
    }
}

fn map_ingestion_backend_error(operation: &'static str, error: anyhow::Error) -> AppError {
    if let Some(problem) = error.downcast_ref::<MineralIngestionProblem>() {
        warn!(
            operation,
            problem_code = problem.code,
            problem_kind = ?problem.kind,
            error = %error,
            "mineral ingestion request rejected"
        );
        return match problem.kind {
            MineralIngestionProblemKind::Invalid => {
                AppError::BadRequest(format!("ingestion request rejected: {}", problem.code))
            }
            MineralIngestionProblemKind::NotFound => {
                AppError::NotFound(format!("ingestion request rejected: {}", problem.code))
            }
            MineralIngestionProblemKind::Conflict => {
                AppError::Conflict(format!("ingestion request rejected: {}", problem.code))
            }
        };
    }

    AppError::Internal(error.context(operation))
}

fn try_ingestion_writer_permit(
    writer: &Arc<Semaphore>,
) -> Result<tokio::sync::OwnedSemaphorePermit, AppError> {
    writer.clone().try_acquire_owned().map_err(|_| {
        AppError::ServiceUnavailable("ingestion request rejected: writer_busy".to_string())
    })
}

fn require_ingestion_write_admission(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<IngestionWriteAdmission, AppError> {
    // Authentication deliberately precedes admission so an unauthenticated
    // caller cannot use writer availability as an oracle.
    let actor = require_ingestion_writer_actor(state, headers)?;
    let permit = try_ingestion_writer_permit(&state.ingestion_writer)?;
    require_json_content_type(headers)?;
    Ok(IngestionWriteAdmission {
        actor,
        _permit: permit,
    })
}

async fn admit_ingestion_write_request(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let admission = require_ingestion_write_admission(&state, request.headers())?;
    request.extensions_mut().insert(Arc::new(admission));
    Ok(next.run(request).await)
}

async fn admit_admin_write_request(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    require_admin_session(&state, request.headers())?;
    require_same_origin(request.headers())?;
    Ok(next.run(request).await)
}

fn require_json_content_type(headers: &HeaderMap) -> Result<(), AppError> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if content_type
        .split(';')
        .next()
        .is_none_or(|value| !value.trim().eq_ignore_ascii_case("application/json"))
    {
        return Err(AppError::BadRequest(
            "ingestion request rejected: invalid_content_type".to_string(),
        ));
    }
    Ok(())
}

fn supplied_ingestion_content_hash(headers: &HeaderMap) -> Result<Option<&str>, AppError> {
    const CONTENT_HASH_HEADER: &str = "x-content-sha256";
    if headers.get_all(CONTENT_HASH_HEADER).iter().count() > 1 {
        return Err(AppError::BadRequest(
            "ingestion request rejected: duplicate_content_hash".to_string(),
        ));
    }
    headers
        .get(CONTENT_HASH_HEADER)
        .map(|value| {
            value.to_str().map_err(|_| {
                AppError::BadRequest("ingestion request rejected: invalid_content_hash".to_string())
            })
        })
        .transpose()
}

fn require_valid_ingestion_batch_id(batch_id: &str) -> Result<(), AppError> {
    let valid = batch_id.strip_prefix("batch_").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    });
    if !valid {
        return Err(AppError::BadRequest(
            "ingestion request rejected: invalid_batch_id".to_string(),
        ));
    }
    Ok(())
}

async fn admin_create_ingestion_batch(
    State(state): State<AppState>,
    Extension(admission): Extension<Arc<IngestionWriteAdmission>>,
    body: Bytes,
) -> Result<Json<IngestionBatchMutationResponse>, AppError> {
    let actor = admission.actor.clone();
    let manifest = serde_json::from_slice::<MineralDatasetManifest>(&body).map_err(|_| {
        AppError::BadRequest("ingestion request rejected: invalid_manifest_json".to_string())
    })?;
    let data_root = state.data_root.clone();
    let actor_id = actor.id.clone();
    let detail = run_blocking(move || {
        create_mineral_ingestion_batch(data_root.as_path(), &actor_id, &manifest)
    })
    .await
    .map_err(|error| map_ingestion_backend_error("create mineral ingestion batch", error))?;
    info!(
        batch_id = %detail.batch_id,
        actor_id = %actor.id,
        actor_kind = actor.kind.as_str(),
        status = ingestion_status_code(detail.status),
        expected_chunks = detail.manifest.expected_chunk_count,
        expected_records = detail.manifest.expected_record_count,
        "mineral ingestion batch created or recovered"
    );
    Ok(Json(ingestion_batch_mutation_response(&detail)))
}

async fn admin_ingestion_batch_status(
    State(state): State<AppState>,
    AxumPath(batch_id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Json<IngestionBatchMutationResponse>, AppError> {
    let actor = require_ingestion_reader_actor(&state, &headers)?;
    require_valid_ingestion_batch_id(&batch_id)?;
    let data_root = state.data_root.clone();
    let stored_batch_id = batch_id.clone();
    let detail =
        run_blocking(move || get_mineral_ingestion_batch(data_root.as_path(), &stored_batch_id))
            .await
            .map_err(|error| map_ingestion_backend_error("read mineral ingestion batch", error))?
            .ok_or_else(|| {
                AppError::NotFound("ingestion request rejected: batch_not_found".to_string())
            })?;
    info!(
        batch_id = %detail.batch_id,
        actor_id = %actor.id,
        actor_kind = actor.kind.as_str(),
        status = ingestion_status_code(detail.status),
        "mineral ingestion batch status read"
    );
    Ok(Json(ingestion_batch_mutation_response(&detail)))
}

async fn admin_put_ingestion_chunk(
    State(state): State<AppState>,
    AxumPath((batch_id, chunk_index)): AxumPath<(String, usize)>,
    Extension(admission): Extension<Arc<IngestionWriteAdmission>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<IngestionChunkMutationResponse>, AppError> {
    let actor = admission.actor.clone();
    require_valid_ingestion_batch_id(&batch_id)?;
    let chunk = serde_json::from_slice::<MineralIngestionChunk>(&body).map_err(|_| {
        AppError::BadRequest("ingestion request rejected: invalid_chunk_json".to_string())
    })?;
    if chunk.chunk_index != chunk_index {
        return Err(AppError::BadRequest(
            "ingestion request rejected: chunk_index_mismatch".to_string(),
        ));
    }
    let content_hash = canonical_mineral_chunk_hash(&chunk)
        .map_err(|error| AppError::Internal(error.context("hash mineral ingestion chunk")))?;
    if supplied_ingestion_content_hash(&headers)?.is_some_and(|supplied| supplied != content_hash) {
        return Err(AppError::Conflict(
            "ingestion request rejected: content_hash_mismatch".to_string(),
        ));
    }

    let data_root = state.data_root.clone();
    let stored_batch_id = batch_id.clone();
    let actor_id = actor.id.clone();
    let stored_hash = content_hash.clone();
    let receipt = run_blocking(move || {
        put_mineral_ingestion_chunk(
            data_root.as_path(),
            &stored_batch_id,
            &actor_id,
            &stored_hash,
            &chunk,
        )
    })
    .await
    .map_err(|error| map_ingestion_backend_error("store mineral ingestion chunk", error))?;
    info!(
        batch_id = %receipt.batch_id,
        chunk_index = receipt.chunk_index,
        stored = receipt.stored,
        item_count = receipt.item_count,
        actor_id = %actor.id,
        actor_kind = actor.kind.as_str(),
        "mineral ingestion chunk accepted"
    );
    Ok(Json(IngestionChunkMutationResponse {
        batch_id: receipt.batch_id,
        chunk_index: receipt.chunk_index,
        content_hash: receipt.content_hash,
        item_count: receipt.item_count,
        stored: receipt.stored,
        received_chunk_count: receipt.received_chunk_count,
        received_record_count: receipt.received_record_count,
    }))
}

async fn admin_finalize_ingestion_batch(
    State(state): State<AppState>,
    AxumPath(batch_id): AxumPath<String>,
    Extension(admission): Extension<Arc<IngestionWriteAdmission>>,
    body: Bytes,
) -> Result<Json<IngestionBatchMutationResponse>, AppError> {
    let actor = admission.actor.clone();
    require_valid_ingestion_batch_id(&batch_id)?;
    serde_json::from_slice::<EmptyIngestionAction>(&body).map_err(|_| {
        AppError::BadRequest("ingestion request rejected: invalid_action_json".to_string())
    })?;
    let data_root = state.data_root.clone();
    let stored_batch_id = batch_id.clone();
    let actor_id = actor.id.clone();
    let detail = run_blocking(move || {
        finalize_mineral_ingestion_batch(data_root.as_path(), &stored_batch_id, &actor_id)
    })
    .await
    .map_err(|error| map_ingestion_backend_error("finalize mineral ingestion batch", error))?;
    info!(
        batch_id = %detail.batch_id,
        actor_id = %actor.id,
        actor_kind = actor.kind.as_str(),
        status = ingestion_status_code(detail.status),
        report_hash = detail.report_hash.as_deref().unwrap_or(""),
        "mineral ingestion batch finalized"
    );
    Ok(Json(ingestion_batch_mutation_response(&detail)))
}

async fn admin_decide_ingestion_batch(
    State(state): State<AppState>,
    AxumPath(batch_id): AxumPath<String>,
    headers: HeaderMap,
    Form(request): Form<AdminIngestionDecisionRequest>,
) -> Result<Redirect, AppError> {
    let reviewer_id = require_ingestion_reviewer_actor(&state, &headers)?;
    require_valid_ingestion_batch_id(&batch_id)?;
    let approve = match request.action.as_str() {
        "approve" => true,
        "reject" => false,
        _ => {
            return Err(AppError::BadRequest(
                "decision action must be approve or reject".to_string(),
            ))
        }
    };
    if request.warning_acknowledged.as_deref() != Some("1") {
        return Err(AppError::BadRequest(
            "the release warning must be acknowledged".to_string(),
        ));
    }
    let manifest_hash = required_string_limited(&request.manifest_hash, "manifest_hash", 128)?;
    let report_hash = required_string_limited(&request.report_hash, "report_hash", 128)?;
    if request.release_version.is_empty() || request.release_version.chars().count() > 200 {
        return Err(AppError::BadRequest(
            "release_version must be present and no longer than 200 characters".to_string(),
        ));
    }
    let release_version = request.release_version;
    let operator_note = required_string_limited(&request.operator_note, "operator_note", 2_000)?;
    if request.release_confirmation != release_version {
        return Err(AppError::BadRequest(
            "release confirmation does not match the exact release version".to_string(),
        ));
    }
    let base_batch_id = match request.base_batch_id.as_str() {
        "" => None,
        value => Some(required_string_limited(value, "base_batch_id", 128)?),
    };

    let _permit = try_ingestion_writer_permit(&state.ingestion_writer)?;
    let data_root = state.data_root.clone();
    let lookup_batch_id = batch_id.clone();
    let current =
        run_blocking(move || get_mineral_ingestion_batch(data_root.as_path(), &lookup_batch_id))
            .await
            .map_err(|error| map_ingestion_backend_error("read mineral ingestion decision", error))?
            .ok_or_else(|| {
                AppError::NotFound("ingestion request rejected: batch_not_found".to_string())
            })?;
    if current.manifest_hash != manifest_hash
        || current.report_hash.as_deref() != Some(report_hash.as_str())
        || current.manifest.base_batch_id != base_batch_id
        || current.manifest.release.version != release_version
    {
        return Err(AppError::Conflict(
            "release coordinates changed; refresh the batch before deciding".to_string(),
        ));
    }

    let decision = MineralBatchDecisionRequest {
        manifest_hash,
        report_hash,
        base_batch_id,
        note: operator_note,
    };
    let data_root = state.data_root.clone();
    let actor_id = reviewer_id.clone();
    let decided_batch_id = batch_id.clone();
    let outcome = run_blocking(move || {
        if approve {
            approve_mineral_ingestion_batch(
                data_root.as_path(),
                &decided_batch_id,
                &actor_id,
                &decision,
            )
        } else {
            reject_mineral_ingestion_batch(
                data_root.as_path(),
                &decided_batch_id,
                &actor_id,
                &decision,
            )
        }
    })
    .await
    .map_err(|error| map_ingestion_backend_error("decide mineral ingestion batch", error))?;
    info!(
        batch_id = %outcome.batch_id,
        actor_id = %reviewer_id,
        action = if approve { "approve" } else { "reject" },
        status = ingestion_status_code(outcome.status),
        changed = outcome.changed,
        created = outcome.applied_create_count,
        adopted = outcome.applied_adopt_count,
        updated = outcome.applied_update_count,
        unchanged = outcome.unchanged_count,
        retired_offers = outcome.retired_offer_count,
        "mineral ingestion release decision recorded"
    );
    let notice = if approve { "approved" } else { "rejected" };
    Ok(Redirect::to(&format!("/admin/ingestion?notice={notice}")))
}

fn ingestion_status_display(
    status: MineralIngestionBatchStatus,
    text: &i18n::IngestionText,
) -> &'static str {
    match status {
        MineralIngestionBatchStatus::Receiving => text.status_receiving,
        MineralIngestionBatchStatus::Ready => text.status_ready,
        MineralIngestionBatchStatus::NeedsAttention => text.status_needs_attention,
        MineralIngestionBatchStatus::Approved => text.status_approved,
        MineralIngestionBatchStatus::Rejected => text.status_rejected,
    }
}

fn ingestion_classification_display(
    classification: MineralIngestionClassification,
    text: &i18n::IngestionText,
) -> &'static str {
    match classification {
        MineralIngestionClassification::Create => text.created,
        MineralIngestionClassification::Adopt => text.adopted,
        MineralIngestionClassification::Update => text.updated,
        MineralIngestionClassification::Unchanged => text.unchanged,
        MineralIngestionClassification::Conflict => text.blockers,
        MineralIngestionClassification::Missing => text.missing,
    }
}

fn ingestion_progress_percent(received: usize, expected: usize) -> usize {
    if expected == 0 {
        0
    } else {
        received
            .saturating_mul(100)
            .checked_div(expected)
            .unwrap_or(0)
            .min(100)
    }
}

fn safe_external_http_url(value: &str) -> Option<String> {
    let value = value.trim();
    let parsed = reqwest::Url::parse(value).ok()?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return None;
    }
    Some(value.to_string())
}

fn ingestion_batch_for_ui(
    detail: MineralIngestionBatchDetail,
    language: Language,
) -> AdminIngestionBatchView {
    let text = ui_text(language).ingestion;
    let expected_chunk_count = detail.manifest.expected_chunk_count;
    let expected_record_count = detail.manifest.expected_record_count;
    let manifest_schema_version = detail.manifest.schema_version;
    let attribution = detail.manifest.source.attribution.as_ref();
    let attribution_party = attribution
        .map(|value| value.attribution_party.clone())
        .unwrap_or_default();
    let attribution_work_title = attribution
        .map(|value| value.work_title.clone())
        .unwrap_or_default();
    let attribution_work_url_display = attribution
        .map(|value| value.work_url.clone())
        .unwrap_or_default();
    let attribution_license_url_display = attribution
        .map(|value| value.license_url.clone())
        .unwrap_or_default();
    let attribution_changes_notice = attribution
        .map(|value| value.changes_notice.clone())
        .unwrap_or_default();
    let attribution_no_endorsement_notice = attribution
        .map(|value| value.no_endorsement_notice.clone())
        .unwrap_or_default();
    let attribution_derived_output_license_spdx = attribution
        .map(|value| value.derived_output_license_spdx.clone())
        .unwrap_or_default();
    let attribution_work_url_href = safe_external_http_url(&attribution_work_url_display);
    let attribution_license_url_href = safe_external_http_url(&attribution_license_url_display);
    let attribution_text_complete = [
        attribution_party.as_str(),
        attribution_work_title.as_str(),
        attribution_work_url_display.as_str(),
        attribution_license_url_display.as_str(),
        attribution_changes_notice.as_str(),
        attribution_no_endorsement_notice.as_str(),
        attribution_derived_output_license_spdx.as_str(),
    ]
    .iter()
    .all(|value| !value.trim().is_empty());
    let source_license = detail.manifest.source.license_spdx.as_str();
    let attribution_complete = manifest_schema_version
        == registry::MINERAL_INGESTION_SCHEMA_VERSION
        && attribution_text_complete
        && attribution_work_url_href.is_some()
        && attribution_license_url_href
            .as_deref()
            .is_some_and(|url| url.starts_with("https://"))
        && !matches!(source_license, "" | "NONE" | "NOASSERTION");
    let report_summary = detail
        .report_summary
        .as_ref()
        .map(|summary| AdminIngestionCountView {
            create_count: summary.create_count,
            adopt_count: summary.adopt_count,
            update_count: summary.update_count,
            unchanged_count: summary.unchanged_count,
            conflict_count: summary.conflict_count,
            missing_count: summary.missing_count,
            identity_critical_warning_count: summary.identity_critical_warning_count,
        });
    let review_samples = detail
        .review_samples
        .iter()
        .map(|item| AdminIngestionReviewSampleView {
            source_record_id: item.source_record_id.clone(),
            canonical_name: item.canonical_name.clone(),
            formula: item.formula.clone(),
            nomenclature_status_display: item.nomenclature_status.clone(),
            is_valid_species: item.is_valid_species,
        })
        .collect::<Vec<_>>();
    let anomaly_samples = detail
        .anomaly_samples
        .iter()
        .map(|item| AdminIngestionAnomalyView {
            source_record_id: item.source_record_id.clone(),
            proposed_slug: item.proposed_slug.clone(),
            resolved_slug: item.resolved_slug.clone().unwrap_or_default(),
            classification_display: ingestion_classification_display(item.classification, &text)
                .to_string(),
            severity_display: item.severity.clone(),
            code: item.code.clone(),
            message: item.message.clone(),
            critical_formula_change: item.critical_formula_change,
            critical_validity_change: item.critical_validity_change,
        })
        .collect::<Vec<_>>();
    let decision = if matches!(
        detail.status,
        MineralIngestionBatchStatus::Ready | MineralIngestionBatchStatus::NeedsAttention
    ) {
        detail
            .report_hash
            .as_ref()
            .map(|report_hash| AdminIngestionDecisionView {
                batch_id: detail.batch_id.clone(),
                manifest_hash: detail.manifest_hash.clone(),
                report_hash: report_hash.clone(),
                base_batch_id: detail.manifest.base_batch_id.clone().unwrap_or_default(),
                release_version: detail.manifest.release.version.clone(),
            })
    } else {
        None
    };
    let source_url_display = detail.manifest.source.url.trim().to_string();
    let status_display = ingestion_status_display(detail.status, &text).to_string();
    let status_note = format!(
        "{} {}/{} · {} {}/{}",
        text.uploaded_chunks,
        detail.received_chunk_count,
        expected_chunk_count,
        text.records,
        detail.received_record_count,
        expected_record_count
    );
    AdminIngestionBatchView {
        batch_id: detail.batch_id,
        status_display,
        status_note,
        is_receiving: detail.status == MineralIngestionBatchStatus::Receiving,
        is_ready: detail.status == MineralIngestionBatchStatus::Ready,
        needs_attention: detail.status == MineralIngestionBatchStatus::NeedsAttention,
        is_approved: detail.status == MineralIngestionBatchStatus::Approved,
        is_rejected: detail.status == MineralIngestionBatchStatus::Rejected,
        can_approve: detail.status == MineralIngestionBatchStatus::Ready && attribution_complete,
        manifest_schema_version,
        dataset_key: detail.manifest.dataset.key,
        dataset_title: detail.manifest.dataset.title,
        source_key: detail.manifest.source.key,
        source_url_href: safe_external_http_url(&source_url_display),
        source_url_display,
        source_license: detail.manifest.source.license_spdx,
        attribution_complete,
        attribution_party,
        attribution_work_title,
        attribution_work_url_href,
        attribution_work_url_display,
        attribution_license_url_href,
        attribution_license_url_display,
        attribution_changes_notice,
        attribution_no_endorsement_notice,
        attribution_derived_output_license_spdx,
        release_version: detail.manifest.release.version,
        released_at_display: detail.manifest.release.released_at,
        retrieved_at_display: detail.manifest.retrieval.retrieved_at,
        parser_name: detail.manifest.parser.name,
        parser_version: detail.manifest.parser.version,
        parser_code_revision: detail.manifest.parser.code_revision,
        parser_configuration_hash: detail.manifest.parser.configuration_sha256,
        artifact_hash: detail.manifest.artifact.sha256,
        manifest_hash: detail.manifest_hash,
        report_hash: detail.report_hash.unwrap_or_default(),
        records_hash: detail.manifest.records_sha256,
        base_batch_display: detail.manifest.base_batch_id.unwrap_or_default(),
        received_chunk_count: detail.received_chunk_count,
        expected_chunk_count,
        received_record_count: detail.received_record_count,
        expected_record_count,
        chunk_progress_percent: ingestion_progress_percent(
            detail.received_chunk_count,
            expected_chunk_count,
        ),
        record_progress_percent: ingestion_progress_percent(
            detail.received_record_count,
            expected_record_count,
        ),
        report_summary,
        review_samples,
        anomaly_samples,
        created_at_display: detail.created_at,
        finalized_at_display: detail.finalized_at.unwrap_or_default(),
        decision,
    }
}

fn admin_ingestion_notice(language: Language, notice: Option<&str>) -> Option<String> {
    let text = ui_text(language).ingestion;
    match notice {
        Some("approved") => Some(text.approved_notice.to_string()),
        Some("rejected") => Some(text.rejected_notice.to_string()),
        _ => None,
    }
}

async fn admin_ingestion_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AdminIngestionQuery>,
) -> Result<TemplateResponse<AdminIngestionTemplate>, AppError> {
    require_admin_session(&state, &headers)?;
    let language = resolve_language(&state, &headers);
    let requested_page = query.page.unwrap_or(1).max(1);
    let data_root = state.data_root.clone();
    let (first_page, published_mineral_count) = run_blocking(move || {
        let page =
            list_mineral_ingestion_batches(data_root.as_path(), ADMIN_INGESTION_PAGE_SIZE, 0)?;
        let published_mineral_count = registry_stats(data_root.as_path())?.mineral_count;
        Ok((page, published_mineral_count))
    })
    .await?;
    let total_results = first_page.total_count;
    let total_pages = total_results
        .saturating_add(ADMIN_INGESTION_PAGE_SIZE - 1)
        .checked_div(ADMIN_INGESTION_PAGE_SIZE)
        .unwrap_or(0)
        .max(1);
    let current_page = requested_page.min(total_pages);
    let offset = (current_page - 1).saturating_mul(ADMIN_INGESTION_PAGE_SIZE);
    let page = if offset == 0 {
        first_page
    } else {
        let data_root = state.data_root.clone();
        run_blocking(move || {
            list_mineral_ingestion_batches(data_root.as_path(), ADMIN_INGESTION_PAGE_SIZE, offset)
        })
        .await?
    };
    let page_start = if total_results == 0 { 0 } else { offset + 1 };
    let page_end = offset.saturating_add(page.items.len()).min(total_results);
    let batches = page
        .items
        .into_iter()
        .map(|detail| ingestion_batch_for_ui(detail, language))
        .collect();

    Ok(TemplateResponse(AdminIngestionTemplate {
        lang_code: language.code().to_string(),
        lang_dir: language.dir().to_string(),
        txt: ui_text(language),
        public_catalog_url: public_catalog_minerals_url(&state),
        has_admin_session: true,
        error_message: None,
        success_message: admin_ingestion_notice(language, query.notice.as_deref()),
        published_mineral_count,
        batches,
        total_results,
        current_page,
        total_pages,
        page_start,
        page_end,
        has_previous_page: current_page > 1,
        previous_page: current_page.saturating_sub(1).max(1),
        has_next_page: total_results > 0 && current_page < total_pages,
        next_page: current_page.saturating_add(1).min(total_pages),
    }))
}

fn admin_review_notice(language: Language, notice: Option<&str>) -> Option<String> {
    let review_text = ui_text(language).review;
    match notice {
        Some("approved") => Some(review_text.approved_notice.to_string()),
        Some("rejected") => Some(review_text.rejected_notice.to_string()),
        _ => None,
    }
}

async fn admin_review_queue(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AdminReviewQuery>,
) -> Result<TemplateResponse<ReviewQueueTemplate>, AppError> {
    require_admin_session(&state, &headers)?;
    let language = resolve_language(&state, &headers);
    let requested_page = query.page.unwrap_or(1).max(1);

    let data_root = state.data_root.clone();
    let first_page = run_blocking(move || {
        list_pending_mineral_reviews(data_root.as_path(), ADMIN_REVIEW_PAGE_SIZE, 0)
    })
    .await?;
    let total_results = first_page.total_count;
    let total_pages = total_results
        .saturating_add(ADMIN_REVIEW_PAGE_SIZE - 1)
        .checked_div(ADMIN_REVIEW_PAGE_SIZE)
        .unwrap_or(0)
        .max(1);
    let current_page = requested_page.min(total_pages);
    let offset = (current_page - 1).saturating_mul(ADMIN_REVIEW_PAGE_SIZE);
    let page = if offset == 0 {
        first_page
    } else {
        let data_root = state.data_root.clone();
        run_blocking(move || {
            list_pending_mineral_reviews(data_root.as_path(), ADMIN_REVIEW_PAGE_SIZE, offset)
        })
        .await?
    };

    let page_start = if total_results == 0 { 0 } else { offset + 1 };
    let page_end = offset.saturating_add(page.items.len()).min(total_results);
    let success_message = admin_review_notice(language, query.notice.as_deref());
    let candidate_slugs = page
        .items
        .iter()
        .map(|review| review.record.slug.clone())
        .collect::<Vec<_>>();
    let data_root = state.data_root.clone();
    let published_slugs = run_blocking(move || {
        let mut published = BTreeSet::new();
        for slug in candidate_slugs {
            if get_material_detail(data_root.as_path(), &slug)?
                .is_some_and(|record| record.record_type == "mineral")
            {
                published.insert(slug);
            }
        }
        Ok(published)
    })
    .await?;
    let reviews = page
        .items
        .into_iter()
        .map(|review| {
            let is_update = published_slugs.contains(&review.record.slug);
            admin_review_candidate_for_ui(
                review,
                language,
                is_update,
                state.public_catalog_base_url.as_deref(),
            )
        })
        .collect();

    Ok(TemplateResponse(ReviewQueueTemplate {
        lang_code: language.code().to_string(),
        lang_dir: language.dir().to_string(),
        txt: ui_text(language),
        public_catalog_url: public_catalog_minerals_url(&state),
        has_admin_session: true,
        error_message: None,
        success_message,
        reviews,
        total_results,
        current_page,
        total_pages,
        page_start,
        page_end,
        has_previous_page: current_page > 1,
        previous_page: current_page.saturating_sub(1).max(1),
        has_next_page: total_results > 0 && current_page < total_pages,
        next_page: current_page.saturating_add(1).min(total_pages),
    }))
}

async fn admin_review_mineral(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(request): Form<AdminReviewRequest>,
) -> Result<Redirect, AppError> {
    require_admin_session(&state, &headers)?;
    require_same_origin(&headers)?;
    if request.review_id <= 0 {
        return Err(AppError::BadRequest(
            "review_id must identify a pending mineral revision".to_string(),
        ));
    }
    let operator_note = required_string_limited(&request.operator_note, "operator_note", 2_000)?;
    let approve = match request.action.trim().to_ascii_lowercase().as_str() {
        "approve" => true,
        "reject" => false,
        _ => {
            return Err(AppError::BadRequest(
                "review action must be approve or reject".to_string(),
            ))
        }
    };

    let data_root = state.data_root.clone();
    let reviewer_id = state.admin_reviewer_id.as_str().to_string();
    let review_id = request.review_id;
    let outcome = run_blocking(move || {
        if approve {
            approve_mineral_review(data_root.as_path(), review_id, &reviewer_id, &operator_note)
        } else {
            reject_mineral_review(data_root.as_path(), review_id, &reviewer_id, &operator_note)
        }
    })
    .await
    .map_err(|err| {
        warn!(review_id, error = %err, "mineral review decision failed");
        AppError::BadRequest(
            "This mineral revision could not be decided. Refresh the review queue and try again."
                .to_string(),
        )
    })?;

    info!(
        review_id = outcome.review_id,
        mineral_slug = %outcome.mineral_slug,
        status = ?outcome.status,
        changed = outcome.changed,
        "mineral review decision recorded"
    );
    let notice = if approve { "approved" } else { "rejected" };
    Ok(Redirect::to(&format!("/admin/reviews?notice={notice}")))
}

async fn admin_login(
    State(state): State<AppState>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Form(request): Form<AdminLoginRequest>,
) -> Result<Response, AppError> {
    let language = resolve_language(&state, &headers);
    require_same_origin(&headers)?;
    let client_ip = client_ip_for_rate_limit(&state, peer_addr.ip(), &headers);
    // Once an address exhausts its initial burst, reserve at most one password
    // comparison per interval. This limits brute-force guesses without making
    // a correct password permanently unusable after an attacker fills a
    // failure bucket.
    {
        let mut failures_by_ip = state
            .admin_login_failures
            .lock()
            .map_err(|_| anyhow!("admin login failure store lock poisoned"))?;
        failures_by_ip.retain(|_, failures| {
            failures.retain(|created_at| created_at.elapsed().as_secs() < 60);
            !failures.is_empty()
        });
        let failures = failures_by_ip.entry(client_ip).or_default();
        if failures.len() >= ADMIN_LOGIN_BURST
            && failures.last().is_some_and(|attempt| {
                attempt.elapsed().as_secs() < ADMIN_LOGIN_THROTTLED_INTERVAL_SECS
            })
        {
            return Err(AppError::TooManyRequests(
                "too many failed login attempts from this address; retry in a few seconds"
                    .to_string(),
            ));
        }
        // Reserve before comparing so a parallel burst cannot race past the
        // limit. A successful login clears its address bucket below.
        failures.push(Instant::now());
    }

    if !constant_time_eq(request.password.as_bytes(), state.admin_password.as_bytes()) {
        return Ok(TemplateResponse(AdminTemplate {
            lang_code: language.code().to_string(),
            lang_dir: language.dir().to_string(),
            txt: ui_text(language),
            public_catalog_url: public_catalog_minerals_url(&state),
            has_admin_session: false,
            error_message: Some("Invalid admin password.".to_string()),
            success_message: None,
            draft_form: MineralFormData::default(),
            has_suggestion: false,
        })
        .into_response());
    }
    state
        .admin_login_failures
        .lock()
        .map_err(|_| anyhow!("admin login failure store lock poisoned"))?
        .remove(&client_ip);

    let token = generate_secure_hex(24)?;
    {
        let mut sessions = state
            .admin_sessions
            .lock()
            .map_err(|_| anyhow!("admin session store lock poisoned"))?;
        sessions.insert(token.clone(), Instant::now());
    }

    let mut response = TemplateResponse(AdminTemplate {
        lang_code: language.code().to_string(),
        lang_dir: language.dir().to_string(),
        txt: ui_text(language),
        public_catalog_url: public_catalog_minerals_url(&state),
        has_admin_session: true,
        error_message: None,
        success_message: Some("Admin session created.".to_string()),
        draft_form: MineralFormData::default(),
        has_suggestion: false,
    })
    .into_response();

    let cookie = format!(
        "admin_session={token}; HttpOnly; Path=/; SameSite=Strict; Max-Age=28800{}",
        if state.secure_cookies { "; Secure" } else { "" }
    );
    append_set_cookie(&mut response, &cookie)?;
    Ok(response)
}

async fn admin_logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let language = resolve_language(&state, &headers);
    require_same_origin(&headers)?;
    if let Some(token) = admin_token_from_headers(&headers) {
        {
            let mut sessions = state
                .admin_sessions
                .lock()
                .map_err(|_| anyhow!("admin session store lock poisoned"))?;
            sessions.remove(&token);
        }
        {
            let mut drafts = state
                .admin_drafts
                .lock()
                .map_err(|_| anyhow!("admin draft store lock poisoned"))?;
            drafts.retain(|_, draft| draft.owner_session != token);
        }
    }

    let mut response = TemplateResponse(AdminTemplate {
        lang_code: language.code().to_string(),
        lang_dir: language.dir().to_string(),
        txt: ui_text(language),
        public_catalog_url: public_catalog_minerals_url(&state),
        has_admin_session: false,
        error_message: None,
        success_message: Some("Admin session closed.".to_string()),
        draft_form: MineralFormData::default(),
        has_suggestion: false,
    })
    .into_response();

    append_set_cookie(
        &mut response,
        &format!(
            "admin_session=; HttpOnly; Path=/; SameSite=Strict; Max-Age=0{}",
            if state.secure_cookies { "; Secure" } else { "" }
        ),
    )?;
    Ok(response)
}

async fn admin_suggest_mineral(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<TemplateResponse<AdminTemplate>, AppError> {
    let language = resolve_language(&state, &headers);
    let admin_token = require_admin_session(&state, &headers)?;
    require_same_origin(&headers)?;

    let input = parse_suggest_multipart(&mut multipart).await?;

    let suggestion = match request_openai_suggestion(&state, &input).await {
        Ok(suggestion) => suggestion,
        Err(err) => {
            error!("admin ai suggestion failed: {err}");
            return Ok(TemplateResponse(AdminTemplate {
                lang_code: language.code().to_string(),
                lang_dir: language.dir().to_string(),
                txt: ui_text(language),
                public_catalog_url: public_catalog_minerals_url(&state),
                has_admin_session: true,
                error_message: Some(format!("AI suggestion failed: {err}")),
                success_message: None,
                draft_form: MineralFormData {
                    suggestion_context: input.suggestion_context,
                    ..MineralFormData::default()
                },
                has_suggestion: false,
            }));
        }
    };

    let preview_image_data_url = format!(
        "data:{};base64,{}",
        content_type_from_ext(&input.image_ext),
        BASE64.encode(&input.image_bytes)
    );

    let draft_id = generate_secure_hex(12)?;
    {
        let mut drafts = state
            .admin_drafts
            .lock()
            .map_err(|_| anyhow!("admin draft store lock poisoned"))?;
        drafts.retain(|_, draft| draft.created_at.elapsed().as_secs() < 1800);
        if drafts.len() >= 32 {
            return Err(AppError::BadRequest(
                "too many pending admin drafts; publish, discard, or wait for expiry".to_string(),
            ));
        }
        drafts.insert(
            draft_id.clone(),
            AdminDraft {
                image_bytes: input.image_bytes,
                image_ext: input.image_ext,
                owner_session: admin_token,
                created_at: Instant::now(),
            },
        );
    }

    let form = MineralFormData {
        draft_id: Some(draft_id),
        common_name: suggestion.common_name,
        description: suggestion.description,
        suggestion_context: input.suggestion_context,
        preview_image_data_url,
        mineral_family: suggestion.mineral_family,
        formula: suggestion.formula,
        hardness_mohs: format!("{:.2}", suggestion.hardness_mohs),
        density_g_cm3: format!("{:.2}", suggestion.density_g_cm3),
        crystal_system: suggestion.crystal_system,
        color: suggestion.color,
        streak: suggestion.streak,
        luster: suggestion.luster,
        major_elements_pct_text: major_elements_to_text(&ai_major_elements_to_map(
            suggestion.major_elements,
        )),
        notes: suggestion.notes,
    };

    Ok(TemplateResponse(AdminTemplate {
        lang_code: language.code().to_string(),
        lang_dir: language.dir().to_string(),
        txt: ui_text(language),
        public_catalog_url: public_catalog_minerals_url(&state),
        has_admin_session: true,
        error_message: None,
        success_message: Some("AI suggestion generated. Review and publish.".to_string()),
        draft_form: form,
        has_suggestion: true,
    }))
}

async fn admin_publish_mineral(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(request): Form<PublishMineralRequest>,
) -> Result<TemplateResponse<AdminTemplate>, AppError> {
    let language = resolve_language(&state, &headers);
    let admin_token = require_admin_session(&state, &headers)?;
    require_same_origin(&headers)?;

    let image_draft = {
        let mut drafts = state
            .admin_drafts
            .lock()
            .map_err(|_| anyhow!("admin draft store lock poisoned"))?;
        drafts.retain(|_, draft| draft.created_at.elapsed().as_secs() < 1800);
        let draft = drafts.remove(&request.draft_id).ok_or_else(|| {
            AppError::BadRequest("draft session not found; run AI suggestion again".to_string())
        })?;
        if draft.owner_session != admin_token {
            drafts.insert(request.draft_id.clone(), draft);
            return Err(AppError::Unauthorized(
                "draft belongs to a different admin session".to_string(),
            ));
        }
        draft
    };

    let form = MineralFormData {
        draft_id: Some(request.draft_id.clone()),
        common_name: request.common_name.clone(),
        description: request.description.clone(),
        suggestion_context: String::new(),
        preview_image_data_url: format!(
            "data:{};base64,{}",
            content_type_from_ext(&image_draft.image_ext),
            BASE64.encode(&image_draft.image_bytes)
        ),
        mineral_family: request.mineral_family.clone(),
        formula: request.formula.clone(),
        hardness_mohs: request.hardness_mohs.clone(),
        density_g_cm3: request.density_g_cm3.clone(),
        crystal_system: request.crystal_system.clone(),
        color: request.color.clone(),
        streak: request.streak.clone(),
        luster: request.luster.clone(),
        major_elements_pct_text: request.major_elements_pct_text.clone(),
        notes: request.notes.clone(),
    };

    let parsed_draft = match parse_publish_request(&request, &image_draft) {
        Ok(value) => value,
        Err(err) => {
            state
                .admin_drafts
                .lock()
                .map_err(|_| anyhow!("admin draft store lock poisoned"))?
                .insert(request.draft_id.clone(), image_draft);
            return Ok(TemplateResponse(AdminTemplate {
                lang_code: language.code().to_string(),
                lang_dir: language.dir().to_string(),
                txt: ui_text(language),
                public_catalog_url: public_catalog_minerals_url(&state),
                has_admin_session: true,
                error_message: Some(err.to_string()),
                success_message: None,
                draft_form: form,
                has_suggestion: true,
            }));
        }
    };

    let (slug, translation_stats) = match create_mineral_record(&state, parsed_draft).await {
        Ok(result) => result,
        Err(err) => {
            state
                .admin_drafts
                .lock()
                .map_err(|_| anyhow!("admin draft store lock poisoned"))?
                .insert(request.draft_id.clone(), image_draft);
            return Err(err);
        }
    };
    reload_catalog(&state)?;

    let mut success_message = format!(
        "Mineral published: {}. Localized records: {} translated.",
        slug, translation_stats.translated_count
    );
    if !translation_stats.fallback_lang_codes.is_empty() {
        success_message.push_str(" Fallback used for: ");
        success_message.push_str(&translation_stats.fallback_lang_codes.join(", "));
    }

    Ok(TemplateResponse(AdminTemplate {
        lang_code: language.code().to_string(),
        lang_dir: language.dir().to_string(),
        txt: ui_text(language),
        public_catalog_url: public_catalog_minerals_url(&state),
        has_admin_session: true,
        error_message: None,
        success_message: Some(success_message),
        draft_form: MineralFormData::default(),
        has_suggestion: false,
    }))
}

async fn admin_delete_mineral(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(request): Form<DeleteMineralRequest>,
) -> Result<TemplateResponse<AdminTemplate>, AppError> {
    let language = resolve_language(&state, &headers);
    require_admin_session(&state, &headers)?;
    require_same_origin(&headers)?;

    let slug = required_string(&request.slug, "slug")?;
    let mineral = match catalog_for_language(&state, language)?
        .by_slug
        .get(&slug)
        .cloned()
    {
        Some(value) => value,
        None => {
            return Ok(TemplateResponse(AdminTemplate {
                lang_code: language.code().to_string(),
                lang_dir: language.dir().to_string(),
                txt: ui_text(language),
                public_catalog_url: public_catalog_minerals_url(&state),
                has_admin_session: true,
                error_message: Some(format!("mineral '{slug}' not found")),
                success_message: None,
                draft_form: MineralFormData::default(),
                has_suggestion: false,
            }));
        }
    };
    let folder_name = mineral.folder_name;

    if !is_valid_mineral_folder_name(&folder_name) {
        return Err(AppError::Internal(anyhow!(
            "stored mineral artifact folder is invalid"
        )));
    }

    let data_root = state.data_root.clone();
    let withdraw_slug = slug.clone();
    let reviewer_id = state.admin_reviewer_id.as_str().to_string();
    run_blocking(move || {
        withdraw_mineral(
            data_root.as_path(),
            &withdraw_slug,
            &reviewer_id,
            "Withdrawn by the legacy catalog delete action.",
        )
    })
    .await?;
    let data_root = state.data_root.clone();
    let delete_slug = slug.clone();
    run_blocking(move || delete_mineral_records(data_root.as_path(), &delete_slug)).await?;
    reload_catalog(&state)?;
    let folder_path = state.data_root.join("minerals").join(&folder_name);
    let mut cleanup_warnings = Vec::new();
    let removed_directory = match fs::metadata(&folder_path).await {
        Ok(metadata) if metadata.is_dir() => match fs::remove_dir_all(&folder_path).await {
            Ok(()) => true,
            Err(err) => {
                cleanup_warnings.push(format!("{}: {err}", folder_path.display()));
                false
            }
        },
        Ok(_) => {
            cleanup_warnings.push(format!("{} is not a directory", folder_path.display()));
            false
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
        Err(err) => {
            cleanup_warnings.push(format!("{}: {err}", folder_path.display()));
            false
        }
    };
    let mut success_message = format!(
        "Mineral deleted: {slug}. Metadata committed; image directory removed: {removed_directory}."
    );
    if !cleanup_warnings.is_empty() {
        success_message.push_str(" Cleanup will need a retry: ");
        success_message.push_str(&cleanup_warnings.join("; "));
    }

    Ok(TemplateResponse(AdminTemplate {
        lang_code: language.code().to_string(),
        lang_dir: language.dir().to_string(),
        txt: ui_text(language),
        public_catalog_url: public_catalog_minerals_url(&state),
        has_admin_session: true,
        error_message: None,
        success_message: Some(success_message),
        draft_form: MineralFormData::default(),
        has_suggestion: false,
    }))
}

async fn admin_withdraw_mineral(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(request): Form<WithdrawMineralRequest>,
) -> Result<TemplateResponse<AdminTemplate>, AppError> {
    let language = resolve_language(&state, &headers);
    require_admin_session(&state, &headers)?;
    require_same_origin(&headers)?;
    let slug = required_string_limited(&request.slug, "slug", 240)?;
    let operator_note = required_string_limited(&request.operator_note, "operator_note", 2_000)?;
    let data_root = state.data_root.clone();
    let withdraw_slug = slug.clone();
    let reviewer_id = state.admin_reviewer_id.as_str().to_string();
    let changed = run_blocking(move || {
        withdraw_mineral(
            data_root.as_path(),
            &withdraw_slug,
            &reviewer_id,
            &operator_note,
        )
    })
    .await
    .map_err(|err| {
        warn!(mineral_slug = %slug, error = %err, "mineral withdrawal failed");
        AppError::BadRequest(
            "This mineral could not be withdrawn. Check the slug and try again.".to_string(),
        )
    })?;

    Ok(TemplateResponse(AdminTemplate {
        lang_code: language.code().to_string(),
        lang_dir: language.dir().to_string(),
        txt: ui_text(language),
        public_catalog_url: public_catalog_minerals_url(&state),
        has_admin_session: true,
        error_message: None,
        success_message: Some(if changed {
            format!("Mineral withdrawn from public view: {slug}.")
        } else {
            format!("Mineral was already withdrawn: {slug}.")
        }),
        draft_form: MineralFormData::default(),
        has_suggestion: false,
    }))
}

async fn admin_db_query(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AdminDbQueryRequest>,
) -> Result<Json<AdminDbQueryResponse>, AppError> {
    require_admin_session(&state, &headers)?;
    require_same_origin(&headers)?;
    if !state.admin_sql_enabled {
        return Err(AppError::NotFound(
            "admin SQL console is disabled".to_string(),
        ));
    }

    let sql = required_string(&request.sql, "sql")?;
    let data_root = state.data_root.clone();
    let execution = run_blocking(move || execute_admin_sql(data_root.as_path(), &sql))
        .await
        .map_err(|err| AppError::BadRequest(err.to_string()))?;

    let message = if execution.columns.is_empty() {
        reload_catalog(&state)?;
        format!(
            "Statement executed. {} row(s) affected.",
            execution.affected_rows
        )
    } else if execution.truncated {
        format!(
            "Query executed. Showing {} row(s) (truncated to server limit).",
            execution.row_count
        )
    } else {
        format!("Query executed. {} row(s) returned.", execution.row_count)
    };

    Ok(Json(AdminDbQueryResponse {
        statement_type: execution.statement_type,
        columns: execution.columns,
        rows: execution.rows,
        row_count: execution.row_count,
        affected_rows: execution.affected_rows,
        truncated: execution.truncated,
        message,
    }))
}

async fn admin_import_minerals(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<MineralImportPayload>,
) -> Result<Json<registry::ImportSummary>, AppError> {
    require_admin_session(&state, &headers)?;
    require_same_origin(&headers)?;

    let records = match payload {
        MineralImportPayload::One(record) => vec![*record],
        MineralImportPayload::Many(records) => records,
    };
    if records
        .iter()
        .any(|record| record.record_type.trim() != "mineral")
    {
        return Err(AppError::BadRequest(
            "only mineral records are accepted; compounds are outside the current scope"
                .to_string(),
        ));
    }
    let data_root = state.data_root.clone();
    let summary =
        run_blocking(move || import_material_batch(data_root.as_path(), "admin_api", &records))
            .await
            .map_err(|err| AppError::BadRequest(err.to_string()))?;
    Ok(Json(summary))
}

async fn admin_import_provider(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(provider): Json<ProviderImport>,
) -> Result<Json<registry::ProviderImportSummary>, AppError> {
    require_admin_session(&state, &headers)?;
    require_same_origin(&headers)?;
    let data_root = state.data_root.clone();
    let summary = run_blocking(move || {
        for offer in &provider.offers {
            if let Some(record) = get_material_detail(data_root.as_path(), &offer.material_slug)? {
                if record.record_type != "mineral" {
                    return Err(anyhow!(
                        "provider offers may reference mineral records only; '{}' is outside the current scope",
                        offer.material_slug
                    ));
                }
            }
        }
        import_provider(data_root.as_path(), &provider)
    })
        .await
        .map_err(|err| AppError::BadRequest(err.to_string()))?;
    Ok(Json(summary))
}

fn parse_publish_request(
    request: &PublishMineralRequest,
    image: &AdminDraft,
) -> Result<NewMineralDraft, AppError> {
    let common_name = required_string_limited(&request.common_name, "common_name", 200)?;
    let description = required_string_limited(&request.description, "description", 20_000)?;
    let mineral_family = required_string_limited(&request.mineral_family, "mineral_family", 120)?;
    let formula = required_string_limited(&request.formula, "formula", 240)?;
    let crystal_system = required_string_limited(&request.crystal_system, "crystal_system", 120)?;
    let color = required_string_limited(&request.color, "color", 240)?;
    let streak = required_string_limited(&request.streak, "streak", 120)?;
    let luster = required_string_limited(&request.luster, "luster", 120)?;
    let notes = required_string_limited(&request.notes, "notes", 20_000)?;

    let hardness_mohs = parse_f32_from_str(&request.hardness_mohs, "hardness_mohs")?;
    let density_g_cm3 = parse_f32_from_str(&request.density_g_cm3, "density_g_cm3")?;
    if !(0.0..=10.0).contains(&hardness_mohs) {
        return Err(AppError::BadRequest(
            "'hardness_mohs' must be between 0 and 10".to_string(),
        ));
    }
    if density_g_cm3 <= 0.0 || density_g_cm3 > 30.0 {
        return Err(AppError::BadRequest(
            "'density_g_cm3' must be greater than 0 and no more than 30".to_string(),
        ));
    }
    let major_elements_pct =
        parse_major_elements(&request.major_elements_pct_text).map_err(AppError::BadRequest)?;

    Ok(NewMineralDraft {
        common_name,
        description,
        mineral_family,
        formula,
        hardness_mohs,
        density_g_cm3,
        crystal_system,
        color,
        streak,
        luster,
        notes,
        major_elements_pct,
        image_bytes: image.image_bytes.clone(),
        image_ext: image.image_ext.clone(),
    })
}

async fn parse_suggest_multipart(multipart: &mut Multipart) -> Result<SuggestInput, AppError> {
    let mut suggestion_context = String::new();
    let mut image_bytes: Option<Vec<u8>> = None;
    let mut image_ext: Option<String> = None;

    while let Some(field) = multipart.next_field().await.map_err(|err| {
        let message = err.to_string();
        if is_request_too_large_error(&message) {
            AppError::BadRequest(format!(
                "image upload too large; keep file under {ADMIN_UPLOAD_MAX_MB} MB"
            ))
        } else {
            AppError::BadRequest(format!("invalid multipart payload: {message}"))
        }
    })? {
        let name = field.name().unwrap_or_default().to_string();
        if name.is_empty() {
            continue;
        }

        if name == "image" {
            let ext = detect_image_extension(&field)?;
            let bytes = field.bytes().await.map_err(|err| {
                let message = err.to_string();
                if is_request_too_large_error(&message) {
                    AppError::BadRequest(format!(
                        "image upload too large; keep file under {ADMIN_UPLOAD_MAX_MB} MB"
                    ))
                } else {
                    AppError::BadRequest(format!("failed to read image field: {message}"))
                }
            })?;
            if bytes.is_empty() {
                return Err(AppError::BadRequest("image upload is required".to_string()));
            }
            if bytes.len() > ADMIN_UPLOAD_MAX_BYTES {
                return Err(AppError::BadRequest(format!(
                    "image upload too large; keep file under {ADMIN_UPLOAD_MAX_MB} MB"
                )));
            }
            validate_image_signature(&bytes, &ext)?;
            image_ext = Some(ext);
            image_bytes = Some(bytes.to_vec());
            continue;
        }

        let value = field
            .text()
            .await
            .map_err(|err| AppError::BadRequest(format!("failed to read field '{name}': {err}")))?;

        if name == "suggestion_context" {
            let trimmed = value.trim();
            if trimmed.chars().count() > 4_000 {
                return Err(AppError::BadRequest(
                    "suggestion_context must not exceed 4000 characters".to_string(),
                ));
            }
            suggestion_context = trimmed.to_string();
        }
    }

    Ok(SuggestInput {
        suggestion_context,
        image_bytes: image_bytes
            .ok_or_else(|| AppError::BadRequest("image upload is required".to_string()))?,
        image_ext: image_ext
            .ok_or_else(|| AppError::BadRequest("unable to determine image format".to_string()))?,
    })
}

fn is_request_too_large_error(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("body too large")
        || normalized.contains("maximum size")
        || normalized.contains("length limit")
}

async fn request_openai_suggestion(
    state: &AppState,
    input: &SuggestInput,
) -> Result<AiMineralSuggestion, AppError> {
    let api_key = state.openai_api_key.as_ref().as_ref().ok_or_else(|| {
        AppError::BadRequest(
            "OPENAI_API_KEY is not configured in the process environment".to_string(),
        )
    })?;

    let image_data_url = format!(
        "data:{};base64,{}",
        content_type_from_ext(&input.image_ext),
        BASE64.encode(&input.image_bytes)
    );

    let schema = serde_json::json!({
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "common_name": {"type": "string"},
        "description": {"type": "string"},
        "mineral_family": {"type": "string"},
        "formula": {"type": "string"},
        "hardness_mohs": {"type": "number"},
        "density_g_cm3": {"type": "number"},
        "crystal_system": {"type": "string"},
        "color": {"type": "string"},
        "streak": {"type": "string"},
        "luster": {"type": "string"},
        "major_elements": {
          "type": "array",
          "items": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
              "element": {"type": "string"},
              "percent": {"type": "number"}
            },
            "required": ["element", "percent"]
          }
        },
        "notes": {"type": "string"}
      },
      "required": [
        "mineral_family",
        "common_name",
        "description",
        "formula",
        "hardness_mohs",
        "density_g_cm3",
        "crystal_system",
        "color",
        "streak",
        "luster",
        "major_elements",
        "notes"
      ]
    });

    let system_prompt = "You assist mineral cataloging. Use the provided photo (and optional operator context) to infer likely mineral properties. Generate a plausible common_name and a concise description. If uncertain, provide conservative estimates and practical values. Output must follow JSON schema exactly.";

    let user_prompt = format!(
        "User context (may be empty): {}\n\nGenerate a likely mineral profile from the image. The common_name and description must be generated too.",
        input.suggestion_context
    );

    let request = ChatCompletionsRequest {
        model: (*state.openai_model).clone(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: vec![MessagePart::Text {
                    text: system_prompt.to_string(),
                }],
            },
            ChatMessage {
                role: "user".to_string(),
                content: vec![
                    MessagePart::Text { text: user_prompt },
                    MessagePart::ImageUrl {
                        image_url: ImageUrlContent {
                            url: image_data_url,
                        },
                    },
                ],
            },
        ],
        response_format: ResponseFormat {
            kind: "json_schema".to_string(),
            json_schema: JsonSchemaSpec {
                name: "mineral_suggestion".to_string(),
                strict: true,
                schema,
            },
        },
        temperature: 0.2,
    };

    let response = state
        .http_client
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(api_key)
        .json(&request)
        .send()
        .await
        .map_err(|err| AppError::BadRequest(format!("failed to call OpenAI API: {err}")))?;

    if !response.status().is_success() {
        let status = response.status();
        error!("openai api error status={status}");
        return Err(AppError::BadRequest(format!(
            "OpenAI API returned {status}; inspect server logs for the request id"
        )));
    }

    let parsed: ChatCompletionsResponse = response
        .json()
        .await
        .map_err(|err| AppError::BadRequest(format!("failed to parse OpenAI response: {err}")))?;

    let content = parsed
        .choices
        .first()
        .map(|choice| choice.message.content.as_str())
        .ok_or_else(|| AppError::BadRequest("OpenAI response had no choices".to_string()))?;

    serde_json::from_str::<AiMineralSuggestion>(content)
        .map_err(|err| AppError::BadRequest(format!("invalid AI JSON payload: {err}")))
}

fn ai_major_elements_to_map(input: Vec<AiMajorElement>) -> BTreeMap<String, f32> {
    let mut out = BTreeMap::new();
    for item in input {
        let name = item.element.trim();
        if name.is_empty() {
            continue;
        }
        out.insert(name.to_string(), item.percent);
    }
    out
}

fn catalog_for_language(state: &AppState, language: Language) -> Result<MineralCatalog, AppError> {
    let code = language.code().to_string();

    if let Some(cached) = state
        .catalogs_by_lang
        .read()
        .map_err(|_| anyhow!("catalog cache lock poisoned"))?
        .get(&code)
        .cloned()
    {
        return Ok(cached);
    }

    let loaded = MineralCatalog::new(load_minerals(state.data_root.as_path(), language.code())?);
    let mut guard = state
        .catalogs_by_lang
        .write()
        .map_err(|_| anyhow!("catalog cache lock poisoned"))?;
    if let Some(cached) = guard.get(&code).cloned() {
        return Ok(cached);
    }
    guard.insert(code, loaded.clone());
    Ok(loaded)
}

fn reload_catalog(state: &AppState) -> Result<()> {
    let mut guard = state
        .catalogs_by_lang
        .write()
        .map_err(|_| anyhow!("catalog lock poisoned"))?;
    guard.clear();
    Ok(())
}

fn has_admin_session(state: &AppState, headers: &HeaderMap) -> bool {
    let Some(token) = admin_token_from_headers(headers) else {
        return false;
    };

    state
        .admin_sessions
        .lock()
        .ok()
        .map(|mut sessions| {
            sessions.retain(|_, created_at| created_at.elapsed().as_secs() < 28_800);
            sessions.contains_key(&token)
        })
        .unwrap_or(false)
}

fn require_admin_session(state: &AppState, headers: &HeaderMap) -> Result<String, AppError> {
    let token = admin_token_from_headers(headers).ok_or_else(|| {
        AppError::Unauthorized("Admin session required. Log in at /admin.".to_string())
    })?;
    let valid = state
        .admin_sessions
        .lock()
        .ok()
        .map(|mut sessions| {
            sessions.retain(|_, created_at| created_at.elapsed().as_secs() < 28_800);
            sessions.contains_key(&token)
        })
        .unwrap_or(false);
    if valid {
        Ok(token)
    } else {
        Err(AppError::Unauthorized(
            "Admin session required. Log in at /admin.".to_string(),
        ))
    }
}

fn require_same_origin(headers: &HeaderMap) -> Result<(), AppError> {
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| AppError::BadRequest("Host header is required".to_string()))?;
    let allowed_http = format!("http://{host}");
    let allowed_https = format!("https://{host}");

    if let Some(origin) = headers.get(header::ORIGIN) {
        let origin = origin
            .to_str()
            .map_err(|_| AppError::BadRequest("invalid Origin header".to_string()))?;
        if origin == allowed_http || origin == allowed_https {
            return Ok(());
        }
        return Err(AppError::Unauthorized(
            "cross-origin admin request rejected".to_string(),
        ));
    }

    if let Some(referer) = headers.get(header::REFERER) {
        let referer = referer
            .to_str()
            .map_err(|_| AppError::BadRequest("invalid Referer header".to_string()))?;
        let same_http = referer == allowed_http
            || referer
                .strip_prefix(&allowed_http)
                .is_some_and(|suffix| suffix.starts_with('/'));
        let same_https = referer == allowed_https
            || referer
                .strip_prefix(&allowed_https)
                .is_some_and(|suffix| suffix.starts_with('/'));
        if same_http || same_https {
            return Ok(());
        }
    }

    Err(AppError::Unauthorized(
        "same-origin admin request evidence is required".to_string(),
    ))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let key = ring::hmac::Key::new(
        ring::hmac::HMAC_SHA256,
        b"waajacu-minerals.constant-time-equality.v1",
    );
    let expected = ring::hmac::sign(&key, left);
    ring::hmac::verify(&key, right, expected.as_ref()).is_ok()
}

fn bearer_token_from_headers(headers: &HeaderMap) -> Result<Option<&str>, AppError> {
    if headers.get_all(header::AUTHORIZATION).iter().count() > 1 {
        return Err(AppError::Unauthorized(
            "valid ingestion credentials are required".to_string(),
        ));
    }
    let Some(value) = headers.get(header::AUTHORIZATION) else {
        return Ok(None);
    };
    let value = value.to_str().map_err(|_| {
        AppError::Unauthorized("valid ingestion credentials are required".to_string())
    })?;
    let Some((scheme, token)) = value.split_once(' ') else {
        return Err(AppError::Unauthorized(
            "valid ingestion credentials are required".to_string(),
        ));
    };
    if !scheme.eq_ignore_ascii_case("bearer")
        || token.is_empty()
        || token.chars().any(char::is_whitespace)
    {
        return Err(AppError::Unauthorized(
            "valid ingestion credentials are required".to_string(),
        ));
    }
    Ok(Some(token))
}

fn authenticate_ingestion_bearer(
    headers: &HeaderMap,
    configured_token: Option<&str>,
    adapter_id: &str,
) -> Result<Option<IngestionActor>, AppError> {
    let Some(presented_token) = bearer_token_from_headers(headers)? else {
        return Ok(None);
    };
    let valid = configured_token
        .map(|expected| constant_time_eq(presented_token.as_bytes(), expected.as_bytes()))
        .unwrap_or(false);
    if !valid {
        return Err(AppError::Unauthorized(
            "valid ingestion credentials are required".to_string(),
        ));
    }
    Ok(Some(IngestionActor {
        id: adapter_id.to_string(),
        kind: IngestionAuthKind::Adapter,
    }))
}

fn require_ingestion_writer_actor(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<IngestionActor, AppError> {
    if let Some(actor) = authenticate_ingestion_bearer(
        headers,
        state.ingestion_api_token.as_deref(),
        state.ingestion_adapter_id.as_str(),
    )? {
        return Ok(actor);
    }

    require_admin_session(state, headers)?;
    require_same_origin(headers)?;
    Ok(IngestionActor {
        id: state.admin_reviewer_id.as_str().to_string(),
        kind: IngestionAuthKind::Admin,
    })
}

fn require_ingestion_reader_actor(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<IngestionActor, AppError> {
    if let Some(actor) = authenticate_ingestion_bearer(
        headers,
        state.ingestion_api_token.as_deref(),
        state.ingestion_adapter_id.as_str(),
    )? {
        return Ok(actor);
    }

    require_admin_session(state, headers)?;
    Ok(IngestionActor {
        id: state.admin_reviewer_id.as_str().to_string(),
        kind: IngestionAuthKind::Admin,
    })
}

fn require_ingestion_reviewer_actor(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<String, AppError> {
    require_admin_session(state, headers)?;
    require_same_origin(headers)?;
    Ok(state.admin_reviewer_id.as_str().to_string())
}

fn admin_token_from_headers(headers: &HeaderMap) -> Option<String> {
    cookie_value(headers, "admin_session")
}

fn resolve_language(state: &AppState, headers: &HeaderMap) -> Language {
    cookie_value(headers, "lang")
        .and_then(|raw| Language::from_code(&raw))
        .unwrap_or(state.default_language)
}

fn cookie_value(headers: &HeaderMap, key: &str) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;

    for cookie in cookie_header.split(';') {
        let trimmed = cookie.trim();
        let Some((name, value)) = trimmed.split_once('=') else {
            continue;
        };
        if name == key && !value.is_empty() {
            return Some(value.to_string());
        }
    }

    None
}

fn append_set_cookie(response: &mut Response, cookie: &str) -> Result<(), AppError> {
    let value = HeaderValue::from_str(cookie)
        .map_err(|_| AppError::Internal(anyhow!("invalid set-cookie header value")))?;
    response.headers_mut().append(header::SET_COOKIE, value);
    Ok(())
}

fn required_string(value: &str, key: &str) -> Result<String, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest(format!("'{key}' is required")));
    }
    Ok(trimmed.to_string())
}

fn required_string_limited(value: &str, key: &str, max_chars: usize) -> Result<String, AppError> {
    let value = required_string(value, key)?;
    if value.chars().count() > max_chars {
        return Err(AppError::BadRequest(format!(
            "'{key}' must not exceed {max_chars} characters"
        )));
    }
    Ok(value)
}

fn parse_f32_from_str(value: &str, key: &str) -> Result<f32, AppError> {
    let value = required_string(value, key)?;
    let parsed = value
        .parse::<f32>()
        .map_err(|_| AppError::BadRequest(format!("'{key}' must be a number")))?;
    if !parsed.is_finite() {
        return Err(AppError::BadRequest(format!("'{key}' must be finite")));
    }
    Ok(parsed)
}

fn detect_image_extension(field: &axum::extract::multipart::Field<'_>) -> Result<String, AppError> {
    if let Some(file_name) = field.file_name() {
        if let Some(ext) = file_name.rsplit('.').next() {
            let normalized = ext.to_ascii_lowercase();
            if ["heic", "heif"].contains(&normalized.as_str()) {
                return Err(AppError::BadRequest(
                    "HEIC/HEIF photos are not supported yet; upload png, jpg, webp, or gif"
                        .to_string(),
                ));
            }
            if ["png", "jpg", "jpeg", "webp", "gif"].contains(&normalized.as_str()) {
                return Ok(if normalized == "jpeg" {
                    "jpg".to_string()
                } else {
                    normalized
                });
            }
        }
    }

    if let Some(content_type) = field.content_type() {
        return match content_type {
            "image/png" => Ok("png".to_string()),
            "image/jpeg" => Ok("jpg".to_string()),
            "image/jpg" => Ok("jpg".to_string()),
            "image/webp" => Ok("webp".to_string()),
            "image/gif" => Ok("gif".to_string()),
            "image/heic" | "image/heif" => Err(AppError::BadRequest(
                "HEIC/HEIF photos are not supported yet; upload png, jpg, webp, or gif".to_string(),
            )),
            _ => Err(AppError::BadRequest(
                "unsupported image type; use png, jpg, webp, or gif".to_string(),
            )),
        };
    }

    Err(AppError::BadRequest(
        "unsupported image type; use png, jpg, webp, or gif".to_string(),
    ))
}

fn content_type_from_ext(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "image/jpeg",
    }
}

fn validate_image_signature(bytes: &[u8], ext: &str) -> Result<(), AppError> {
    let valid = match ext {
        "png" => bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]),
        "jpg" => bytes.starts_with(&[0xff, 0xd8, 0xff]),
        "gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        "webp" => bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP",
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!(
            "uploaded bytes do not match the declared {ext} image format"
        )))
    }
}

async fn create_mineral_record(
    state: &AppState,
    draft: NewMineralDraft,
) -> Result<(String, TranslationStats), AppError> {
    let family_slug = slugify_family(&draft.mineral_family);

    let slug = create_unique_slug(state, &family_slug)?;
    if !is_valid_mineral_folder_name(&slug) {
        return Err(AppError::Internal(anyhow!(
            "generated invalid mineral slug: {slug}"
        )));
    }

    let folder_name = slug.clone();
    let image_file = format!("source.{}", draft.image_ext);

    let metadata = MineralDiskRecord {
        common_name: draft.common_name,
        description: draft.description,
        mineral_family: draft.mineral_family,
        formula: draft.formula,
        hardness_mohs: draft.hardness_mohs,
        density_g_cm3: draft.density_g_cm3,
        crystal_system: draft.crystal_system,
        color: draft.color,
        streak: draft.streak,
        luster: draft.luster,
        major_elements_pct: draft.major_elements_pct,
        notes: draft.notes,
        image_file: Some(image_file.clone()),
    };

    let (localized_records, translation_stats) = build_localized_metadata(state, &metadata).await;
    save_localized_mineral_records(
        state.data_root.as_path(),
        &slug,
        &folder_name,
        &localized_records,
        NewImageRecord {
            bytes: &draft.image_bytes,
            ext: &draft.image_ext,
            original_name: Some(&image_file),
        },
    )?;

    Ok((slug, translation_stats))
}

async fn build_localized_metadata(
    state: &AppState,
    english: &MineralDiskRecord,
) -> (HashMap<String, MineralDiskRecord>, TranslationStats) {
    let mut out = HashMap::new();
    out.insert(Language::En.code().to_string(), english.clone());

    let mut stats = TranslationStats::default();
    if state.openai_api_key.as_ref().is_none() {
        warn!(
            "OPENAI_API_KEY is not configured; writing English fallback metadata for all non-English languages"
        );
        for language in Language::all() {
            if *language == Language::En {
                continue;
            }
            out.insert(language.code().to_string(), english.clone());
            stats.fallback_lang_codes.push(language.code().to_string());
        }
        return (out, stats);
    }

    for language in Language::all() {
        if *language == Language::En {
            continue;
        }

        let code = language.code().to_string();
        match request_openai_translation(state, english, *language).await {
            Ok(translated) => {
                out.insert(code, translated);
                stats.translated_count += 1;
            }
            Err(err) => {
                warn!(
                    "metadata translation fallback lang={} reason={:#}",
                    language.code(),
                    err
                );
                out.insert(language.code().to_string(), english.clone());
                stats.fallback_lang_codes.push(language.code().to_string());
            }
        }
    }

    (out, stats)
}

async fn request_openai_translation(
    state: &AppState,
    english: &MineralDiskRecord,
    target_language: Language,
) -> Result<MineralDiskRecord> {
    let api_key = state
        .openai_api_key
        .as_ref()
        .as_ref()
        .ok_or_else(|| anyhow!("OPENAI_API_KEY is not configured"))?;

    let schema = serde_json::json!({
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "common_name": {"type": "string"},
        "description": {"type": "string"},
        "mineral_family": {"type": "string"},
        "crystal_system": {"type": "string"},
        "color": {"type": "string"},
        "streak": {"type": "string"},
        "luster": {"type": "string"},
        "notes": {"type": "string"}
      },
      "required": [
        "common_name",
        "description",
        "mineral_family",
        "crystal_system",
        "color",
        "streak",
        "luster",
        "notes"
      ]
    });

    let source_payload = serde_json::json!({
        "common_name": english.common_name,
        "description": english.description,
        "mineral_family": english.mineral_family,
        "formula": english.formula,
        "crystal_system": english.crystal_system,
        "color": english.color,
        "streak": english.streak,
        "luster": english.luster,
        "notes": english.notes,
    });

    let user_prompt = format!(
        "Translate the mineral metadata JSON from English into {target_name} ({target_code}). \
Use concise professional wording. Preserve chemical formulas and symbols exactly.\n\nSource JSON:\n{source_json}",
        target_name = target_language.english_name(),
        target_code = target_language.code(),
        source_json = source_payload
    );

    let request = ChatCompletionsRequest {
        model: (*state.openai_translation_model).clone(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: vec![MessagePart::Text {
                    text: "You are a translation engine for mineral catalog metadata. Output JSON only and follow schema exactly.".to_string(),
                }],
            },
            ChatMessage {
                role: "user".to_string(),
                content: vec![MessagePart::Text { text: user_prompt }],
            },
        ],
        response_format: ResponseFormat {
            kind: "json_schema".to_string(),
            json_schema: JsonSchemaSpec {
                name: format!("mineral_translation_{}", target_language.code()),
                strict: true,
                schema,
            },
        },
        temperature: 0.1,
    };

    let response = state
        .http_client
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(api_key)
        .json(&request)
        .send()
        .await
        .with_context(|| "failed to call OpenAI translation endpoint")?;

    if !response.status().is_success() {
        let status = response.status();
        return Err(anyhow!("OpenAI translation error {status}"));
    }

    let parsed: ChatCompletionsResponse = response
        .json()
        .await
        .with_context(|| "failed to parse OpenAI translation response")?;

    let content = parsed
        .choices
        .first()
        .map(|choice| choice.message.content.as_str())
        .ok_or_else(|| anyhow!("OpenAI translation response had no choices"))?;

    let translated: AiMineralTranslation =
        serde_json::from_str(content).with_context(|| "invalid OpenAI translation JSON payload")?;

    Ok(MineralDiskRecord {
        common_name: translated_or_source(translated.common_name, &english.common_name),
        description: translated_or_source(translated.description, &english.description),
        mineral_family: translated_or_source(translated.mineral_family, &english.mineral_family),
        formula: english.formula.clone(),
        hardness_mohs: english.hardness_mohs,
        density_g_cm3: english.density_g_cm3,
        crystal_system: translated_or_source(translated.crystal_system, &english.crystal_system),
        color: translated_or_source(translated.color, &english.color),
        streak: translated_or_source(translated.streak, &english.streak),
        luster: translated_or_source(translated.luster, &english.luster),
        major_elements_pct: english.major_elements_pct.clone(),
        notes: translated_or_source(translated.notes, &english.notes),
        image_file: english.image_file.clone(),
    })
}

fn translated_or_source(value: String, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn create_unique_slug(state: &AppState, family_slug: &str) -> Result<String, AppError> {
    for _ in 0..16 {
        let id = generate_secure_hex(4)?;
        let candidate = format!("mineral.{family_slug}.0x{id}");
        if !mineral_slug_exists(state.data_root.as_path(), &candidate)? {
            return Ok(candidate);
        }
    }

    Err(AppError::Internal(anyhow!(
        "failed to allocate unique mineral id"
    )))
}

fn slugify_family(value: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }

    while out.ends_with('-') {
        out.pop();
    }

    if out.is_empty() {
        "unknown".to_string()
    } else {
        out
    }
}

fn generate_secure_hex(byte_len: usize) -> Result<String, AppError> {
    let mut buf = vec![0_u8; byte_len];
    getrandom::getrandom(&mut buf).map_err(|err| {
        AppError::Internal(anyhow!("failed to generate secure random bytes: {err}"))
    })?;

    Ok(buf.iter().map(|b| format!("{b:02x}")).collect::<String>())
}

fn validate_actor_id(name: &str, value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(anyhow!("{name} cannot be empty"));
    }
    if value.chars().count() > 200 {
        return Err(anyhow!("{name} must not exceed 200 characters"));
    }
    if !value.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | ':' | '@' | '-')
    }) {
        return Err(anyhow!(
            "{name} may contain only letters, numbers, '.', '_', ':', '@', and '-'"
        ));
    }
    Ok(value.to_string())
}

fn configured_actor_id(name: &str, default: &str) -> Result<String> {
    match std::env::var(name) {
        Ok(value) => validate_actor_id(name, &value),
        Err(std::env::VarError::NotPresent) => validate_actor_id(name, default),
        Err(std::env::VarError::NotUnicode(_)) => Err(anyhow!("{name} must be valid Unicode")),
    }
}

fn validate_ingestion_api_token(value: &str) -> Result<String> {
    let value = value.trim();
    if value.len() < 32 || !value.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(anyhow!(
            "INGESTION_API_TOKEN must contain at least 32 printable ASCII characters without whitespace"
        ));
    }
    Ok(value.to_string())
}

fn configured_ingestion_api_token() -> Result<Option<String>> {
    let value = match std::env::var("INGESTION_API_TOKEN") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => return Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(anyhow!("INGESTION_API_TOKEN must be valid Unicode"))
        }
    };
    validate_ingestion_api_token(&value).map(Some)
}

fn validate_public_catalog_base_url(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(anyhow!("PUBLIC_CATALOG_BASE_URL cannot be empty when set"));
    }
    let url =
        reqwest::Url::parse(value).context("PUBLIC_CATALOG_BASE_URL must be an absolute URL")?;
    if url.username() != "" || url.password().is_some() {
        return Err(anyhow!(
            "PUBLIC_CATALOG_BASE_URL must not contain credentials"
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(anyhow!(
            "PUBLIC_CATALOG_BASE_URL must not contain a query or fragment"
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("PUBLIC_CATALOG_BASE_URL must contain a host"))?;
    match url.scheme() {
        "https" => {}
        "http" => {
            let is_literal_loopback = host
                .parse::<IpAddr>()
                .is_ok_and(|address| address.is_loopback());
            if !is_literal_loopback {
                return Err(anyhow!(
                    "PUBLIC_CATALOG_BASE_URL may use http only with a literal loopback address"
                ));
            }
        }
        _ => {
            return Err(anyhow!(
                "PUBLIC_CATALOG_BASE_URL must use https (or loopback http for development)"
            ))
        }
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}

fn public_catalog_minerals_url(state: &AppState) -> Option<String> {
    state
        .public_catalog_base_url
        .as_deref()
        .map(|base| format!("{base}/#/minerals"))
}

fn configured_public_catalog_base_url() -> Result<Option<String>> {
    match std::env::var("PUBLIC_CATALOG_BASE_URL") {
        Ok(value) => validate_public_catalog_base_url(&value).map(Some),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(anyhow!("PUBLIC_CATALOG_BASE_URL must be valid Unicode"))
        }
    }
}

fn parse_trusted_proxy_ips(value: &str) -> Result<BTreeSet<IpAddr>> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(BTreeSet::new());
    }

    value
        .split(',')
        .map(|entry| {
            let entry = entry.trim();
            if entry.is_empty() {
                return Err(anyhow!(
                    "TRUSTED_PROXY_IPS must be a comma-separated list of IP addresses"
                ));
            }
            entry
                .parse::<IpAddr>()
                .with_context(|| format!("invalid trusted proxy IP '{entry}'"))
        })
        .collect()
}

fn configured_trusted_proxy_ips() -> Result<BTreeSet<IpAddr>> {
    match std::env::var("TRUSTED_PROXY_IPS") {
        Ok(value) => parse_trusted_proxy_ips(&value),
        Err(std::env::VarError::NotPresent) => Ok(BTreeSet::new()),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(anyhow!("TRUSTED_PROXY_IPS must be valid Unicode"))
        }
    }
}

fn parse_env_flag(name: &str, value: &str) -> Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(anyhow!(
            "{name} must be one of true/false, 1/0, yes/no, or on/off"
        )),
    }
}

fn configured_env_flag(name: &str, default: bool) -> Result<bool> {
    match std::env::var(name) {
        Ok(value) => parse_env_flag(name, &value),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(std::env::VarError::NotUnicode(_)) => Err(anyhow!("{name} must be valid Unicode")),
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            error!("failed to install Ctrl+C handler: {err}");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(err) => error!("failed to install SIGTERM handler: {err}"),
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    info!("shutdown signal received; draining active requests");
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeSet, HashMap},
        io::{Read, Write},
        net::{IpAddr, SocketAddr, TcpStream},
        path::Path,
        sync::{Arc, Mutex, RwLock},
        time::Duration,
    };

    use axum::{
        extract::DefaultBodyLimit,
        http::{header, HeaderMap, HeaderValue},
        middleware,
        response::IntoResponse,
        routing::{get, post},
        Router,
    };
    use reqwest::Client;
    use tokio::sync::Semaphore;

    use super::{
        admin_create_ingestion_batch, admin_import_minerals, admin_import_provider,
        admin_review_notice, admin_static_asset, admit_admin_write_request,
        admit_ingestion_write_request, authenticate_ingestion_bearer, constant_time_eq,
        is_admin_path, map_ingestion_backend_error, mineral_status_display, parse_env_flag,
        parse_trusted_proxy_ips, require_ingestion_reader_actor, require_ingestion_reviewer_actor,
        require_ingestion_writer_actor, require_same_origin, require_valid_ingestion_batch_id,
        resolve_client_ip, review_claim_scope_display, security_headers,
        try_ingestion_writer_permit, ui_text, validate_actor_id, validate_ingestion_api_token,
        validate_public_catalog_base_url, AppError, AppState, IngestionAuthKind, Language,
        ADMIN_IMPORT_MAX_BYTES, ADMIN_INGESTION_CHUNK_MAX_BYTES,
        ADMIN_INGESTION_MANIFEST_MAX_BYTES,
    };
    use minerals::registry::{MineralIngestionProblem, MineralIngestionProblemKind};

    fn ingestion_test_state(data_root: &Path) -> AppState {
        let sessions = HashMap::from([("test-session".to_string(), std::time::Instant::now())]);
        AppState {
            catalogs_by_lang: Arc::new(RwLock::new(HashMap::new())),
            admin_sessions: Arc::new(Mutex::new(sessions)),
            admin_login_failures: Arc::new(Mutex::new(HashMap::new())),
            admin_drafts: Arc::new(Mutex::new(HashMap::new())),
            data_root: Arc::new(data_root.to_path_buf()),
            admin_password: Arc::new("test-admin-password".to_string()),
            openai_api_key: Arc::new(None),
            openai_model: Arc::new("test-model".to_string()),
            openai_translation_model: Arc::new("test-model".to_string()),
            default_language: Language::En,
            http_client: Arc::new(Client::new()),
            ingestion_writer: Arc::new(Semaphore::new(1)),
            admin_reviewer_id: Arc::new("reviewer.primary".to_string()),
            ingestion_api_token: Arc::new(Some("0123456789abcdef0123456789abcdef".to_string())),
            ingestion_adapter_id: Arc::new("adapter.primary".to_string()),
            trusted_proxy_ips: Arc::new(BTreeSet::new()),
            public_catalog_base_url: Arc::new(None),
            secure_cookies: false,
            admin_sql_enabled: false,
        }
    }

    fn send_incomplete_oversized_manifest(
        address: SocketAddr,
        authorization: Option<&str>,
    ) -> String {
        let mut stream = TcpStream::connect(address).expect("connect to admission test server");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set admission response timeout");
        let authorization = authorization
            .map(|token| format!("Authorization: Bearer {token}\r\n"))
            .unwrap_or_default();
        let request = format!(
            "POST /admin/ingestion/batches HTTP/1.1\r\nHost: minerals.test\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{authorization}Connection: close\r\n\r\n",
            ADMIN_INGESTION_MANIFEST_MAX_BYTES + 1
        );
        stream
            .write_all(request.as_bytes())
            .expect("write request headers without body");
        let mut response = [0_u8; 2_048];
        let count = stream
            .read(&mut response)
            .expect("admission must respond before reading the declared body");
        String::from_utf8_lossy(&response[..count]).into_owned()
    }

    fn send_incomplete_admin_import(
        address: SocketAddr,
        path: &str,
        cookie: Option<&str>,
        origin: Option<&str>,
    ) -> String {
        let mut stream = TcpStream::connect(address).expect("connect to admin admission server");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set admin admission response timeout");
        let cookie = cookie
            .map(|value| format!("Cookie: admin_session={value}\r\n"))
            .unwrap_or_default();
        let origin = origin
            .map(|value| format!("Origin: {value}\r\n"))
            .unwrap_or_default();
        let request = format!(
            "POST {path} HTTP/1.1\r\nHost: minerals.test\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{cookie}{origin}Connection: close\r\n\r\n",
            ADMIN_IMPORT_MAX_BYTES,
        );
        stream
            .write_all(request.as_bytes())
            .expect("write admin import headers without body");
        let mut response = [0_u8; 2_048];
        let count = stream
            .read(&mut response)
            .expect("admin admission must respond before reading the declared body");
        String::from_utf8_lossy(&response[..count]).into_owned()
    }

    fn send_get(address: SocketAddr, path: &str) -> String {
        let mut stream = TcpStream::connect(address).expect("connect to static test server");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set static response timeout");
        let request =
            format!("GET {path} HTTP/1.1\r\nHost: minerals.test\r\nConnection: close\r\n\r\n");
        stream
            .write_all(request.as_bytes())
            .expect("write static request");
        let mut response = [0_u8; 2_048];
        let count = stream.read(&mut response).expect("read static response");
        String::from_utf8_lossy(&response[..count]).into_owned()
    }

    #[test]
    fn constant_time_comparison_matches_exact_bytes() {
        assert!(constant_time_eq(b"waajacu", b"waajacu"));
        assert!(!constant_time_eq(b"waajacu", b"minerals"));
        assert!(!constant_time_eq(b"waajacu", b"waajacu-longer"));
    }

    #[test]
    fn forwarded_client_ip_is_used_only_across_configured_proxy_hops() {
        let direct_peer = "192.0.2.10".parse().expect("direct peer IP");
        let proxy = "127.0.0.1".parse().expect("proxy IP");
        let upstream_proxy = "10.0.0.8".parse().expect("upstream proxy IP");
        let client: IpAddr = "198.51.100.24".parse().expect("client IP");
        let spoofed_prefix = "203.0.113.99";
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_str(&format!("{spoofed_prefix}, {client}, {upstream_proxy}"))
                .expect("forwarded chain"),
        );

        let no_trusted_proxies = BTreeSet::new();
        assert_eq!(
            resolve_client_ip(direct_peer, &headers, &no_trusted_proxies),
            direct_peer,
            "an untrusted peer must not choose a bucket through forwarded headers"
        );

        let trusted = BTreeSet::from([proxy, upstream_proxy]);
        assert_eq!(resolve_client_ip(proxy, &headers, &trusted), client);

        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_str(&format!("not-an-ip, {client}, {upstream_proxy}"))
                .expect("forwarded chain with an untrusted prefix"),
        );
        assert_eq!(
            resolve_client_ip(proxy, &headers, &trusted),
            client,
            "an untrusted prefix must not collapse every proxied client into one bucket"
        );

        headers.insert("x-forwarded-for", HeaderValue::from_static("not-an-ip"));
        assert_eq!(
            resolve_client_ip(proxy, &headers, &trusted),
            proxy,
            "malformed metadata at the trust boundary must fail closed"
        );
    }

    #[test]
    fn trusted_proxy_and_boolean_configuration_are_strict() {
        let proxies = parse_trusted_proxy_ips("127.0.0.1, 2001:db8::1")
            .expect("valid trusted proxy configuration");
        assert!(proxies.contains(&"127.0.0.1".parse().expect("IPv4")));
        assert!(proxies.contains(&"2001:db8::1".parse().expect("IPv6")));
        assert!(parse_trusted_proxy_ips("")
            .expect("empty means disabled")
            .is_empty());
        assert!(parse_trusted_proxy_ips("127.0.0.1,").is_err());
        assert!(parse_trusted_proxy_ips("127.0.0.0/8").is_err());

        assert!(parse_env_flag("FLAG", "true").expect("true flag"));
        assert!(!parse_env_flag("FLAG", "OFF").expect("false flag"));
        assert!(parse_env_flag("FLAG", "sometimes").is_err());
    }

    #[test]
    fn public_catalog_links_require_a_safe_absolute_origin() {
        assert_eq!(
            validate_public_catalog_base_url("https://minerals.example/catalog/")
                .expect("https catalog URL"),
            "https://minerals.example/catalog"
        );
        assert_eq!(
            validate_public_catalog_base_url("http://127.0.0.1:8080/")
                .expect("loopback development URL"),
            "http://127.0.0.1:8080"
        );
        assert!(validate_public_catalog_base_url("http://minerals.example").is_err());
        assert!(validate_public_catalog_base_url("http://localhost:8080").is_err());
        assert!(validate_public_catalog_base_url("https://user:secret@minerals.example").is_err());
        assert!(validate_public_catalog_base_url("https://minerals.example/#/wrong").is_err());
    }

    #[test]
    fn ingestion_bearer_auth_is_server_attributed_and_fails_closed() {
        let secret = "0123456789abcdef0123456789abcdef";
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {secret}")).expect("authorization header"),
        );

        let actor = authenticate_ingestion_bearer(&headers, Some(secret), "adapter.primary")
            .expect("valid token")
            .expect("adapter actor");
        assert_eq!(actor.id, "adapter.primary");
        assert_eq!(actor.kind, IngestionAuthKind::Adapter);

        assert!(matches!(
            authenticate_ingestion_bearer(
                &headers,
                Some("ffffffffffffffffffffffffffffffff"),
                "adapter.primary"
            ),
            Err(AppError::Unauthorized(_))
        ));
        assert!(matches!(
            authenticate_ingestion_bearer(&headers, None, "adapter.primary"),
            Err(AppError::Unauthorized(_))
        ));
        assert!(
            authenticate_ingestion_bearer(&HeaderMap::new(), Some(secret), "adapter.primary")
                .expect("missing bearer is not an adapter attempt")
                .is_none()
        );

        headers.append(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer duplicate-token-is-not-accepted"),
        );
        assert!(matches!(
            authenticate_ingestion_bearer(&headers, Some(secret), "adapter.primary"),
            Err(AppError::Unauthorized(_))
        ));

        let mut malformed = HeaderMap::new();
        malformed.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Basic not-a-bearer-token"),
        );
        assert!(matches!(
            authenticate_ingestion_bearer(&malformed, Some(secret), "adapter.primary"),
            Err(AppError::Unauthorized(_))
        ));
    }

    #[test]
    fn ingestion_actor_ids_are_bounded_and_admin_routes_are_private() {
        assert_eq!(
            validate_actor_id("ACTOR", " adapter.primary ").expect("actor id"),
            "adapter.primary"
        );
        assert!(validate_actor_id("ACTOR", "").is_err());
        assert!(validate_actor_id("ACTOR", "adapter secret").is_err());
        assert!(is_admin_path("/admin"));
        assert!(is_admin_path("/admin/ingestion"));
        assert!(is_admin_path("/admin/ingestion/batches/1"));
        assert!(!is_admin_path("/administrator"));
        assert!(require_valid_ingestion_batch_id(&format!("batch_{}", "a".repeat(64))).is_ok());
        assert!(require_valid_ingestion_batch_id("batch_AAAA").is_err());
        assert!(require_valid_ingestion_batch_id("../batch_secret").is_err());
    }

    #[test]
    fn ingestion_credentials_keep_adapter_and_reviewer_authority_separate() {
        let temp = tempfile::tempdir().expect("temporary data root");
        let state = ingestion_test_state(temp.path());
        let mut bearer = HeaderMap::new();
        bearer.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer 0123456789abcdef0123456789abcdef"),
        );

        let adapter = require_ingestion_writer_actor(&state, &bearer)
            .expect("adapter can mutate ingestion quarantine");
        assert_eq!(adapter.id, "adapter.primary");
        assert_eq!(adapter.kind, IngestionAuthKind::Adapter);
        assert_eq!(
            require_ingestion_reader_actor(&state, &bearer)
                .expect("adapter can recover status")
                .kind,
            IngestionAuthKind::Adapter
        );
        assert!(matches!(
            require_ingestion_reviewer_actor(&state, &bearer),
            Err(AppError::Unauthorized(_))
        ));

        let mut admin = HeaderMap::new();
        admin.insert(
            header::COOKIE,
            HeaderValue::from_static("admin_session=test-session"),
        );
        admin.insert(header::HOST, HeaderValue::from_static("minerals.test"));
        assert_eq!(
            require_ingestion_reader_actor(&state, &admin)
                .expect("safe GET only needs an admin session")
                .kind,
            IngestionAuthKind::Admin
        );
        assert!(matches!(
            require_ingestion_writer_actor(&state, &admin),
            Err(AppError::Unauthorized(_))
        ));
        assert!(matches!(
            require_ingestion_reviewer_actor(&state, &admin),
            Err(AppError::Unauthorized(_))
        ));

        admin.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://minerals.test"),
        );
        assert_eq!(
            require_ingestion_writer_actor(&state, &admin)
                .expect("same-origin admin can write quarantine")
                .kind,
            IngestionAuthKind::Admin
        );
        assert_eq!(
            require_ingestion_reviewer_actor(&state, &admin)
                .expect("same-origin admin can decide releases"),
            "reviewer.primary"
        );
    }

    #[test]
    fn ingestion_writer_fails_fast_with_retry_after_when_busy() {
        let writer = Arc::new(Semaphore::new(1));
        let held = writer
            .clone()
            .try_acquire_owned()
            .expect("hold sole writer permit");
        let response = try_ingestion_writer_permit(&writer)
            .expect_err("second writer must fail without queueing")
            .into_response();
        assert_eq!(
            response.status(),
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            response
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok()),
            Some("1")
        );
        drop(held);
        assert!(try_ingestion_writer_permit(&writer).is_ok());
        assert_eq!(ADMIN_INGESTION_CHUNK_MAX_BYTES, 8 * 1024 * 1024);
    }

    #[tokio::test]
    async fn legacy_admin_imports_authenticate_before_reading_large_json_bodies() {
        let temp = tempfile::tempdir().expect("temporary data root");
        let state = ingestion_test_state(temp.path());
        let app = Router::new()
            .route(
                "/admin/minerals/import",
                post(admin_import_minerals)
                    .layer(DefaultBodyLimit::max(ADMIN_IMPORT_MAX_BYTES))
                    .layer(middleware::from_fn_with_state(
                        state.clone(),
                        admit_admin_write_request,
                    )),
            )
            .route(
                "/admin/providers/import",
                post(admin_import_provider)
                    .layer(DefaultBodyLimit::max(ADMIN_IMPORT_MAX_BYTES))
                    .layer(middleware::from_fn_with_state(
                        state.clone(),
                        admit_admin_write_request,
                    )),
            )
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind admin admission test server");
        let address = listener.local_addr().expect("admin admission address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("admin admission test server");
        });

        for path in ["/admin/minerals/import", "/admin/providers/import"] {
            let unauthorized_path = path.to_string();
            let unauthorized = tokio::task::spawn_blocking(move || {
                send_incomplete_admin_import(address, &unauthorized_path, None, None)
            })
            .await
            .expect("unauthorized import request task");
            assert!(
                unauthorized.starts_with("HTTP/1.1 401"),
                "unexpected unauthenticated response: {unauthorized:?}"
            );

            let cross_origin_path = path.to_string();
            let cross_origin = tokio::task::spawn_blocking(move || {
                send_incomplete_admin_import(
                    address,
                    &cross_origin_path,
                    Some("test-session"),
                    Some("https://attacker.invalid"),
                )
            })
            .await
            .expect("cross-origin import request task");
            assert!(
                cross_origin.starts_with("HTTP/1.1 401"),
                "unexpected cross-origin response: {cross_origin:?}"
            );
        }

        server.abort();
    }

    #[tokio::test]
    async fn only_embedded_admin_assets_are_served() {
        let app = Router::new()
            .route("/static/:asset", get(admin_static_asset))
            .layer(middleware::from_fn(security_headers));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind static test server");
        let address = listener.local_addr().expect("static test address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("static test server");
        });

        for path in [
            "/static/admin.html",
            "/static/admin%2ehtml",
            "/static/admin.ht%6dl",
            "/static/report%2etex",
            "/static/map.html",
            "/static/map.css",
            "/static/map-loader.js",
        ] {
            let path = path.to_string();
            let response = tokio::task::spawn_blocking(move || send_get(address, &path))
                .await
                .expect("private static request task");
            assert!(
                response.starts_with("HTTP/1.1 404"),
                "private static path was served: {response:?}"
            );
        }

        for path in [
            "/static/app.css",
            "/static/theme.js",
            "/static/loading_1.png",
        ] {
            let path = path.to_string();
            let response = tokio::task::spawn_blocking(move || send_get(address, &path))
                .await
                .expect("public static request task");
            assert!(
                response.starts_with("HTTP/1.1 200"),
                "allowlisted static path was blocked: {response:?}"
            );
        }

        server.abort();
    }

    #[tokio::test]
    async fn ingestion_admission_rejects_oversized_requests_before_reading_the_body() {
        let temp = tempfile::tempdir().expect("temporary data root");
        let state = ingestion_test_state(temp.path());
        let held = state
            .ingestion_writer
            .clone()
            .try_acquire_owned()
            .expect("hold sole ingestion writer permit");
        let app = Router::new()
            .route(
                "/admin/ingestion/batches",
                post(admin_create_ingestion_batch)
                    .layer(DefaultBodyLimit::max(ADMIN_INGESTION_MANIFEST_MAX_BYTES))
                    .layer(middleware::from_fn_with_state(
                        state.clone(),
                        admit_ingestion_write_request,
                    )),
            )
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind admission test server");
        let address = listener.local_addr().expect("admission server address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("admission test server");
        });

        let unauthorized =
            tokio::task::spawn_blocking(move || send_incomplete_oversized_manifest(address, None))
                .await
                .expect("unauthorized request task");
        assert!(
            unauthorized.starts_with("HTTP/1.1 401"),
            "unexpected unauthorized response: {unauthorized:?}"
        );

        let busy = tokio::task::spawn_blocking(move || {
            send_incomplete_oversized_manifest(address, Some("0123456789abcdef0123456789abcdef"))
        })
        .await
        .expect("contended request task");
        assert!(
            busy.starts_with("HTTP/1.1 503"),
            "unexpected contended response: {busy:?}"
        );
        assert!(busy.to_ascii_lowercase().contains("retry-after: 1"));

        server.abort();
        drop(held);
    }

    #[test]
    fn ingestion_errors_expose_only_static_codes() {
        let error = anyhow::Error::new(MineralIngestionProblem {
            kind: MineralIngestionProblemKind::Conflict,
            code: "chunk_replay_conflict",
            message: "private database path C:\\secret\\minerals.db for batch_private".to_string(),
        });
        match map_ingestion_backend_error("store test chunk", error) {
            AppError::Conflict(message) => {
                assert_eq!(message, "ingestion request rejected: chunk_replay_conflict");
                assert!(!message.contains("secret"));
                assert!(!message.contains("batch_private"));
            }
            other => panic!("unexpected mapped error: {other:?}"),
        }
    }

    #[test]
    fn ingestion_tokens_are_header_safe_and_sufficiently_long() {
        assert_eq!(
            validate_ingestion_api_token(" 0123456789abcdef0123456789abcdef ")
                .expect("valid token"),
            "0123456789abcdef0123456789abcdef"
        );
        assert!(validate_ingestion_api_token("short").is_err());
        assert!(validate_ingestion_api_token("0123456789abcdef0123456789abcde ").is_err());
        assert!(validate_ingestion_api_token("0123456789abcdef0123456789abcdeé").is_err());
    }

    #[test]
    fn admin_origin_check_fails_closed_and_accepts_same_origin_referers() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("127.0.0.1:7979"));
        assert!(require_same_origin(&headers).is_err());

        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://127.0.0.1:7979"),
        );
        assert!(require_same_origin(&headers).is_ok());
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://127.0.0.1:7979.evil.invalid"),
        );
        assert!(require_same_origin(&headers).is_err());

        headers.remove(header::ORIGIN);
        headers.insert(
            header::REFERER,
            HeaderValue::from_static("http://127.0.0.1:7979/admin/reviews"),
        );
        assert!(require_same_origin(&headers).is_ok());
        headers.insert(
            header::REFERER,
            HeaderValue::from_static("http://127.0.0.1:7979.evil.invalid/admin"),
        );
        assert!(require_same_origin(&headers).is_err());
    }

    #[test]
    fn review_claim_scopes_are_human_and_localized() {
        assert_eq!(
            review_claim_scope_display(Language::En, "properties.hardness_mohs"),
            "Hardness (Mohs)"
        );
        assert_eq!(
            review_claim_scope_display(Language::Es, "properties.hardness_mohs"),
            "Dureza (Mohs)"
        );
        assert!(!review_claim_scope_display(Language::Ar, "safety.handling").contains('.'));
    }

    #[test]
    fn admin_review_notices_report_completed_actions() {
        assert_eq!(
            admin_review_notice(Language::En, Some("approved")).as_deref(),
            Some("Mineral revision approved and published.")
        );
        assert_eq!(
            admin_review_notice(Language::Es, Some("rejected")).as_deref(),
            Some("La revisión del mineral fue rechazada.")
        );
        assert_eq!(admin_review_notice(Language::En, Some("approve")), None);
        assert_eq!(admin_review_notice(Language::En, None), None);
    }

    #[test]
    fn mineral_registry_actions_are_localized() {
        let english = ui_text(Language::En).registry;

        for language in Language::all().iter().copied() {
            let text = ui_text(language).registry;
            assert!(!text.title.trim().is_empty());
            assert!(!text.search_action.trim().is_empty());
            for pagination_label in [
                text.pagination_showing,
                text.pagination_of,
                text.pagination_results,
                text.pagination_page,
                text.pagination_previous,
                text.pagination_next,
                text.properties,
                text.safety,
                text.price_unit,
                text.price_lot,
                text.price_package,
            ] {
                assert!(!pagination_label.trim().is_empty());
            }

            if language != Language::En {
                assert_ne!(text.title, english.title);
                assert_ne!(text.search_action, english.search_action);
                assert_ne!(text.pagination_showing, english.pagination_showing);
                assert_ne!(text.pagination_previous, english.pagination_previous);
                assert_ne!(text.properties, english.properties);
                assert_ne!(text.safety, english.safety);
                assert_ne!(text.price_unit, english.price_unit);
                assert_ne!(text.price_package, english.price_package);
            }
        }
    }

    #[test]
    fn mineral_statuses_are_presented_with_localized_human_labels() {
        let spanish = ui_text(Language::Es).registry;
        assert_eq!(
            mineral_status_display("verified", &spanish),
            spanish.status_verified
        );
        assert_eq!(
            mineral_status_display("generated", &spanish),
            spanish.status_preliminary
        );
    }
}
