use askama::Template;
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};

use crate::{
    agent::MineralReport,
    i18n::{LanguageOption, UiText},
    models::{Mineral, MineralFormData, ReportRequest},
};

pub struct TemplateResponse<T>(pub T);

impl<T> IntoResponse for TemplateResponse<T>
where
    T: Template,
{
    fn into_response(self) -> Response {
        match self.0.render() {
            Ok(html) => Html(html).into_response(),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("template rendering failed: {err}"),
            )
                .into_response(),
        }
    }
}

#[derive(Template)]
#[template(path = "home.html")]
pub struct HomeTemplate {
    pub lang_code: String,
    pub lang_dir: String,
    pub txt: UiText,
    pub language_options: Vec<LanguageOption>,
    pub current_lang_code: &'static str,
}

#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexTemplate {
    pub lang_code: String,
    pub lang_dir: String,
    pub txt: UiText,
    pub minerals: Vec<Mineral>,
    pub has_admin_session: bool,
}

#[derive(Template)]
#[template(path = "mineral.html")]
pub struct MineralTemplate {
    pub lang_code: String,
    pub lang_dir: String,
    pub txt: UiText,
    pub has_admin_session: bool,
    pub mineral: Mineral,
    pub request: ReportRequest,
    pub report: MineralReport,
    pub generated_pdf_path: Option<String>,
    pub generated_html_path: Option<String>,
    pub generation_error: Option<String>,
}

#[derive(Template)]
#[template(path = "admin.html")]
pub struct AdminTemplate {
    pub lang_code: String,
    pub lang_dir: String,
    pub txt: UiText,
    pub has_admin_session: bool,
    pub error_message: Option<String>,
    pub success_message: Option<String>,
    pub draft_form: MineralFormData,
    pub has_suggestion: bool,
}

#[derive(Template)]
#[template(path = "about.html")]
pub struct AboutTemplate {
    pub lang_code: String,
    pub lang_dir: String,
    pub txt: UiText,
    pub has_admin_session: bool,
}

#[derive(Template)]
#[template(path = "info.html")]
pub struct InfoTemplate {
    pub lang_code: String,
    pub lang_dir: String,
    pub txt: UiText,
    pub has_admin_session: bool,
    pub page_title: String,
    pub page_body: String,
}
