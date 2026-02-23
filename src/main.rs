mod agent;
mod i18n;
mod models;
mod pdf;
mod web;

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::Read,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
};

use anyhow::{anyhow, Context, Result};
use axum::{
    extract::{Multipart, Path as AxumPath, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Json, Router,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use i18n::{language_options, ui_text, Language};
use models::{
    is_valid_mineral_folder_name, load_minerals, major_elements_to_text, parse_major_elements,
    Mineral, MineralDiskRecord, MineralFormData, ReportRequest,
};
use pdf::GeneratedArtifacts;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{fs, net::TcpListener};
use tower_http::services::ServeDir;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use crate::{
    agent::run_agentic_chain,
    pdf::PdfGenerator,
    web::{
        AboutTemplate, AdminTemplate, HomeTemplate, IndexTemplate, InfoTemplate, MineralTemplate,
        TemplateResponse,
    },
};

#[derive(Clone)]
struct AppState {
    catalogs_by_lang: Arc<RwLock<HashMap<String, MineralCatalog>>>,
    admin_sessions: Arc<Mutex<HashSet<String>>>,
    admin_drafts: Arc<Mutex<HashMap<String, AdminDraft>>>,
    pdf_generator: Arc<PdfGenerator>,
    data_root: Arc<PathBuf>,
    admin_password: Arc<String>,
    openai_api_key: Arc<Option<String>>,
    openai_model: Arc<String>,
    default_language: Language,
    http_client: Arc<Client>,
}

#[derive(Debug, Clone)]
struct AdminDraft {
    image_bytes: Vec<u8>,
    image_ext: String,
}

#[derive(Debug, Clone, Default)]
struct MineralCatalog {
    by_slug: HashMap<String, Mineral>,
    ordered: Vec<Mineral>,
}

#[derive(Debug, Deserialize)]
struct LanguageSelectionRequest {
    lang: String,
}

impl MineralCatalog {
    fn new(minerals: Vec<Mineral>) -> Self {
        let by_slug = minerals
            .iter()
            .cloned()
            .map(|mineral| (mineral.slug.clone(), mineral))
            .collect::<HashMap<_, _>>();

        Self {
            by_slug,
            ordered: minerals,
        }
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

#[derive(Debug, Serialize)]
struct PdfApiResponse {
    pdf_path: String,
    html_path: String,
    summary: String,
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
    formula: String,
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
    let _ = dotenvy::from_filename(".env");
    let _ = dotenvy::from_filename_override(".env.local");

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("minerals=info,tower_http=info"));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    let data_root = PathBuf::from("data");
    fs::create_dir_all(data_root.join("minerals"))
        .await
        .context("failed to create data/minerals directory")?;

    let admin_password = std::env::var("ADMIN_PASSWORD")
        .context("ADMIN_PASSWORD is required. Set it in .env.local (or env) before starting.")?;
    if admin_password.trim().is_empty() {
        return Err(anyhow!("ADMIN_PASSWORD cannot be empty"));
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

    let state = AppState {
        catalogs_by_lang: Arc::new(RwLock::new(HashMap::new())),
        admin_sessions: Arc::new(Mutex::new(HashSet::new())),
        admin_drafts: Arc::new(Mutex::new(HashMap::new())),
        pdf_generator: Arc::new(PdfGenerator::new(data_root.join("minerals"))),
        data_root: Arc::new(data_root),
        admin_password: Arc::new(admin_password),
        openai_api_key: Arc::new(std::env::var("OPENAI_API_KEY").ok()),
        openai_model: Arc::new(
            std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string()),
        ),
        default_language,
        http_client: Arc::new(
            Client::builder()
                .build()
                .context("failed to initialize HTTP client")?,
        ),
    };

    let app = Router::new()
        .route("/", get(home_page))
        .route("/language", post(set_language))
        .route("/minerals", get(index))
        .route("/about", get(about_page))
        .route("/pages/:slug", get(info_page))
        .route("/minerals/:slug", get(mineral_page))
        .route("/minerals/:slug/pdf", post(generate_pdf_form))
        .route("/api/minerals/:slug/pdf", post(generate_pdf_api))
        .route("/admin", get(admin_page))
        .route("/admin/login", post(admin_login))
        .route("/admin/logout", post(admin_logout))
        .route("/admin/minerals/suggest", post(admin_suggest_mineral))
        .route("/admin/minerals/publish", post(admin_publish_mineral))
        .nest_service("/static", ServeDir::new("static"))
        .nest_service("/data", ServeDir::new("data"))
        .with_state(state);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(7979);

    let address = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(address)
        .await
        .with_context(|| format!("failed to bind to {address}"))?;

    info!("minerals server listening on http://{address}");
    axum::serve(listener, app)
        .await
        .context("server failed unexpectedly")?;

    Ok(())
}

async fn home_page(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> TemplateResponse<HomeTemplate> {
    let language = resolve_language(&state, &headers);

    TemplateResponse(HomeTemplate {
        lang_code: language.code().to_string(),
        lang_dir: language.dir().to_string(),
        txt: ui_text(language),
        language_options: language_options(),
        current_lang_code: language.code(),
    })
}

async fn set_language(
    State(state): State<AppState>,
    Form(request): Form<LanguageSelectionRequest>,
) -> Result<Response, AppError> {
    let selected = Language::from_code(&request.lang).unwrap_or(state.default_language);
    let mut response = Redirect::to("/").into_response();
    append_set_cookie(
        &mut response,
        &format!(
            "lang={}; Path=/; SameSite=Lax; Max-Age=31536000",
            selected.code()
        ),
    )?;
    Ok(response)
}

async fn index(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<TemplateResponse<IndexTemplate>, AppError> {
    let language = resolve_language(&state, &headers);
    let has_admin_session = has_admin_session(&state, &headers);
    let minerals = catalog_for_language(&state, language)?.ordered;

    Ok(TemplateResponse(IndexTemplate {
        lang_code: language.code().to_string(),
        lang_dir: language.dir().to_string(),
        txt: ui_text(language),
        minerals,
        has_admin_session,
    }))
}

async fn about_page(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> TemplateResponse<AboutTemplate> {
    let language = resolve_language(&state, &headers);
    TemplateResponse(AboutTemplate {
        lang_code: language.code().to_string(),
        lang_dir: language.dir().to_string(),
        txt: ui_text(language),
        has_admin_session: has_admin_session(&state, &headers),
    })
}

async fn info_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(slug): AxumPath<String>,
) -> TemplateResponse<InfoTemplate> {
    let language = resolve_language(&state, &headers);
    let (page_title, page_body) = footer_page_content(&slug);

    TemplateResponse(InfoTemplate {
        lang_code: language.code().to_string(),
        lang_dir: language.dir().to_string(),
        txt: ui_text(language),
        has_admin_session: has_admin_session(&state, &headers),
        page_title: page_title.to_string(),
        page_body: page_body.to_string(),
    })
}

async fn mineral_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(slug): AxumPath<String>,
) -> Result<TemplateResponse<MineralTemplate>, AppError> {
    let language = resolve_language(&state, &headers);
    let mineral = get_mineral(&state, language, &slug)?;
    let request = ReportRequest::default();
    let report = run_agentic_chain(&mineral, &request);

    Ok(TemplateResponse(MineralTemplate {
        lang_code: language.code().to_string(),
        lang_dir: language.dir().to_string(),
        txt: ui_text(language),
        has_admin_session: has_admin_session(&state, &headers),
        mineral,
        request,
        report,
        generated_pdf_path: None,
        generated_html_path: None,
        generation_error: None,
    }))
}

async fn generate_pdf_form(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(slug): AxumPath<String>,
    Form(request): Form<ReportRequest>,
) -> Result<TemplateResponse<MineralTemplate>, AppError> {
    let language = resolve_language(&state, &headers);
    let mineral = get_mineral(&state, language, &slug)?;
    let report = run_agentic_chain(&mineral, &request);

    let (artifacts, generation_error): (Option<GeneratedArtifacts>, Option<String>) =
        match state.pdf_generator.generate_pdf(&report, language).await {
            Ok(paths) => (Some(paths), None),
            Err(err) => (None, Some(err.to_string())),
        };

    Ok(TemplateResponse(MineralTemplate {
        lang_code: language.code().to_string(),
        lang_dir: language.dir().to_string(),
        txt: ui_text(language),
        has_admin_session: has_admin_session(&state, &headers),
        mineral,
        request,
        report,
        generated_pdf_path: artifacts.as_ref().map(|value| value.pdf_path.clone()),
        generated_html_path: artifacts.as_ref().map(|value| value.html_path.clone()),
        generation_error,
    }))
}

async fn generate_pdf_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(slug): AxumPath<String>,
    Json(request): Json<ReportRequest>,
) -> Result<Json<PdfApiResponse>, AppError> {
    let language = resolve_language(&state, &headers);
    let mineral = get_mineral(&state, language, &slug)?;
    let report = run_agentic_chain(&mineral, &request);
    let artifacts = state
        .pdf_generator
        .generate_pdf(&report, language)
        .await
        .with_context(|| format!("failed to generate pdf for slug '{slug}'"))?;

    Ok(Json(PdfApiResponse {
        pdf_path: artifacts.pdf_path,
        html_path: artifacts.html_path,
        summary: report.summary,
    }))
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
        has_admin_session: has_admin_session(&state, &headers),
        error_message: None,
        success_message: None,
        draft_form: MineralFormData::default(),
        has_suggestion: false,
    })
}

async fn admin_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(request): Form<AdminLoginRequest>,
) -> Result<Response, AppError> {
    let language = resolve_language(&state, &headers);
    if request.password != *state.admin_password {
        return Ok(TemplateResponse(AdminTemplate {
            lang_code: language.code().to_string(),
            lang_dir: language.dir().to_string(),
            txt: ui_text(language),
            has_admin_session: false,
            error_message: Some("Invalid admin password.".to_string()),
            success_message: None,
            draft_form: MineralFormData::default(),
            has_suggestion: false,
        })
        .into_response());
    }

    let token = generate_secure_hex(24)?;
    {
        let mut sessions = state
            .admin_sessions
            .lock()
            .map_err(|_| anyhow!("admin session store lock poisoned"))?;
        sessions.insert(token.clone());
    }

    let mut response = TemplateResponse(AdminTemplate {
        lang_code: language.code().to_string(),
        lang_dir: language.dir().to_string(),
        txt: ui_text(language),
        has_admin_session: true,
        error_message: None,
        success_message: Some("Admin session created.".to_string()),
        draft_form: MineralFormData::default(),
        has_suggestion: false,
    })
    .into_response();

    let cookie = format!("admin_session={token}; HttpOnly; Path=/; SameSite=Lax; Max-Age=28800");
    append_set_cookie(&mut response, &cookie)?;
    Ok(response)
}

async fn admin_logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let language = resolve_language(&state, &headers);
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
            drafts.clear();
        }
    }

    let mut response = TemplateResponse(AdminTemplate {
        lang_code: language.code().to_string(),
        lang_dir: language.dir().to_string(),
        txt: ui_text(language),
        has_admin_session: false,
        error_message: None,
        success_message: Some("Admin session closed.".to_string()),
        draft_form: MineralFormData::default(),
        has_suggestion: false,
    })
    .into_response();

    append_set_cookie(
        &mut response,
        "admin_session=; HttpOnly; Path=/; SameSite=Lax; Max-Age=0",
    )?;
    Ok(response)
}

async fn admin_suggest_mineral(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<TemplateResponse<AdminTemplate>, AppError> {
    let language = resolve_language(&state, &headers);
    if !has_admin_session(&state, &headers) {
        return Err(AppError::Unauthorized(
            "Admin session required. Log in at /admin.".to_string(),
        ));
    }

    let input = parse_suggest_multipart(&mut multipart).await?;

    let suggestion = match request_openai_suggestion(&state, &input).await {
        Ok(suggestion) => suggestion,
        Err(err) => {
            error!("admin ai suggestion failed: {err}");
            return Ok(TemplateResponse(AdminTemplate {
                lang_code: language.code().to_string(),
                lang_dir: language.dir().to_string(),
                txt: ui_text(language),
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
        drafts.insert(
            draft_id.clone(),
            AdminDraft {
                image_bytes: input.image_bytes,
                image_ext: input.image_ext,
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
    if !has_admin_session(&state, &headers) {
        return Err(AppError::Unauthorized(
            "Admin session required. Log in at /admin.".to_string(),
        ));
    }

    let image_draft = {
        let drafts = state
            .admin_drafts
            .lock()
            .map_err(|_| anyhow!("admin draft store lock poisoned"))?;
        drafts.get(&request.draft_id).cloned().ok_or_else(|| {
            AppError::BadRequest("draft session not found; run AI suggestion again".to_string())
        })?
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

    let parsed_draft = match parse_publish_request(&request, image_draft) {
        Ok(value) => value,
        Err(err) => {
            return Ok(TemplateResponse(AdminTemplate {
                lang_code: language.code().to_string(),
                lang_dir: language.dir().to_string(),
                txt: ui_text(language),
                has_admin_session: true,
                error_message: Some(err.to_string()),
                success_message: None,
                draft_form: form,
                has_suggestion: true,
            }));
        }
    };

    let (folder_name, translation_stats) = create_mineral_folder(&state, parsed_draft).await?;
    {
        let mut drafts = state
            .admin_drafts
            .lock()
            .map_err(|_| anyhow!("admin draft store lock poisoned"))?;
        drafts.remove(&request.draft_id);
    }
    reload_catalog(&state)?;

    let mut success_message = format!(
        "Mineral published: {}. Localized files: {} translated.",
        folder_name, translation_stats.translated_count
    );
    if !translation_stats.fallback_lang_codes.is_empty() {
        success_message.push_str(" Fallback used for: ");
        success_message.push_str(&translation_stats.fallback_lang_codes.join(", "));
    }

    Ok(TemplateResponse(AdminTemplate {
        lang_code: language.code().to_string(),
        lang_dir: language.dir().to_string(),
        txt: ui_text(language),
        has_admin_session: true,
        error_message: None,
        success_message: Some(success_message),
        draft_form: MineralFormData::default(),
        has_suggestion: false,
    }))
}

fn parse_publish_request(
    request: &PublishMineralRequest,
    image: AdminDraft,
) -> Result<NewMineralDraft, AppError> {
    let common_name = required_string(&request.common_name, "common_name")?;
    let description = required_string(&request.description, "description")?;
    let mineral_family = required_string(&request.mineral_family, "mineral_family")?;
    let formula = required_string(&request.formula, "formula")?;
    let crystal_system = required_string(&request.crystal_system, "crystal_system")?;
    let color = required_string(&request.color, "color")?;
    let streak = required_string(&request.streak, "streak")?;
    let luster = required_string(&request.luster, "luster")?;
    let notes = required_string(&request.notes, "notes")?;

    let hardness_mohs = parse_f32_from_str(&request.hardness_mohs, "hardness_mohs")?;
    let density_g_cm3 = parse_f32_from_str(&request.density_g_cm3, "density_g_cm3")?;
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
        image_bytes: image.image_bytes,
        image_ext: image.image_ext,
    })
}

async fn parse_suggest_multipart(multipart: &mut Multipart) -> Result<SuggestInput, AppError> {
    let mut suggestion_context = String::new();
    let mut image_bytes: Option<Vec<u8>> = None;
    let mut image_ext: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| AppError::BadRequest(format!("invalid multipart payload: {err}")))?
    {
        let name = field.name().unwrap_or_default().to_string();
        if name.is_empty() {
            continue;
        }

        if name == "image" {
            let ext = detect_image_extension(&field)?;
            let bytes = field.bytes().await.map_err(|err| {
                AppError::BadRequest(format!("failed to read image field: {err}"))
            })?;
            if bytes.is_empty() {
                return Err(AppError::BadRequest("image upload is required".to_string()));
            }
            image_ext = Some(ext);
            image_bytes = Some(bytes.to_vec());
            continue;
        }

        let value = field
            .text()
            .await
            .map_err(|err| AppError::BadRequest(format!("failed to read field '{name}': {err}")))?;

        match name.as_str() {
            "suggestion_context" => suggestion_context = value.trim().to_string(),
            _ => {}
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

async fn request_openai_suggestion(
    state: &AppState,
    input: &SuggestInput,
) -> Result<AiMineralSuggestion, AppError> {
    let api_key = state.openai_api_key.as_ref().as_ref().ok_or_else(|| {
        AppError::BadRequest("OPENAI_API_KEY is not configured. Add it to .env.local".to_string())
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
        let body = response.text().await.unwrap_or_default();
        error!("openai api error status={status} body={body}");
        return Err(AppError::BadRequest(format!(
            "OpenAI API returned {status}: {body}"
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

fn get_mineral(state: &AppState, language: Language, slug: &str) -> Result<Mineral, AppError> {
    catalog_for_language(state, language)?
        .by_slug
        .get(slug)
        .cloned()
        .ok_or_else(|| AppError::NotFound(format!("mineral '{slug}' not found")))
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
        .map(|sessions| sessions.contains(&token))
        .unwrap_or(false)
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

fn parse_f32_from_str(value: &str, key: &str) -> Result<f32, AppError> {
    let value = required_string(value, key)?;
    value
        .parse::<f32>()
        .map_err(|_| AppError::BadRequest(format!("'{key}' must be a number")))
}

fn detect_image_extension(field: &axum::extract::multipart::Field<'_>) -> Result<String, AppError> {
    if let Some(file_name) = field.file_name() {
        if let Some(ext) = file_name.rsplit('.').next() {
            let normalized = ext.to_ascii_lowercase();
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
            "image/webp" => Ok("webp".to_string()),
            "image/gif" => Ok("gif".to_string()),
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

async fn create_mineral_folder(
    state: &AppState,
    draft: NewMineralDraft,
) -> Result<(String, TranslationStats), AppError> {
    let family_slug = slugify_family(&draft.mineral_family);
    let minerals_root = state.data_root.join("minerals");

    let folder_name = create_unique_folder_name(&minerals_root, &family_slug)?;
    if !is_valid_mineral_folder_name(&folder_name) {
        return Err(AppError::Internal(anyhow!(
            "generated invalid mineral folder name: {folder_name}"
        )));
    }

    let folder_path = minerals_root.join(&folder_name);
    fs::create_dir_all(&folder_path)
        .await
        .with_context(|| format!("failed to create {}", folder_path.display()))?;

    let image_file = format!("image.{}", draft.image_ext);
    let image_path = folder_path.join(&image_file);
    fs::write(&image_path, draft.image_bytes)
        .await
        .with_context(|| format!("failed to write {}", image_path.display()))?;

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
        image_file: Some(image_file),
    };

    let (localized_records, translation_stats) = build_localized_metadata(state, &metadata).await;
    for (lang_code, localized) in &localized_records {
        let metadata_path = folder_path.join(format!("mineral.{lang_code}.json"));
        write_metadata_file(&metadata_path, localized).await?;
    }

    let fallback_english = localized_records
        .get(Language::En.code())
        .cloned()
        .unwrap_or(metadata);
    write_metadata_file(&folder_path.join("mineral.json"), &fallback_english).await?;

    Ok((folder_name, translation_stats))
}

async fn write_metadata_file(path: &Path, metadata: &MineralDiskRecord) -> Result<(), AppError> {
    let metadata_json = serde_json::to_string_pretty(metadata).map_err(|err| {
        AppError::Internal(anyhow!("failed to serialize mineral metadata: {err}"))
    })?;

    fs::write(path, metadata_json)
        .await
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
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
        "formula": {"type": "string"},
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
        "formula",
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
        model: (*state.openai_model).clone(),
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
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!("OpenAI translation error {status}: {body}"));
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
        formula: translated_or_source(translated.formula, &english.formula),
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

fn footer_page_content(slug: &str) -> (&'static str, &'static str) {
    match slug {
        "contact-us" => (
            "Contact Us",
            "For sales, partnerships, and operations queries, contact support@waajacu.com.",
        ),
        "support" => (
            "Support",
            "For platform issues, account access, and report troubleshooting, open a support request with your mineral record id.",
        ),
        "frequently-asked-questions" => (
            "Frequently Asked Questions",
            "This section answers common questions about publishing minerals, report generation, and account operations.",
        ),
        "legal" => (
            "Legal",
            "Legal notices, jurisdiction terms, platform liabilities, and content governance policies.",
        ),
        "shipping" => (
            "Shipping",
            "Shipping policy, logistics windows, and custody documentation requirements for listed mineral products.",
        ),
        "account" => (
            "Account",
            "Manage account identity, organization profile, login security, and notification settings.",
        ),
        "conflict-free-minerals" => (
            "Conflict Free Minerals",
            "Conflict-free sourcing statement and compliance position across mineral provenance workflows.",
        ),
        "privacy-policy" => (
            "Privacy Policy",
            "Data handling, retention, operational logs, and personal information usage policy.",
        ),
        "terms-of-service" => (
            "Terms of Service",
            "Terms governing access, use, and publication of mineral records on this platform.",
        ),
        "returns-and-refunds" => (
            "Returns and Refunds",
            "Return eligibility, refund conditions, and resolution paths for disputed transactions.",
        ),
        _ => (
            "Information",
            "This page is part of the WAAJACU information center for operational, legal, and customer support topics.",
        ),
    }
}

fn create_unique_folder_name(minerals_root: &Path, family_slug: &str) -> Result<String, AppError> {
    for _ in 0..16 {
        let id = generate_secure_hex(4)?;
        let candidate = format!("mineral.{family_slug}.0x{id}");
        if !minerals_root.join(&candidate).exists() {
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
    let mut file = std::fs::File::open("/dev/urandom")
        .map_err(|err| AppError::Internal(anyhow!("failed to open /dev/urandom: {err}")))?;
    let mut buf = vec![0_u8; byte_len];
    file.read_exact(&mut buf)
        .map_err(|err| AppError::Internal(anyhow!("failed to read random bytes: {err}")))?;

    Ok(buf.iter().map(|b| format!("{b:02x}")).collect::<String>())
}
