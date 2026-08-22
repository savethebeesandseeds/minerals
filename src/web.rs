use askama::Template;
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};

use crate::{
    agent::MineralReport,
    i18n::{LanguageOption, UiText},
    models::{Mineral, MineralFormData, ReportRequest},
    registry::MaterialDetail,
};

#[derive(Debug, Clone)]
pub struct MineralDiscoveryItem {
    pub slug: String,
    pub common_name: String,
    pub mineral_family: String,
    pub formula: String,
}

#[derive(Debug, Clone)]
pub struct AdminReviewEvidenceView {
    pub title: String,
    pub publisher: String,
    pub claim_scope_display: String,
    pub claim_value_display: String,
    pub claim_locator: String,
    pub claim_note: String,
    pub claim_json: String,
    pub confidence_display: String,
    pub review_status_display: String,
    pub canonical_url: String,
    pub source_license: String,
    pub retrieved_at_display: String,
    pub content_hash: String,
}

#[derive(Debug, Clone)]
pub struct AdminReviewFactView {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct AdminReviewCandidateView {
    pub review_id: i64,
    pub revision: usize,
    pub is_update: bool,
    pub current_profile_path: String,
    pub slug: String,
    pub canonical_name: String,
    pub formula: String,
    pub mineral_family: String,
    pub description: String,
    pub cas_number: String,
    pub synonyms: Vec<String>,
    pub identifiers: Vec<AdminReviewFactView>,
    pub properties: Vec<AdminReviewFactView>,
    pub safety: Vec<AdminReviewFactView>,
    pub record_license: String,
    pub scientific_status_display: String,
    pub quality_display: String,
    pub source_display: String,
    pub submitted_at_display: String,
    pub evidence: Vec<AdminReviewEvidenceView>,
    pub payload_json: String,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct AdminIngestionCountView {
    pub create_count: usize,
    pub adopt_count: usize,
    pub update_count: usize,
    pub unchanged_count: usize,
    pub conflict_count: usize,
    pub missing_count: usize,
    pub identity_critical_warning_count: usize,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AdminIngestionAnomalyView {
    pub source_record_id: String,
    pub proposed_slug: String,
    pub resolved_slug: String,
    pub classification_display: String,
    pub severity_display: String,
    pub code: String,
    pub message: String,
    pub critical_formula_change: bool,
    pub critical_validity_change: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AdminIngestionReviewSampleView {
    pub source_record_id: String,
    pub canonical_name: String,
    pub formula: String,
    pub nomenclature_status_display: String,
    pub is_valid_species: bool,
}

#[derive(Debug, Clone)]
pub struct AdminIngestionDecisionView {
    pub batch_id: String,
    pub manifest_hash: String,
    pub report_hash: String,
    pub base_batch_id: String,
    pub release_version: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AdminIngestionBatchView {
    pub batch_id: String,
    pub status_display: String,
    pub status_note: String,
    pub is_receiving: bool,
    pub is_ready: bool,
    pub needs_attention: bool,
    pub is_approved: bool,
    pub is_rejected: bool,
    pub can_approve: bool,
    pub manifest_schema_version: u32,
    pub dataset_key: String,
    pub dataset_title: String,
    pub source_key: String,
    pub source_url_display: String,
    pub source_url_href: Option<String>,
    pub source_license: String,
    pub attribution_complete: bool,
    pub attribution_party: String,
    pub attribution_work_title: String,
    pub attribution_work_url_display: String,
    pub attribution_work_url_href: Option<String>,
    pub attribution_license_url_display: String,
    pub attribution_license_url_href: Option<String>,
    pub attribution_changes_notice: String,
    pub attribution_no_endorsement_notice: String,
    pub attribution_derived_output_license_spdx: String,
    pub release_version: String,
    pub released_at_display: String,
    pub retrieved_at_display: String,
    pub parser_name: String,
    pub parser_version: String,
    pub parser_code_revision: String,
    pub parser_configuration_hash: String,
    pub artifact_hash: String,
    pub manifest_hash: String,
    pub report_hash: String,
    pub records_hash: String,
    pub base_batch_display: String,
    pub received_chunk_count: usize,
    pub expected_chunk_count: usize,
    pub received_record_count: usize,
    pub expected_record_count: usize,
    pub chunk_progress_percent: usize,
    pub record_progress_percent: usize,
    pub report_summary: Option<AdminIngestionCountView>,
    pub review_samples: Vec<AdminIngestionReviewSampleView>,
    pub anomaly_samples: Vec<AdminIngestionAnomalyView>,
    pub created_at_display: String,
    pub finalized_at_display: String,
    pub decision: Option<AdminIngestionDecisionView>,
}

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
    pub has_admin_session: bool,
    pub language_options: Vec<LanguageOption>,
    pub current_lang_code: &'static str,
}

#[derive(Template)]
#[template(path = "all_minerals.html")]
pub struct AllMineralsTemplate {
    pub lang_code: String,
    pub lang_dir: String,
    pub txt: UiText,
    pub has_admin_session: bool,
    pub minerals: Vec<MineralDiscoveryItem>,
    pub query: String,
    pub total_results: usize,
    pub published_count: usize,
    pub current_page: usize,
    pub total_pages: usize,
    pub page_start: usize,
    pub page_end: usize,
    pub has_previous_page: bool,
    pub previous_page: usize,
    pub has_next_page: bool,
    pub next_page: usize,
}

#[derive(Template)]
#[template(path = "material_record.html")]
pub struct RegistryMineralTemplate {
    pub lang_code: String,
    pub lang_dir: String,
    pub txt: UiText,
    pub has_admin_session: bool,
    pub material: MaterialDetail,
}

#[derive(Template)]
#[template(path = "mineral.html")]
pub struct MineralTemplate {
    pub lang_code: String,
    pub lang_dir: String,
    pub txt: UiText,
    pub has_admin_session: bool,
    pub mineral: Mineral,
    pub registry: Option<MaterialDetail>,
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
#[template(path = "admin_ingestion.html")]
#[allow(dead_code)]
pub struct AdminIngestionTemplate {
    pub lang_code: String,
    pub lang_dir: String,
    pub txt: UiText,
    pub has_admin_session: bool,
    pub error_message: Option<String>,
    pub success_message: Option<String>,
    pub published_mineral_count: usize,
    pub batches: Vec<AdminIngestionBatchView>,
    pub total_results: usize,
    pub current_page: usize,
    pub total_pages: usize,
    pub page_start: usize,
    pub page_end: usize,
    pub has_previous_page: bool,
    pub previous_page: usize,
    pub has_next_page: bool,
    pub next_page: usize,
}

#[derive(Template)]
#[template(path = "admin_reviews.html")]
pub struct ReviewQueueTemplate {
    pub lang_code: String,
    pub lang_dir: String,
    pub txt: UiText,
    pub has_admin_session: bool,
    pub error_message: Option<String>,
    pub success_message: Option<String>,
    pub reviews: Vec<AdminReviewCandidateView>,
    pub total_results: usize,
    pub current_page: usize,
    pub total_pages: usize,
    pub page_start: usize,
    pub page_end: usize,
    pub has_previous_page: bool,
    pub previous_page: usize,
    pub has_next_page: bool,
    pub next_page: usize,
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
#[template(path = "map.html")]
pub struct MapTemplate {
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

#[cfg(test)]
mod tests {
    use askama::Template;

    use super::{
        AdminIngestionAnomalyView, AdminIngestionBatchView, AdminIngestionCountView,
        AdminIngestionDecisionView, AdminIngestionReviewSampleView, AdminIngestionTemplate,
        AdminReviewCandidateView, AdminReviewEvidenceView, AdminReviewFactView,
        RegistryMineralTemplate, ReviewQueueTemplate,
    };
    use crate::i18n::{ui_text, Language};
    use crate::registry::{EvidenceAttributionSummary, EvidenceSummary, MaterialDetail};
    use serde_json::json;

    #[test]
    fn review_queue_escapes_imported_content_and_posts_exact_revision_id() {
        let rendered = ReviewQueueTemplate {
            lang_code: "en".to_string(),
            lang_dir: "ltr".to_string(),
            txt: ui_text(Language::En),
            has_admin_session: true,
            error_message: None,
            success_message: None,
            reviews: vec![AdminReviewCandidateView {
                review_id: 42,
                revision: 3,
                is_update: true,
                current_profile_path: "/minerals/mineral.silicates.unsafe".to_string(),
                slug: "mineral.silicates.unsafe".to_string(),
                canonical_name: "<script>unsafe()</script>".to_string(),
                formula: "SiO2".to_string(),
                mineral_family: "Silicates".to_string(),
                description: "Imported candidate".to_string(),
                cas_number: "14808-60-7".to_string(),
                synonyms: vec!["Rock crystal".to_string()],
                identifiers: vec![AdminReviewFactView {
                    name: "IMA symbol".to_string(),
                    value: "Qz".to_string(),
                }],
                properties: vec![AdminReviewFactView {
                    name: "Hardness".to_string(),
                    value: "7".to_string(),
                }],
                safety: vec![AdminReviewFactView {
                    name: "Handling".to_string(),
                    value: "Avoid respirable dust".to_string(),
                }],
                record_license: "CC0-1.0".to_string(),
                scientific_status_display: "Sources attached".to_string(),
                quality_display: "72%".to_string(),
                source_display: "Research import".to_string(),
                submitted_at_display: "2026-08-15 12:00 UTC".to_string(),
                evidence: vec![AdminReviewEvidenceView {
                    title: "Reference <b>title</b>".to_string(),
                    publisher: "Publisher".to_string(),
                    claim_scope_display: "Identity".to_string(),
                    claim_value_display: "Quartz".to_string(),
                    claim_locator: "Section 2".to_string(),
                    claim_note: "Cross-checked".to_string(),
                    claim_json: "{\n  \"value\": \"Quartz\"\n}".to_string(),
                    confidence_display: "90%".to_string(),
                    review_status_display: "Reviewed".to_string(),
                    canonical_url: "https://example.org/source".to_string(),
                    source_license: "CC-BY-4.0".to_string(),
                    retrieved_at_display: "2026-08-15 11:00 UTC".to_string(),
                    content_hash: "sha256:abc123".to_string(),
                }],
                payload_json: "{\n  \"description\": \"</pre><script>payload()</script>\"\n}"
                    .to_string(),
            }],
            total_results: 1,
            current_page: 1,
            total_pages: 1,
            page_start: 1,
            page_end: 1,
            has_previous_page: false,
            previous_page: 1,
            has_next_page: false,
            next_page: 1,
        }
        .render()
        .expect("review queue template");

        assert!(rendered.contains("&lt;script&gt;unsafe()&lt;/script&gt;"));
        assert!(rendered.contains("Reference &lt;b&gt;title&lt;/b&gt;"));
        assert!(!rendered.contains("<script>unsafe()</script>"));
        assert!(rendered.contains("&lt;/pre&gt;&lt;script&gt;payload()&lt;/script&gt;"));
        assert!(!rendered.contains("</pre><script>payload()</script>"));
        assert!(rendered.contains("Updates existing profile"));
        assert!(rendered.contains("Review ID 42"));
        assert!(rendered.contains("href=\"/minerals/mineral.silicates.unsafe\""));
        assert!(rendered.contains("Complete staged payload"));
        assert!(rendered.contains("Complete claim"));
        assert!(rendered.contains("14808-60-7"));
        assert!(rendered.contains("Rock crystal"));
        assert!(rendered.contains("CC0-1.0"));
        assert!(rendered.contains("Section 2"));
        assert!(rendered.contains("Cross-checked"));
        assert!(rendered.contains("CC-BY-4.0"));
        assert!(rendered.contains("2026-08-15 11:00 UTC"));
        assert!(rendered.contains("sha256:abc123"));
        assert!(rendered.contains("action=\"/admin/minerals/review\""));
        assert!(rendered.contains("name=\"review_id\" value=\"42\""));
        assert!(rendered.contains("name=\"action\" value=\"approve\""));
        assert!(rendered.contains("name=\"action\" value=\"reject\""));
        assert!(rendered.contains("name=\"operator_note\""));
    }

    #[test]
    fn ingestion_page_is_simple_escapes_data_and_posts_exact_release_coordinates() {
        let mut page = AdminIngestionTemplate {
            lang_code: "en".to_string(),
            lang_dir: "ltr".to_string(),
            txt: ui_text(Language::En),
            has_admin_session: true,
            error_message: None,
            success_message: None,
            published_mineral_count: 6_226,
            batches: vec![AdminIngestionBatchView {
                batch_id: "batch_unsafe<script>batch()</script>".to_string(),
                status_display: "Needs attention".to_string(),
                status_note: "Review <strong>carefully</strong>".to_string(),
                is_receiving: false,
                is_ready: true,
                needs_attention: false,
                is_approved: false,
                is_rejected: false,
                can_approve: true,
                manifest_schema_version: 2,
                dataset_key: "ima.primary".to_string(),
                dataset_title: "<script>dataset()</script>".to_string(),
                source_key: "ima".to_string(),
                source_url_display: "https://example.org/<unsafe>".to_string(),
                source_url_href: None,
                source_license: "CC-BY-4.0".to_string(),
                attribution_complete: true,
                attribution_party: "International Mineral Authority <unsafe>".to_string(),
                attribution_work_title: "Authoritative list <work>".to_string(),
                attribution_work_url_display: "https://example.org/work".to_string(),
                attribution_work_url_href: Some("https://example.org/work".to_string()),
                attribution_license_url_display: "https://creativecommons.org/licenses/by/4.0/"
                    .to_string(),
                attribution_license_url_href: Some(
                    "https://creativecommons.org/licenses/by/4.0/".to_string(),
                ),
                attribution_changes_notice: "Extracted and normalized <changes>".to_string(),
                attribution_no_endorsement_notice: "The authority does not endorse <notice>"
                    .to_string(),
                attribution_derived_output_license_spdx: "CC-BY-4.0".to_string(),
                release_version: "2026.1".to_string(),
                released_at_display: "2026-08-15".to_string(),
                retrieved_at_display: "2026-08-15 12:00 UTC".to_string(),
                parser_name: "parser <next>".to_string(),
                parser_version: "1.0".to_string(),
                parser_code_revision: "abc123".to_string(),
                parser_configuration_hash: "sha256:config".to_string(),
                artifact_hash: "sha256:artifact".to_string(),
                manifest_hash: "sha256:manifest".to_string(),
                report_hash: "sha256:report".to_string(),
                records_hash: "sha256:records".to_string(),
                base_batch_display: "batch_base".to_string(),
                received_chunk_count: 2,
                expected_chunk_count: 2,
                received_record_count: 500,
                expected_record_count: 500,
                chunk_progress_percent: 100,
                record_progress_percent: 100,
                report_summary: Some(AdminIngestionCountView {
                    create_count: 100,
                    adopt_count: 20,
                    update_count: 10,
                    unchanged_count: 360,
                    conflict_count: 4,
                    missing_count: 5,
                    identity_critical_warning_count: 1,
                }),
                review_samples: vec![AdminIngestionReviewSampleView {
                    source_record_id: "</strong><script>sample()</script>".to_string(),
                    canonical_name: "Quartz <img src=x onerror=sample()>".to_string(),
                    formula: "SiO<sub>2</sub>".to_string(),
                    nomenclature_status_display: "Approved <script>status()</script>".to_string(),
                    is_valid_species: true,
                }],
                anomaly_samples: vec![AdminIngestionAnomalyView {
                    source_record_id: "</strong><script>anomaly()</script>".to_string(),
                    proposed_slug: "mineral.unsafe".to_string(),
                    resolved_slug: "mineral.safe".to_string(),
                    classification_display: "Conflict".to_string(),
                    severity_display: "Blocker".to_string(),
                    code: "identity_conflict".to_string(),
                    message: "Formula changed <img src=x onerror=unsafe()>".to_string(),
                    critical_formula_change: true,
                    critical_validity_change: false,
                }],
                created_at_display: "2026-08-15 12:00 UTC".to_string(),
                finalized_at_display: "2026-08-15 12:05 UTC".to_string(),
                decision: Some(AdminIngestionDecisionView {
                    batch_id: "batch_exact".to_string(),
                    manifest_hash: "sha256:manifest".to_string(),
                    report_hash: "sha256:report".to_string(),
                    base_batch_id: "batch_base".to_string(),
                    release_version: "2026.1".to_string(),
                }),
            }],
            total_results: 1,
            current_page: 1,
            total_pages: 1,
            page_start: 1,
            page_end: 1,
            has_previous_page: false,
            previous_page: 1,
            has_next_page: false,
            next_page: 1,
        };
        let rendered = page.render().expect("ingestion template");

        assert!(rendered.contains("&lt;script&gt;dataset()&lt;/script&gt;"));
        assert!(!rendered.contains("sample()"));
        assert!(!rendered.contains("parser &lt;next&gt;"));
        assert!(!rendered.contains("sha256:config"));
        assert!(!rendered.contains("sha256:artifact"));
        assert!(!rendered.contains("sha256:records"));
        assert!(!rendered.contains("anomaly()"));
        assert!(rendered.contains("International Mineral Authority &lt;unsafe&gt;"));
        assert!(rendered.contains("Authoritative list &lt;work&gt;"));
        assert!(rendered.contains(">Authoritative list &lt;work&gt;</a>"));
        assert!(rendered.contains(">CC-BY-4.0</a>"));
        assert!(rendered.contains("Extracted and normalized &lt;changes&gt;"));
        assert!(rendered.contains("does not endorse &lt;notice&gt;"));
        assert!(!rendered.contains("<script>dataset()</script>"));
        assert!(rendered.contains("action=\"/admin/ingestion/batches/batch_exact/decision\""));
        assert!(rendered.contains("name=\"manifest_hash\" value=\"sha256:manifest\""));
        assert!(rendered.contains("name=\"report_hash\" value=\"sha256:report\""));
        assert!(rendered.contains("name=\"base_batch_id\" value=\"batch_base\""));
        assert!(rendered.contains("name=\"release_version\" value=\"2026.1\""));
        assert!(rendered.contains("name=\"release_confirmation\""));
        assert!(rendered.contains("name=\"warning_acknowledged\" value=\"1\" required"));
        assert!(rendered.contains("name=\"operator_note\" maxlength=\"2000\""));
        assert!(rendered.contains("name=\"action\" value=\"approve\""));
        assert!(rendered.contains("name=\"action\" value=\"reject\""));

        let template = include_str!("../static/admin_ingestion.html");
        assert!(!template.contains("data-ingestion-create-form"));
        assert!(!template.contains("ingestion-manifest-json"));
        assert!(!template.contains("data-ingestion-chunk-form"));
        assert!(!template.contains("data-ingestion-finalize-form"));
        assert!(!template.contains("batch.parser_"));
        assert!(!template.contains("batch.review_samples"));
        assert!(template.contains("{% if batch.can_approve %}"));
        assert!(!template.contains("innerHTML"));
        assert!(!template.contains("Authorization"));
        assert!(!template.contains("INGESTION_API_TOKEN"));

        page.batches[0].is_ready = false;
        page.batches[0].needs_attention = true;
        page.batches[0].can_approve = false;
        let attention = page.render().expect("needs-attention ingestion template");
        assert!(!attention.contains("anomaly()"));
        assert!(attention.contains("Formula changed &lt;img src=x onerror=unsafe()&gt;"));
        assert!(!attention.contains("<script>anomaly()</script>"));
        assert!(!attention.contains("name=\"action\" value=\"approve\""));
        assert!(attention.contains("name=\"action\" value=\"reject\""));

        page.batches[0].is_ready = true;
        page.batches[0].needs_attention = false;
        page.batches[0].attribution_complete = false;
        page.batches[0].can_approve = false;
        let historical = page.render().expect("historical ingestion template");
        assert!(historical.contains(page.txt.ingestion.historical_attribution_missing));
        assert!(!historical.contains("name=\"action\" value=\"approve\""));
        assert!(historical.contains("name=\"action\" value=\"reject\""));
    }

    #[test]
    fn admin_withdrawal_form_requires_a_reason_and_exact_slug() {
        let template = include_str!("../static/admin.html");

        assert!(template.contains("action=\"/admin/minerals/withdraw\""));
        assert!(template.contains("<input name=\"slug\" autocomplete=\"off\" required"));
        assert!(template
            .contains("<textarea name=\"operator_note\" maxlength=\"2000\" required></textarea>"));
        assert!(template.contains("supersedes every pending revision"));
    }

    #[test]
    fn public_detail_renders_linked_attribution_changes_and_no_endorsement() {
        let rendered = RegistryMineralTemplate {
            lang_code: "en".to_string(),
            lang_dir: "ltr".to_string(),
            txt: ui_text(Language::En),
            has_admin_session: false,
            material: MaterialDetail {
                public_id: "mat_attribution".to_string(),
                slug: "mineral.attribution".to_string(),
                record_type: "mineral".to_string(),
                canonical_name: "Attributed mineral".to_string(),
                formula: "X".to_string(),
                description: String::new(),
                mineral_family: String::new(),
                nomenclature_status: "approved".to_string(),
                is_valid_species: true,
                official_facts: Default::default(),
                verification_status: "sourced".to_string(),
                data_quality_score: 0.8,
                source_kind: "registry_import".to_string(),
                registry_authoritative: true,
                license_spdx: "CC-BY-SA-3.0".to_string(),
                cas_number: None,
                identifiers: Vec::new(),
                properties: Vec::new(),
                safety: Vec::new(),
                legacy_profile_path: None,
                properties_json: "{}".to_string(),
                safety_json: "{}".to_string(),
                evidence: vec![EvidenceSummary {
                    title: "Source title".to_string(),
                    publisher: "IMA-CNMNC".to_string(),
                    canonical_url: "https://example.org/source.pdf".to_string(),
                    license_spdx: "CC-BY-SA-3.0".to_string(),
                    claim_scope: "identity".to_string(),
                    claim: json!({"value": "Attributed mineral"}),
                    claim_label: "Identity".to_string(),
                    claim_summary: "Attributed mineral".to_string(),
                    confidence: 0.95,
                    confidence_percent: 95,
                    review_status: "reviewed".to_string(),
                    retrieved_at: "2026-08-15T10:00:00Z".to_string(),
                    content_hash: format!("sha256:{}", "1".repeat(64)),
                    attribution: Some(EvidenceAttributionSummary {
                        attribution_party: "Authority <credit>".to_string(),
                        work_title: "Exact <work> title".to_string(),
                        work_url: "https://example.org/work".to_string(),
                        license_url: "https://creativecommons.org/licenses/by-sa/3.0/".to_string(),
                        changes_notice: "Extracted and normalized <changes>.".to_string(),
                        no_endorsement_notice: "The authority does not endorse <notice>."
                            .to_string(),
                        derived_output_license_spdx: "CC-BY-SA-3.0".to_string(),
                    }),
                }],
                offers: Vec::new(),
            },
        }
        .render()
        .expect("public attributed detail");

        assert!(rendered.contains("id=\"attribution\""));
        assert!(rendered.contains("class=\"panel buy-panel\""));
        assert!(rendered.contains("background: #f1e5cd"));
        assert!(rendered.contains("background: transparent"));
        assert!(!rendered.contains("<details>"));
        assert!(rendered.contains("Authority &lt;credit&gt;"));
        assert!(rendered.contains("Exact &lt;work&gt; title"));
        assert!(rendered.contains("Extracted and normalized &lt;changes&gt;."));
        assert!(rendered.contains("does not endorse &lt;notice&gt;."));
        assert!(rendered.contains("rel=\"license noopener noreferrer\""));
        assert!(rendered.contains("CC-BY-SA-3.0"));

        let facts_position = rendered.find("facts-column").expect("facts column");
        let buy_position = rendered.find("buy-column").expect("buy column");
        let research_position = rendered.find("research-column").expect("research column");
        assert!(facts_position < buy_position);
        assert!(buy_position < research_position);

        let legacy_template = include_str!("../static/mineral.html");
        for field in [
            "attribution.attribution_party",
            "attribution.work_title",
            "attribution.work_url",
            "attribution.license_url",
            "attribution.changes_notice",
            "attribution.no_endorsement_notice",
            "attribution.derived_output_license_spdx",
        ] {
            assert!(
                legacy_template.contains(field),
                "legacy template omits {field}"
            );
        }
    }
}
