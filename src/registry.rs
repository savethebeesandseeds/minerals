use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    time::Duration,
};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration as ChronoDuration, NaiveDateTime, SecondsFormat, Utc};
use reqwest::Url;
use ring::digest::{digest, Context as DigestContext, SHA256};
use rusqlite::{
    params, Connection, DatabaseName, OptionalExtension, Transaction, TransactionBehavior,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

const DATABASE_FILE: &str = "minerals.db";
const REGISTRY_MIGRATION: &str = "registry_v1";
const MINERAL_REVIEW_MIGRATION: &str = "mineral_review_workflow_v1";
const EVIDENCE_SNAPSHOT_MIGRATION: &str = "material_evidence_snapshots_v1";
const EVIDENCE_ATTRIBUTION_SNAPSHOT_MIGRATION: &str = "material_evidence_attribution_snapshots_v2";
const MINERAL_WITHDRAWAL_MIGRATION: &str = "mineral_withdrawal_v1";
const REGISTRY_IMAGE_DETACH_MIGRATION: &str = "registry_import_image_detach_v1";
const BULK_MINERAL_INGESTION_MIGRATION: &str = "bulk_mineral_ingestion_v1";
const BULK_MINERAL_INGESTION_SAFETY_MIGRATION: &str = "bulk_mineral_ingestion_safety_v2";
const MINERAL_DATASET_FACTS_MIGRATION: &str = "mineral_dataset_facts_v1";
const MAX_SEARCH_RESULTS: usize = 100;
pub const MINERAL_INGESTION_SCHEMA_VERSION: u32 = 2;
pub const MAX_MINERAL_INGESTION_CHUNK_ITEMS: usize = 500;
const MAX_MINERAL_INGESTION_RECORDS: usize = 100_000;
const MAX_MINERAL_INGESTION_CHUNKS: usize = 10_000;
const DEFAULT_INGESTION_BATCH_MAX_BYTES: u64 = 64 * 1024 * 1024;
const DEFAULT_INGESTION_QUARANTINE_MAX_BYTES: u64 = 512 * 1024 * 1024;
const DEFAULT_INGESTION_ABANDONED_HOURS: u64 = 14 * 24;
const MIN_INGESTION_BATCH_MAX_BYTES: u64 = 1024 * 1024;
const MAX_INGESTION_BATCH_MAX_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_INGESTION_QUARANTINE_MAX_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MAX_INGESTION_ABANDONED_HOURS: u64 = 365 * 24;
const PRE_ACTIVATION_BACKUP_RETENTION: usize = 10;
const MAX_DESCRIPTION_EXCERPT_CHARS: usize = 320;
const MAX_CLAIM_SUMMARY_CHARS: usize = 600;
const ACTIVE_OFFER_PREDICATE: &str = r#"
    o.active = 1
    AND p.active = 1
    AND p.verification_status <> 'suspended'
    AND (o.expires_at IS NULL OR datetime(o.expires_at) > CURRENT_TIMESTAMP)
"#;

#[derive(Debug, Clone, Copy)]
struct MineralIngestionLimits {
    batch_max_bytes: u64,
    quarantine_max_bytes: u64,
    abandoned_hours: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegistryStats {
    pub material_count: usize,
    pub mineral_count: usize,
    pub compound_count: usize,
    pub evidence_count: usize,
    pub active_offer_count: usize,
    pub provider_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MaterialSearchItem {
    pub public_id: String,
    pub slug: String,
    pub record_type: String,
    pub canonical_name: String,
    pub formula: String,
    pub description: String,
    pub description_excerpt: String,
    pub mineral_family: String,
    pub nomenclature_status: String,
    pub is_valid_species: bool,
    pub verification_status: String,
    pub data_quality_score: f64,
    pub evidence_count: usize,
    pub active_offer_count: usize,
    pub detail_path: String,
    /// Stable public location where source attribution for this result is
    /// rendered. Absent when the record has no attributed evidence snapshot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribution_path: Option<String>,
    /// Compact license signal for list/API consumers. The complete attribution
    /// and change notice live at `attribution_path`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribution_license_spdx: Option<String>,
    /// Internal display precedence signal. Registry-approved content is
    /// authoritative over the legacy catalog's localized projection, but this
    /// implementation detail is not part of the public API contract.
    #[serde(skip_serializing)]
    pub registry_authoritative: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MaterialSearchPage {
    pub items: Vec<MaterialSearchItem>,
    pub total_count: usize,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MaterialFact {
    #[serde(skip_serializing)]
    pub key: String,
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvidenceSummary {
    pub title: String,
    pub publisher: String,
    pub canonical_url: String,
    pub license_spdx: String,
    pub claim_scope: String,
    pub claim: Value,
    pub claim_label: String,
    pub claim_summary: String,
    pub confidence: f64,
    pub confidence_percent: u8,
    pub review_status: String,
    pub retrieved_at: String,
    pub content_hash: String,
    pub attribution: Option<EvidenceAttributionSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvidenceAttributionSummary {
    pub attribution_party: String,
    pub work_title: String,
    pub work_url: String,
    pub license_url: String,
    pub changes_notice: String,
    pub no_endorsement_notice: String,
    pub derived_output_license_spdx: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderOffer {
    pub provider_name: String,
    pub provider_slug: String,
    pub provider_verification_status: String,
    pub provider_trust_score: f64,
    pub title: String,
    pub product_url: String,
    pub price_display: String,
    pub pricing_basis: String,
    pub pricing_basis_display: String,
    pub minimum_order_display: String,
    pub stock_status: String,
    pub purity_text: String,
    pub grade: String,
    pub origin_country_code: String,
    pub verification_status: String,
    pub last_checked_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MaterialDetail {
    pub public_id: String,
    pub slug: String,
    pub record_type: String,
    pub canonical_name: String,
    pub formula: String,
    pub description: String,
    pub mineral_family: String,
    pub nomenclature_status: String,
    pub is_valid_species: bool,
    pub official_facts: MineralOfficialFacts,
    pub verification_status: String,
    pub data_quality_score: f64,
    pub source_kind: String,
    pub registry_authoritative: bool,
    pub license_spdx: String,
    pub cas_number: Option<String>,
    pub identifiers: Vec<MaterialFact>,
    pub properties: Vec<MaterialFact>,
    pub safety: Vec<MaterialFact>,
    pub legacy_profile_path: Option<String>,
    pub properties_json: String,
    pub safety_json: String,
    pub evidence: Vec<EvidenceSummary>,
    pub offers: Vec<ProviderOffer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MaterialImport {
    pub slug: String,
    pub record_type: String,
    pub canonical_name: String,
    pub formula: String,
    pub description: String,
    pub mineral_family: String,
    pub cas_number: Option<String>,
    pub identifiers: Value,
    pub synonyms: Vec<String>,
    pub properties: Value,
    pub safety: Value,
    pub verification_status: String,
    pub data_quality_score: f64,
    pub license_spdx: String,
    pub sources: Vec<EvidenceImport>,
}

impl Default for MaterialImport {
    fn default() -> Self {
        Self {
            slug: String::new(),
            record_type: "mineral".to_string(),
            canonical_name: String::new(),
            formula: String::new(),
            description: String::new(),
            mineral_family: String::new(),
            cas_number: None,
            identifiers: json!({}),
            synonyms: Vec::new(),
            properties: json!({}),
            safety: json!({}),
            verification_status: "draft".to_string(),
            data_quality_score: 0.0,
            license_spdx: "NOASSERTION".to_string(),
            sources: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EvidenceImport {
    pub url: String,
    pub title: String,
    pub publisher: String,
    pub license_spdx: String,
    pub claim_scope: String,
    pub claim: Value,
    pub confidence: f64,
    pub review_status: String,
    pub retrieved_at: String,
    pub content_hash: String,
}

impl Default for EvidenceImport {
    fn default() -> Self {
        Self {
            url: String::new(),
            title: String::new(),
            publisher: String::new(),
            license_spdx: "NOASSERTION".to_string(),
            claim_scope: "identity".to_string(),
            claim: json!({}),
            confidence: 0.5,
            review_status: "unreviewed".to_string(),
            retrieved_at: String::new(),
            content_hash: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportSummary {
    pub run_id: i64,
    /// Total records accepted by the ingestion transaction. Mineral records
    /// are staged, not published, until their review ids are approved.
    pub imported_count: usize,
    pub evidence_count: usize,
    pub queued_count: usize,
    pub published_count: usize,
    pub review_ids: Vec<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MineralReviewStatus {
    Pending,
    Approved,
    Rejected,
    Superseded,
}

impl MineralReviewStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Superseded => "superseded",
        }
    }

    fn from_database(value: &str) -> Result<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            "superseded" => Ok(Self::Superseded),
            _ => bail!("unsupported mineral review status '{value}'"),
        }
    }
}

/// An immutable imported mineral revision awaiting an operator decision.
///
/// `review_id` identifies the exact revision. Callers should retain it and
/// approve or reject by id rather than by slug so a newer import cannot be
/// mistaken for the revision an operator actually inspected.
#[derive(Debug, Clone, Serialize)]
pub struct PendingMineralReview {
    pub review_id: i64,
    pub revision: usize,
    pub source_label: String,
    pub submitted_at: String,
    pub record: MaterialImport,
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingMineralReviewPage {
    pub items: Vec<PendingMineralReview>,
    pub total_count: usize,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MineralReviewOutcome {
    pub review_id: i64,
    pub revision: usize,
    pub mineral_slug: String,
    pub status: MineralReviewStatus,
    pub operator_note: String,
    pub submitted_at: String,
    pub reviewed_at: Option<String>,
    /// True only for the caller that performed the pending -> terminal state
    /// transition. Repeating the same decision is a successful no-op.
    pub changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderImport {
    pub slug: String,
    pub name: String,
    pub website_url: String,
    pub network_kind: String,
    pub country_code: String,
    pub verification_status: String,
    pub trust_score: f64,
    pub offers: Vec<OfferImport>,
}

impl Default for ProviderImport {
    fn default() -> Self {
        Self {
            slug: String::new(),
            name: String::new(),
            website_url: String::new(),
            network_kind: "direct".to_string(),
            country_code: String::new(),
            verification_status: "unverified".to_string(),
            trust_score: 0.0,
            offers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OfferImport {
    #[serde(rename = "mineral_slug", alias = "material_slug")]
    pub material_slug: String,
    pub external_id: String,
    pub title: String,
    pub product_url: String,
    pub currency_code: String,
    pub price_minor: Option<i64>,
    pub currency_exponent: i64,
    pub pricing_basis: String,
    pub minimum_order_quantity: Option<f64>,
    pub minimum_order_unit: String,
    pub available_quantity: Option<f64>,
    pub available_quantity_unit: String,
    pub stock_status: String,
    pub purity_text: String,
    pub grade: String,
    pub origin_country_code: String,
    pub provider_claims: Value,
    pub evidence_url: Option<String>,
    pub verification_status: String,
    pub last_checked_at: String,
    pub expires_at: Option<String>,
    pub active: bool,
}

impl Default for OfferImport {
    fn default() -> Self {
        Self {
            material_slug: String::new(),
            external_id: String::new(),
            title: String::new(),
            product_url: String::new(),
            currency_code: String::new(),
            price_minor: None,
            currency_exponent: 2,
            pricing_basis: "quote".to_string(),
            minimum_order_quantity: None,
            minimum_order_unit: String::new(),
            available_quantity: None,
            available_quantity_unit: String::new(),
            stock_status: "unknown".to_string(),
            purity_text: String::new(),
            grade: String::new(),
            origin_country_code: String::new(),
            provider_claims: json!({}),
            evidence_url: None,
            verification_status: "provider_claim".to_string(),
            last_checked_at: String::new(),
            expires_at: None,
            active: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderImportSummary {
    pub provider_slug: String,
    pub offers_upserted: usize,
}

/// A machine-readable problem raised by the release ingestion state machine.
/// HTTP callers can downcast `anyhow::Error` to this type and map conflicts to
/// 409 without inspecting presentation text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MineralIngestionProblemKind {
    Invalid,
    NotFound,
    Conflict,
}

#[derive(Debug, Error)]
#[error("{code}: {message}")]
pub struct MineralIngestionProblem {
    pub kind: MineralIngestionProblemKind,
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MineralDatasetManifest {
    pub schema_version: u32,
    pub dataset: MineralDatasetDescriptor,
    pub source: MineralSourceDescriptor,
    pub release: MineralReleaseDescriptor,
    pub retrieval: MineralRetrievalDescriptor,
    pub artifact: MineralArtifactDescriptor,
    pub parser: MineralParserDescriptor,
    pub policy: MineralIngestionPolicy,
    pub expected_record_count: usize,
    pub expected_chunk_count: usize,
    /// SHA-256 of the canonical JSON array containing every item in chunk
    /// index/item order. See `canonical_mineral_records_hash`.
    pub records_sha256: String,
    pub snapshot_kind: MineralSnapshotKind,
    /// Exact previously approved batch for this dataset, or null for the first
    /// release. It is checked again inside the activation transaction.
    pub base_batch_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MineralDatasetDescriptor {
    pub key: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MineralSourceDescriptor {
    pub key: String,
    pub url: String,
    pub license_spdx: String,
    /// Optional only so immutable historical schema-v1 terminal manifests can
    /// still be deserialized and inspected. Every accepted schema-v2 manifest
    /// must carry a complete attribution object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<MineralSourceAttribution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MineralSourceAttribution {
    pub attribution_party: String,
    pub work_title: String,
    pub work_url: String,
    pub license_url: String,
    pub changes_notice: String,
    pub no_endorsement_notice: String,
    pub derived_output_license_spdx: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MineralReleaseDescriptor {
    pub version: String,
    pub released_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MineralRetrievalDescriptor {
    pub retrieved_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MineralArtifactDescriptor {
    pub url: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MineralParserDescriptor {
    pub name: String,
    pub version: String,
    pub code_revision: String,
    pub configuration_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MineralIngestionPolicy {
    CreateOnlyV1,
    ImaIdentityV1,
}

impl MineralIngestionPolicy {
    fn as_str(self) -> &'static str {
        match self {
            Self::CreateOnlyV1 => "create_only_v1",
            Self::ImaIdentityV1 => "ima_identity_v1",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MineralSnapshotKind {
    Complete,
    Incremental,
}

impl MineralSnapshotKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Incremental => "incremental",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MineralIngestionChunk {
    pub schema_version: u32,
    pub chunk_index: usize,
    pub items: Vec<MineralIngestionItem>,
}

/// Context stated by an official mineral authority alongside an identity.
///
/// These values are deliberately separate from curator-owned physical
/// properties. They can be refreshed only through the reviewed dataset
/// release workflow and retain their dataset/release provenance in storage.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MineralOfficialFacts {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub discovery_country: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub first_reference: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub second_reference: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub source_status: String,
}

impl MineralOfficialFacts {
    fn is_empty(&self) -> bool {
        self.discovery_country.is_empty()
            && self.first_reference.is_empty()
            && self.second_reference.is_empty()
            && self.source_status.is_empty()
    }

    fn as_nonempty_map(&self) -> BTreeMap<String, String> {
        [
            ("discovery_country", self.discovery_country.as_str()),
            ("first_reference", self.first_reference.as_str()),
            ("second_reference", self.second_reference.as_str()),
            ("source_status", self.source_status.as_str()),
        ]
        .into_iter()
        .filter(|(_, value)| !value.is_empty())
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MineralIngestionItem {
    pub source_record_id: String,
    pub source_locator: Option<String>,
    pub slug: String,
    pub canonical_name: String,
    pub formula: String,
    pub nomenclature_status: String,
    pub is_valid_species: bool,
    pub official_identifiers: BTreeMap<String, String>,
    pub synonyms: Vec<String>,
    #[serde(default, skip_serializing_if = "MineralOfficialFacts::is_empty")]
    pub official_facts: MineralOfficialFacts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MineralIngestionBatchStatus {
    Receiving,
    Ready,
    NeedsAttention,
    Approved,
    Rejected,
}

impl MineralIngestionBatchStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Receiving => "receiving",
            Self::Ready => "ready",
            Self::NeedsAttention => "needs_attention",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
        }
    }

    fn from_database(value: &str) -> Result<Self> {
        match value {
            "receiving" => Ok(Self::Receiving),
            "ready" => Ok(Self::Ready),
            "needs_attention" => Ok(Self::NeedsAttention),
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            _ => bail!("unsupported mineral ingestion status '{value}'"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MineralIngestionClassification {
    Create,
    Adopt,
    Update,
    Unchanged,
    Conflict,
    Missing,
}

impl MineralIngestionClassification {
    fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Adopt => "adopt",
            Self::Update => "update",
            Self::Unchanged => "unchanged",
            Self::Conflict => "conflict",
            Self::Missing => "missing",
        }
    }

    fn from_database(value: &str) -> Result<Self> {
        match value {
            "create" => Ok(Self::Create),
            "adopt" => Ok(Self::Adopt),
            "update" => Ok(Self::Update),
            "unchanged" => Ok(Self::Unchanged),
            "conflict" => Ok(Self::Conflict),
            "missing" => Ok(Self::Missing),
            _ => bail!("unsupported mineral ingestion classification '{value}'"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MineralIngestionReportItem {
    pub source_record_id: String,
    pub proposed_slug: String,
    pub resolved_slug: Option<String>,
    pub material_public_id: Option<String>,
    /// Content address of the exact existing target state reviewed at
    /// finalization. `None` means no existing material was targeted.
    #[serde(default)]
    pub target_baseline_hash: Option<String>,
    pub classification: MineralIngestionClassification,
    pub severity: String,
    pub code: String,
    pub message: String,
    pub critical_formula_change: bool,
    pub critical_validity_change: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MineralIngestionReportSummary {
    pub create_count: usize,
    pub adopt_count: usize,
    pub update_count: usize,
    pub unchanged_count: usize,
    pub conflict_count: usize,
    pub missing_count: usize,
    pub identity_critical_warning_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MineralIngestionReport {
    pub schema_version: u32,
    pub batch_id: String,
    pub manifest_hash: String,
    pub records_sha256: String,
    pub base_batch_id: Option<String>,
    pub generated_at: String,
    pub summary: MineralIngestionReportSummary,
    pub items: Vec<MineralIngestionReportItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MineralIngestionBatchDetail {
    pub batch_id: String,
    pub status: MineralIngestionBatchStatus,
    pub manifest_hash: String,
    pub report_hash: Option<String>,
    pub manifest: MineralDatasetManifest,
    pub received_chunk_count: usize,
    pub received_record_count: usize,
    pub report_summary: Option<MineralIngestionReportSummary>,
    /// At most 25 deterministic, evenly distributed payload samples so an
    /// operator can inspect ordinary records even when there are no anomalies.
    pub review_samples: Vec<MineralIngestionItem>,
    /// Bounded blockers/warnings for the operator queue. The immutable full
    /// report remains in quarantine and is used for activation.
    pub anomaly_samples: Vec<MineralIngestionReportItem>,
    pub created_at: String,
    pub finalized_at: Option<String>,
    pub decided_at: Option<String>,
    pub decision_actor: Option<String>,
    pub decision_note: String,
    pub backup_path: Option<String>,
    pub backup_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MineralIngestionBatchPage {
    pub items: Vec<MineralIngestionBatchDetail>,
    pub total_count: usize,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MineralChunkReceipt {
    pub batch_id: String,
    pub chunk_index: usize,
    pub content_hash: String,
    pub item_count: usize,
    pub stored: bool,
    pub received_chunk_count: usize,
    pub received_record_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MineralBatchDecisionRequest {
    pub manifest_hash: String,
    pub report_hash: String,
    pub base_batch_id: Option<String>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MineralBatchDecisionOutcome {
    pub batch_id: String,
    pub status: MineralIngestionBatchStatus,
    pub changed: bool,
    pub applied_create_count: usize,
    pub applied_adopt_count: usize,
    pub applied_update_count: usize,
    pub unchanged_count: usize,
    pub retired_offer_count: usize,
    pub backup_path: Option<String>,
    pub backup_sha256: Option<String>,
    pub decided_at: String,
}

pub fn init_registry_database(data_root: &Path) -> Result<()> {
    init_registry_database_with_options(data_root, true)
}

pub fn init_registry_database_with_options(data_root: &Path, backfill_legacy: bool) -> Result<()> {
    // Validate operational limits at startup, before accepting any writes.
    mineral_ingestion_limits()?;
    let mut conn = open_connection(data_root, true)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .context("failed to start registry migration transaction")?;
    tx.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS schema_migrations (
            name TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS materials (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            public_id TEXT,
            source_mineral_id INTEGER UNIQUE,
            slug TEXT NOT NULL UNIQUE,
            record_type TEXT NOT NULL CHECK(record_type IN ('mineral', 'compound')),
            canonical_name TEXT NOT NULL,
            formula TEXT NOT NULL DEFAULT '',
            description TEXT NOT NULL DEFAULT '',
            mineral_family TEXT NOT NULL DEFAULT '',
            cas_number TEXT,
            identifiers_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(identifiers_json)),
            synonyms_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(synonyms_json)),
            properties_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(properties_json)),
            safety_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(safety_json)),
            search_text TEXT NOT NULL DEFAULT '',
            verification_status TEXT NOT NULL DEFAULT 'draft'
                CHECK(verification_status IN ('draft', 'generated', 'sourced', 'reviewed', 'verified', 'disputed')),
            data_quality_score REAL NOT NULL DEFAULT 0.0
                CHECK(data_quality_score >= 0.0 AND data_quality_score <= 1.0),
            source_kind TEXT NOT NULL DEFAULT 'registry_import',
            license_spdx TEXT NOT NULL DEFAULT 'NOASSERTION',
            publication_status TEXT NOT NULL DEFAULT 'published'
                CHECK(publication_status IN ('published', 'withdrawn')),
            withdrawal_note TEXT NOT NULL DEFAULT '',
            withdrawn_at TEXT,
            nomenclature_status TEXT NOT NULL DEFAULT 'unknown',
            is_valid_species INTEGER NOT NULL DEFAULT 1 CHECK(is_valid_species IN (0, 1)),
            image_id INTEGER,
            metadata_schema_version INTEGER NOT NULL DEFAULT 2,
            embeddings_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(embeddings_json)),
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(source_mineral_id) REFERENCES minerals(id) ON DELETE SET NULL,
            FOREIGN KEY(image_id) REFERENCES images(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS material_aliases (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            material_id INTEGER NOT NULL,
            alias TEXT NOT NULL,
            alias_normalized TEXT NOT NULL,
            language_code TEXT NOT NULL DEFAULT '',
            alias_type TEXT NOT NULL DEFAULT 'synonym',
            origin TEXT NOT NULL DEFAULT 'import',
            dataset_key TEXT,
            source_release_id TEXT,
            UNIQUE(material_id, alias_normalized, language_code, alias_type),
            FOREIGN KEY(material_id) REFERENCES materials(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS evidence_sources (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            canonical_url TEXT NOT NULL UNIQUE,
            title TEXT NOT NULL,
            publisher TEXT NOT NULL DEFAULT '',
            license_spdx TEXT NOT NULL DEFAULT 'NOASSERTION',
            retrieved_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            content_hash TEXT NOT NULL DEFAULT '',
            metadata_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(metadata_json)),
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS material_evidence (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            material_id INTEGER NOT NULL,
            source_id INTEGER NOT NULL,
            claim_scope TEXT NOT NULL,
            claim_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(claim_json)),
            confidence REAL NOT NULL DEFAULT 0.5
                CHECK(confidence >= 0.0 AND confidence <= 1.0),
            review_status TEXT NOT NULL DEFAULT 'unreviewed'
                CHECK(review_status IN ('unreviewed', 'reviewed', 'verified', 'disputed')),
            source_title TEXT,
            source_publisher TEXT,
            source_license_spdx TEXT,
            source_retrieved_at TEXT,
            source_content_hash TEXT,
            source_attribution_party TEXT,
            source_work_title TEXT,
            source_work_url TEXT,
            source_license_url TEXT,
            source_changes_notice TEXT,
            source_no_endorsement_notice TEXT,
            source_derived_output_license_spdx TEXT,
            dataset_key TEXT,
            source_release_id TEXT,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(material_id, source_id, claim_scope),
            FOREIGN KEY(material_id) REFERENCES materials(id) ON DELETE CASCADE,
            FOREIGN KEY(source_id) REFERENCES evidence_sources(id) ON DELETE RESTRICT
        );

        CREATE TABLE IF NOT EXISTS providers (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            slug TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            website_url TEXT NOT NULL,
            network_kind TEXT NOT NULL DEFAULT 'direct',
            country_code TEXT NOT NULL DEFAULT '',
            verification_status TEXT NOT NULL DEFAULT 'unverified'
                CHECK(verification_status IN ('unverified', 'reviewed', 'verified', 'suspended')),
            trust_score REAL NOT NULL DEFAULT 0.0
                CHECK(trust_score >= 0.0 AND trust_score <= 1.0),
            terms_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(terms_json)),
            active INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0, 1)),
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS offers (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            material_id INTEGER NOT NULL,
            provider_id INTEGER NOT NULL,
            external_id TEXT NOT NULL,
            title TEXT NOT NULL,
            product_url TEXT NOT NULL,
            currency_code TEXT NOT NULL DEFAULT '',
            price_minor INTEGER CHECK(price_minor IS NULL OR price_minor >= 0),
            currency_exponent INTEGER NOT NULL DEFAULT 2 CHECK(currency_exponent BETWEEN 0 AND 6),
            pricing_basis TEXT NOT NULL DEFAULT 'quote',
            minimum_order_quantity REAL CHECK(minimum_order_quantity IS NULL OR minimum_order_quantity > 0),
            minimum_order_unit TEXT NOT NULL DEFAULT '',
            available_quantity REAL CHECK(available_quantity IS NULL OR available_quantity > 0),
            available_quantity_unit TEXT NOT NULL DEFAULT '',
            stock_status TEXT NOT NULL DEFAULT 'unknown'
                CHECK(stock_status IN ('in_stock', 'limited', 'made_to_order', 'quote_required', 'out_of_stock', 'unknown')),
            purity_text TEXT NOT NULL DEFAULT '',
            grade TEXT NOT NULL DEFAULT '',
            origin_country_code TEXT NOT NULL DEFAULT '',
            provider_claims_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(provider_claims_json)),
            evidence_source_id INTEGER,
            verification_status TEXT NOT NULL DEFAULT 'provider_claim'
                CHECK(verification_status IN ('provider_claim', 'observed', 'verified', 'disputed')),
            last_checked_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            expires_at TEXT,
            active INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0, 1)),
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(provider_id, external_id),
            FOREIGN KEY(material_id) REFERENCES materials(id) ON DELETE CASCADE,
            FOREIGN KEY(provider_id) REFERENCES providers(id) ON DELETE CASCADE,
            FOREIGN KEY(evidence_source_id) REFERENCES evidence_sources(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS sourcing_requests (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            public_id TEXT NOT NULL UNIQUE,
            material_id INTEGER,
            query TEXT NOT NULL,
            target_spec_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(target_spec_json)),
            quantity_value REAL CHECK(quantity_value IS NULL OR quantity_value > 0),
            quantity_unit TEXT NOT NULL DEFAULT '',
            destination_country_code TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'queued'
                CHECK(status IN ('queued', 'searching', 'options_found', 'needs_review', 'completed', 'cancelled')),
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(material_id) REFERENCES materials(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS provider_search_runs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            sourcing_request_id INTEGER NOT NULL,
            provider_id INTEGER,
            status TEXT NOT NULL DEFAULT 'queued',
            query_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(query_json)),
            result_count INTEGER NOT NULL DEFAULT 0,
            error_message TEXT NOT NULL DEFAULT '',
            started_at TEXT,
            completed_at TEXT,
            FOREIGN KEY(sourcing_request_id) REFERENCES sourcing_requests(id) ON DELETE CASCADE,
            FOREIGN KEY(provider_id) REFERENCES providers(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS material_media (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            material_id INTEGER NOT NULL,
            image_id INTEGER NOT NULL,
            media_role TEXT NOT NULL DEFAULT 'reference',
            origin_kind TEXT NOT NULL DEFAULT 'source'
                CHECK(origin_kind IN ('source', 'uploaded', 'generated')),
            synthetic INTEGER NOT NULL DEFAULT 0 CHECK(synthetic IN (0, 1)),
            generator_model TEXT NOT NULL DEFAULT '',
            prompt_hash TEXT NOT NULL DEFAULT '',
            source_url TEXT NOT NULL DEFAULT '',
            license_spdx TEXT NOT NULL DEFAULT 'NOASSERTION',
            alt_text TEXT NOT NULL DEFAULT '',
            verification_status TEXT NOT NULL DEFAULT 'unreviewed',
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(material_id, image_id, media_role),
            FOREIGN KEY(material_id) REFERENCES materials(id) ON DELETE CASCADE,
            FOREIGN KEY(image_id) REFERENCES images(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS ingestion_runs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_label TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'running',
            item_count INTEGER NOT NULL DEFAULT 0,
            imported_count INTEGER NOT NULL DEFAULT 0,
            evidence_count INTEGER NOT NULL DEFAULT 0,
            started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            completed_at TEXT,
            metadata_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(metadata_json))
        );

        CREATE TABLE IF NOT EXISTS ingestion_items (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            run_id INTEGER NOT NULL,
            material_slug TEXT NOT NULL,
            outcome TEXT NOT NULL,
            message TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(run_id) REFERENCES ingestion_runs(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS mineral_ingestion_batches (
            batch_id TEXT PRIMARY KEY,
            manifest_hash TEXT NOT NULL UNIQUE,
            manifest_json TEXT NOT NULL CHECK(json_valid(manifest_json)),
            dataset_key TEXT NOT NULL,
            source_key TEXT NOT NULL,
            release_version TEXT NOT NULL,
            artifact_sha256 TEXT NOT NULL,
            parser_name TEXT NOT NULL,
            parser_version TEXT NOT NULL,
            parser_code_revision TEXT NOT NULL,
            parser_configuration_sha256 TEXT NOT NULL,
            policy TEXT NOT NULL CHECK(policy IN ('create_only_v1', 'ima_identity_v1')),
            snapshot_kind TEXT NOT NULL CHECK(snapshot_kind IN ('complete', 'incremental')),
            expected_record_count INTEGER NOT NULL CHECK(expected_record_count > 0),
            expected_chunk_count INTEGER NOT NULL CHECK(expected_chunk_count > 0),
            expected_records_sha256 TEXT NOT NULL,
            base_batch_id TEXT,
            status TEXT NOT NULL DEFAULT 'receiving'
                CHECK(status IN ('receiving', 'ready', 'needs_attention', 'approved', 'rejected')),
            report_hash TEXT,
            report_json TEXT CHECK(report_json IS NULL OR json_valid(report_json)),
            decision_actor TEXT,
            decision_note TEXT NOT NULL DEFAULT '',
            compacted_chunk_count INTEGER NOT NULL DEFAULT 0 CHECK(compacted_chunk_count >= 0),
            compacted_record_count INTEGER NOT NULL DEFAULT 0 CHECK(compacted_record_count >= 0),
            compacted_payload_bytes INTEGER NOT NULL DEFAULT 0 CHECK(compacted_payload_bytes >= 0),
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            finalized_at TEXT,
            decided_at TEXT,
            FOREIGN KEY(base_batch_id) REFERENCES mineral_ingestion_batches(batch_id) ON DELETE RESTRICT
        );

        CREATE TABLE IF NOT EXISTS mineral_ingestion_chunks (
            batch_id TEXT NOT NULL,
            chunk_index INTEGER NOT NULL CHECK(chunk_index >= 0),
            content_hash TEXT NOT NULL,
            payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
            payload_bytes INTEGER NOT NULL CHECK(payload_bytes > 0),
            item_count INTEGER NOT NULL CHECK(item_count > 0),
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY(batch_id, chunk_index),
            FOREIGN KEY(batch_id) REFERENCES mineral_ingestion_batches(batch_id) ON DELETE RESTRICT
        );

        CREATE TABLE IF NOT EXISTS mineral_ingestion_items (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            batch_id TEXT NOT NULL,
            chunk_index INTEGER NOT NULL,
            item_index INTEGER NOT NULL CHECK(item_index >= 0),
            source_record_id TEXT NOT NULL,
            proposed_slug TEXT NOT NULL,
            normalized_name TEXT NOT NULL,
            item_hash TEXT NOT NULL,
            payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(batch_id, chunk_index, item_index),
            FOREIGN KEY(batch_id, chunk_index)
                REFERENCES mineral_ingestion_chunks(batch_id, chunk_index) ON DELETE RESTRICT
        );

        CREATE TABLE IF NOT EXISTS mineral_ingestion_report_items (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            batch_id TEXT NOT NULL,
            source_record_id TEXT NOT NULL,
            proposed_slug TEXT NOT NULL,
            resolved_slug TEXT,
            material_id INTEGER,
            target_baseline_hash TEXT,
            classification TEXT NOT NULL
                CHECK(classification IN ('create', 'adopt', 'update', 'unchanged', 'conflict', 'missing')),
            severity TEXT NOT NULL CHECK(severity IN ('info', 'warning', 'error')),
            code TEXT NOT NULL,
            message TEXT NOT NULL,
            critical_formula_change INTEGER NOT NULL DEFAULT 0 CHECK(critical_formula_change IN (0, 1)),
            critical_validity_change INTEGER NOT NULL DEFAULT 0 CHECK(critical_validity_change IN (0, 1)),
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(batch_id) REFERENCES mineral_ingestion_batches(batch_id) ON DELETE RESTRICT,
            FOREIGN KEY(material_id) REFERENCES materials(id) ON DELETE RESTRICT
        );

        CREATE TABLE IF NOT EXISTS mineral_external_identities (
            dataset_key TEXT NOT NULL,
            source_record_id TEXT NOT NULL,
            material_id INTEGER NOT NULL,
            created_batch_id TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY(dataset_key, source_record_id),
            UNIQUE(dataset_key, material_id),
            FOREIGN KEY(material_id) REFERENCES materials(id) ON DELETE RESTRICT,
            FOREIGN KEY(created_batch_id) REFERENCES mineral_ingestion_batches(batch_id) ON DELETE RESTRICT
        );

        CREATE TABLE IF NOT EXISTS mineral_dataset_identifiers (
            dataset_key TEXT NOT NULL,
            material_id INTEGER NOT NULL,
            identifier_key TEXT NOT NULL,
            identifier_value TEXT NOT NULL,
            normalized_value TEXT NOT NULL,
            source_release_id TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY(dataset_key, material_id, identifier_key),
            UNIQUE(dataset_key, identifier_key, normalized_value),
            FOREIGN KEY(material_id) REFERENCES materials(id) ON DELETE RESTRICT,
            FOREIGN KEY(source_release_id) REFERENCES mineral_ingestion_batches(batch_id) ON DELETE RESTRICT
        );

        CREATE TABLE IF NOT EXISTS mineral_dataset_facts (
            dataset_key TEXT NOT NULL,
            material_id INTEGER NOT NULL,
            fact_key TEXT NOT NULL,
            fact_value TEXT NOT NULL,
            source_release_id TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY(dataset_key, material_id, fact_key),
            FOREIGN KEY(material_id) REFERENCES materials(id) ON DELETE RESTRICT,
            FOREIGN KEY(source_release_id) REFERENCES mineral_ingestion_batches(batch_id) ON DELETE RESTRICT
        );

        CREATE TABLE IF NOT EXISTS mineral_dataset_heads (
            dataset_key TEXT PRIMARY KEY,
            batch_id TEXT NOT NULL UNIQUE,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(batch_id) REFERENCES mineral_ingestion_batches(batch_id) ON DELETE RESTRICT
        );

        CREATE TABLE IF NOT EXISTS mineral_ingestion_authorities (
            policy TEXT PRIMARY KEY CHECK(policy IN ('ima_identity_v1')),
            dataset_key TEXT NOT NULL,
            source_key TEXT NOT NULL,
            bound_batch_id TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(bound_batch_id) REFERENCES mineral_ingestion_batches(batch_id) ON DELETE RESTRICT
        );

        CREATE TABLE IF NOT EXISTS mineral_ingestion_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            batch_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            actor TEXT NOT NULL,
            policy_version TEXT NOT NULL,
            manifest_hash TEXT NOT NULL,
            report_hash TEXT,
            detail_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(detail_json)),
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(batch_id) REFERENCES mineral_ingestion_batches(batch_id) ON DELETE RESTRICT
        );

        CREATE TABLE IF NOT EXISTS mineral_ingestion_backups (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            batch_id TEXT NOT NULL UNIQUE,
            relative_path TEXT NOT NULL UNIQUE,
            sha256 TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('completed', 'failed', 'pruned')),
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(batch_id) REFERENCES mineral_ingestion_batches(batch_id) ON DELETE RESTRICT
        );

        CREATE TABLE IF NOT EXISTS mineral_review_revisions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            material_slug TEXT NOT NULL,
            revision INTEGER NOT NULL CHECK(revision > 0),
            ingestion_run_id INTEGER NOT NULL,
            source_label TEXT NOT NULL,
            payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
            status TEXT NOT NULL DEFAULT 'pending'
                CHECK(status IN ('pending', 'approved', 'rejected', 'superseded')),
            operator_note TEXT NOT NULL DEFAULT '',
            submitted_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            reviewed_at TEXT,
            UNIQUE(material_slug, revision),
            FOREIGN KEY(ingestion_run_id) REFERENCES ingestion_runs(id) ON DELETE RESTRICT
        );

        CREATE INDEX IF NOT EXISTS idx_materials_type_name
            ON materials(record_type, canonical_name);
        CREATE INDEX IF NOT EXISTS idx_materials_formula
            ON materials(formula);
        CREATE INDEX IF NOT EXISTS idx_materials_cas
            ON materials(cas_number);
        CREATE INDEX IF NOT EXISTS idx_materials_verification
            ON materials(verification_status, data_quality_score DESC);
        CREATE INDEX IF NOT EXISTS idx_aliases_normalized
            ON material_aliases(alias_normalized);
        CREATE INDEX IF NOT EXISTS idx_material_evidence_material
            ON material_evidence(material_id, review_status);
        CREATE INDEX IF NOT EXISTS idx_offers_material_active
            ON offers(material_id, active, stock_status, verification_status);
        CREATE INDEX IF NOT EXISTS idx_offers_provider_active
            ON offers(provider_id, active);
        CREATE INDEX IF NOT EXISTS idx_sourcing_requests_status
            ON sourcing_requests(status, created_at);
        CREATE INDEX IF NOT EXISTS idx_mineral_reviews_queue
            ON mineral_review_revisions(status, submitted_at, id);
        CREATE INDEX IF NOT EXISTS idx_mineral_ingestion_batches_queue
            ON mineral_ingestion_batches(status, created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_mineral_ingestion_items_source
            ON mineral_ingestion_items(batch_id, source_record_id);
        CREATE INDEX IF NOT EXISTS idx_mineral_ingestion_items_slug
            ON mineral_ingestion_items(batch_id, proposed_slug);
        CREATE INDEX IF NOT EXISTS idx_mineral_ingestion_items_name
            ON mineral_ingestion_items(batch_id, normalized_name);
        CREATE INDEX IF NOT EXISTS idx_mineral_ingestion_report_anomalies
            ON mineral_ingestion_report_items(batch_id, severity DESC, id);
        CREATE INDEX IF NOT EXISTS idx_mineral_dataset_facts_material
            ON mineral_dataset_facts(material_id, dataset_key, source_release_id);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_mineral_reviews_one_pending
            ON mineral_review_revisions(material_slug) WHERE status = 'pending';

        CREATE TRIGGER IF NOT EXISTS mineral_ingestion_chunks_immutable_update
        BEFORE UPDATE ON mineral_ingestion_chunks BEGIN
            SELECT RAISE(ABORT, 'ingestion chunks are immutable');
        END;
        CREATE TRIGGER IF NOT EXISTS mineral_ingestion_chunks_immutable_delete
        BEFORE DELETE ON mineral_ingestion_chunks
        WHEN NOT EXISTS (
            SELECT 1 FROM mineral_ingestion_batches b
            WHERE b.batch_id = old.batch_id
              AND b.status IN ('approved', 'rejected')
              AND b.decided_at IS NOT NULL
        ) BEGIN
            SELECT RAISE(ABORT, 'ingestion chunks are immutable');
        END;
        CREATE TRIGGER IF NOT EXISTS mineral_ingestion_manifest_immutable
        BEFORE UPDATE OF
            manifest_hash, manifest_json, dataset_key, source_key,
            release_version, artifact_sha256, parser_name, parser_version,
            parser_code_revision, parser_configuration_sha256, policy,
            snapshot_kind, expected_record_count, expected_chunk_count,
            expected_records_sha256, base_batch_id
        ON mineral_ingestion_batches BEGIN
            SELECT RAISE(ABORT, 'ingestion manifests are immutable');
        END;
        CREATE TRIGGER IF NOT EXISTS mineral_ingestion_frozen_report_immutable
        BEFORE UPDATE OF report_hash, report_json, finalized_at
        ON mineral_ingestion_batches
        WHEN old.report_hash IS NOT NULL BEGIN
            SELECT RAISE(ABORT, 'finalized ingestion reports are immutable');
        END;
        CREATE TRIGGER IF NOT EXISTS mineral_ingestion_items_immutable_update
        BEFORE UPDATE ON mineral_ingestion_items BEGIN
            SELECT RAISE(ABORT, 'ingestion items are immutable');
        END;
        CREATE TRIGGER IF NOT EXISTS mineral_ingestion_items_immutable_delete
        BEFORE DELETE ON mineral_ingestion_items
        WHEN NOT EXISTS (
            SELECT 1 FROM mineral_ingestion_batches b
            WHERE b.batch_id = old.batch_id
              AND b.status IN ('approved', 'rejected')
              AND b.decided_at IS NOT NULL
        ) BEGIN
            SELECT RAISE(ABORT, 'ingestion items are immutable');
        END;
        CREATE TRIGGER IF NOT EXISTS mineral_ingestion_reports_immutable_update
        BEFORE UPDATE ON mineral_ingestion_report_items BEGIN
            SELECT RAISE(ABORT, 'ingestion reports are immutable');
        END;
        CREATE TRIGGER IF NOT EXISTS mineral_ingestion_reports_immutable_delete
        BEFORE DELETE ON mineral_ingestion_report_items BEGIN
            SELECT RAISE(ABORT, 'ingestion reports are immutable');
        END;
        CREATE TRIGGER IF NOT EXISTS mineral_ingestion_events_immutable_update
        BEFORE UPDATE ON mineral_ingestion_events BEGIN
            SELECT RAISE(ABORT, 'ingestion events are append-only');
        END;
        CREATE TRIGGER IF NOT EXISTS mineral_ingestion_events_immutable_delete
        BEFORE DELETE ON mineral_ingestion_events BEGIN
            SELECT RAISE(ABORT, 'ingestion events are append-only');
        END;
        CREATE TRIGGER IF NOT EXISTS mineral_external_identities_immutable_update
        BEFORE UPDATE ON mineral_external_identities BEGIN
            SELECT RAISE(ABORT, 'external identity mappings are immutable');
        END;
        CREATE TRIGGER IF NOT EXISTS mineral_external_identities_immutable_delete
        BEFORE DELETE ON mineral_external_identities BEGIN
            SELECT RAISE(ABORT, 'external identity mappings are immutable');
        END;
        CREATE TRIGGER IF NOT EXISTS mineral_ingestion_authorities_immutable_update
        BEFORE UPDATE ON mineral_ingestion_authorities BEGIN
            SELECT RAISE(ABORT, 'ingestion authority bindings are immutable');
        END;
        CREATE TRIGGER IF NOT EXISTS mineral_ingestion_authorities_immutable_delete
        BEFORE DELETE ON mineral_ingestion_authorities BEGIN
            SELECT RAISE(ABORT, 'ingestion authority bindings are immutable');
        END;

        CREATE VIRTUAL TABLE IF NOT EXISTS material_search USING fts5(
            canonical_name,
            formula,
            description,
            search_text,
            content='materials',
            content_rowid='id',
            tokenize='unicode61 remove_diacritics 2'
        );

        CREATE TRIGGER IF NOT EXISTS materials_search_insert AFTER INSERT ON materials BEGIN
            INSERT INTO material_search(rowid, canonical_name, formula, description, search_text)
            VALUES (new.id, new.canonical_name, new.formula, new.description, new.search_text);
        END;

        CREATE TRIGGER IF NOT EXISTS materials_search_delete AFTER DELETE ON materials BEGIN
            INSERT INTO material_search(material_search, rowid, canonical_name, formula, description, search_text)
            VALUES ('delete', old.id, old.canonical_name, old.formula, old.description, old.search_text);
        END;

        CREATE TRIGGER IF NOT EXISTS materials_search_update AFTER UPDATE ON materials BEGIN
            INSERT INTO material_search(material_search, rowid, canonical_name, formula, description, search_text)
            VALUES ('delete', old.id, old.canonical_name, old.formula, old.description, old.search_text);
            INSERT INTO material_search(rowid, canonical_name, formula, description, search_text)
            VALUES (new.id, new.canonical_name, new.formula, new.description, new.search_text);
        END;

        CREATE TRIGGER IF NOT EXISTS minerals_registry_insert AFTER INSERT ON minerals BEGIN
            INSERT OR IGNORE INTO materials (
                public_id, source_mineral_id, slug, record_type, canonical_name, formula, description,
                mineral_family, properties_json, search_text, verification_status,
                data_quality_score, source_kind, license_spdx, image_id
            ) VALUES (
                'mat_' || lower(hex(randomblob(16))),
                new.id, new.slug, 'mineral', new.common_name, new.formula, new.description,
                new.mineral_family,
                json_object(
                    'hardness_mohs', new.hardness_mohs,
                    'density_g_cm3', new.density_g_cm3,
                    'crystal_system', new.crystal_system,
                    'color', new.color,
                    'streak', new.streak,
                    'luster', new.luster,
                    'major_elements_pct', json(new.major_elements_pct_json)
                ),
                new.common_name || ' ' || new.formula || ' ' || new.mineral_family,
                'generated', 0.35, 'legacy_catalog', 'NOASSERTION', new.image_id
            );
            UPDATE materials
            SET source_mineral_id = new.id
            WHERE slug = new.slug AND source_mineral_id IS NULL;
        END;

        CREATE TRIGGER IF NOT EXISTS minerals_registry_update AFTER UPDATE ON minerals BEGIN
            UPDATE materials
            SET canonical_name = new.common_name,
                formula = new.formula,
                description = new.description,
                mineral_family = new.mineral_family,
                properties_json = json_object(
                    'hardness_mohs', new.hardness_mohs,
                    'density_g_cm3', new.density_g_cm3,
                    'crystal_system', new.crystal_system,
                    'color', new.color,
                    'streak', new.streak,
                    'luster', new.luster,
                    'major_elements_pct', json(new.major_elements_pct_json)
                ),
                search_text = new.common_name || ' ' || new.formula || ' ' || new.mineral_family,
                image_id = new.image_id,
                updated_at = CURRENT_TIMESTAMP
            WHERE source_mineral_id = new.id AND source_kind = 'legacy_catalog';
        END;

        CREATE TRIGGER IF NOT EXISTS minerals_registry_delete AFTER DELETE ON minerals BEGIN
            DELETE FROM materials
            WHERE slug = old.slug
              AND source_kind = 'legacy_catalog'
              AND NOT EXISTS (
                  SELECT 1 FROM material_evidence WHERE material_id = materials.id
              )
              AND NOT EXISTS (
                  SELECT 1 FROM offers WHERE material_id = materials.id
              );
        END;
        "#,
    )
    .context("failed to initialize registry schema")?;

    if !table_has_column(&tx, "mineral_ingestion_chunks", "payload_bytes")? {
        tx.execute_batch(
            "ALTER TABLE mineral_ingestion_chunks ADD COLUMN payload_bytes INTEGER NOT NULL DEFAULT 0 CHECK(payload_bytes >= 0);",
        )
        .context("failed to add bounded quarantine payload accounting")?;
    }
    if !table_has_column(
        &tx,
        "mineral_ingestion_report_items",
        "target_baseline_hash",
    )? {
        tx.execute_batch(
            "ALTER TABLE mineral_ingestion_report_items ADD COLUMN target_baseline_hash TEXT;",
        )
        .context("failed to add report target preconditions")?;
    }
    for column in [
        "compacted_chunk_count",
        "compacted_record_count",
        "compacted_payload_bytes",
    ] {
        if !table_has_column(&tx, "mineral_ingestion_batches", column)? {
            tx.execute_batch(&format!(
                "ALTER TABLE mineral_ingestion_batches ADD COLUMN {column} INTEGER NOT NULL DEFAULT 0 CHECK({column} >= 0);"
            ))
            .with_context(|| format!("failed to add terminal compaction column '{column}'"))?;
        }
    }
    // Older preview schemas had unconditional delete guards. Payload deletion
    // remains forbidden until a terminal decision has durably committed its
    // report/event metadata in the same transaction.
    tx.execute_batch(
        r#"
        DROP TRIGGER IF EXISTS mineral_ingestion_chunks_immutable_delete;
        CREATE TRIGGER mineral_ingestion_chunks_immutable_delete
        BEFORE DELETE ON mineral_ingestion_chunks
        WHEN NOT EXISTS (
            SELECT 1 FROM mineral_ingestion_batches b
            WHERE b.batch_id = old.batch_id
              AND b.status IN ('approved', 'rejected')
              AND b.decided_at IS NOT NULL
        ) BEGIN
            SELECT RAISE(ABORT, 'ingestion chunks are immutable');
        END;
        DROP TRIGGER IF EXISTS mineral_ingestion_items_immutable_delete;
        CREATE TRIGGER mineral_ingestion_items_immutable_delete
        BEFORE DELETE ON mineral_ingestion_items
        WHEN NOT EXISTS (
            SELECT 1 FROM mineral_ingestion_batches b
            WHERE b.batch_id = old.batch_id
              AND b.status IN ('approved', 'rejected')
              AND b.decided_at IS NOT NULL
        ) BEGIN
            SELECT RAISE(ABORT, 'ingestion items are immutable');
        END;
        "#,
    )
    .context("failed to install quarantine retention guards")?;

    let approved_ima_authorities = {
        let mut stmt = tx
            .prepare(
                r#"
                SELECT dataset_key, source_key, MIN(batch_id)
                FROM mineral_ingestion_batches
                WHERE policy = 'ima_identity_v1' AND status = 'approved'
                GROUP BY dataset_key, source_key
                ORDER BY dataset_key, source_key
                "#,
            )
            .context("failed to inspect historical IMA authority")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    if approved_ima_authorities.len() > 1 {
        bail!(
            "multiple dataset/source pairs already own approved ima_identity_v1 batches; operator authority resolution is required"
        );
    }
    if let Some((dataset_key, source_key, batch_id)) = approved_ima_authorities.first() {
        tx.execute(
            r#"
            INSERT INTO mineral_ingestion_authorities(
                policy, dataset_key, source_key, bound_batch_id
            ) VALUES ('ima_identity_v1', ?1, ?2, ?3)
            ON CONFLICT(policy) DO NOTHING
            "#,
            params![dataset_key, source_key, batch_id],
        )
        .context("failed to backfill IMA authority binding")?;
        let bound: (String, String) = tx.query_row(
            "SELECT dataset_key, source_key FROM mineral_ingestion_authorities WHERE policy = 'ima_identity_v1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if bound.0 != *dataset_key || bound.1 != *source_key {
            bail!("persisted IMA authority binding conflicts with approved batch history");
        }
    }

    // Schema-v1 manifests predate the hashed attribution boundary. They must
    // remain readable for audit, but no non-terminal v1 batch may cross into
    // publication after this migration. Reject it without rewriting its
    // immutable manifest or frozen report; the adapter must restage a v2 batch.
    let legacy_nonterminal_batches = {
        let mut stmt = tx
            .prepare(
                r#"
                SELECT batch_id
                FROM mineral_ingestion_batches
                WHERE status IN ('receiving', 'ready', 'needs_attention')
                  AND CAST(json_extract(manifest_json, '$.schema_version') AS INTEGER) < 2
                ORDER BY batch_id
                "#,
            )
            .context("failed to inspect legacy non-terminal ingestion batches")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for batch_id in legacy_nonterminal_batches {
        let stored = load_stored_batch(&tx, &batch_id)?
            .with_context(|| format!("legacy batch '{batch_id}' disappeared"))?;
        tx.execute(
            r#"
            UPDATE mineral_ingestion_batches
            SET status = 'rejected',
                decision_actor = 'system:attribution-v2-migration',
                decision_note = 'schema_v1_requires_attributed_restage',
                decided_at = CURRENT_TIMESTAMP
            WHERE batch_id = ?1
              AND status IN ('receiving', 'ready', 'needs_attention')
            "#,
            params![batch_id],
        )
        .context("failed to reject an unattributed schema-v1 batch")?;
        append_mineral_ingestion_event(
            &tx,
            &batch_id,
            "batch_rejected",
            "system:attribution-v2-migration",
            stored.manifest.policy,
            &stored.manifest_hash,
            stored.report_hash.as_deref(),
            &json!({"reason": "schema_v1_requires_attributed_restage"}),
        )?;
    }

    let terminal_batches = {
        let mut stmt = tx
            .prepare(
                r#"
                SELECT DISTINCT b.batch_id
                FROM mineral_ingestion_batches b
                JOIN mineral_ingestion_chunks c ON c.batch_id = b.batch_id
                WHERE b.status IN ('approved', 'rejected')
                ORDER BY b.batch_id
                "#,
            )
            .context("failed to inspect terminal quarantine payloads")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for batch_id in terminal_batches {
        let stored = load_stored_batch(&tx, &batch_id)?
            .with_context(|| format!("terminal batch '{batch_id}' disappeared"))?;
        compact_terminal_mineral_ingestion_payload(
            &tx,
            &stored,
            "system:terminal-payload-migration",
        )?;
    }

    for (column, definition) in [
        ("public_id", "TEXT"),
        ("nomenclature_status", "TEXT NOT NULL DEFAULT 'unknown'"),
        (
            "is_valid_species",
            "INTEGER NOT NULL DEFAULT 1 CHECK(is_valid_species IN (0, 1))",
        ),
    ] {
        if !table_has_column(&tx, "materials", column)? {
            tx.execute_batch(&format!(
                "ALTER TABLE materials ADD COLUMN {column} {definition};"
            ))
            .with_context(|| format!("failed to add bulk mineral column '{column}'"))?;
        }
    }
    for table in ["material_aliases", "material_evidence"] {
        for (column, definition) in [("dataset_key", "TEXT"), ("source_release_id", "TEXT")] {
            if !table_has_column(&tx, table, column)? {
                tx.execute_batch(&format!(
                    "ALTER TABLE {table} ADD COLUMN {column} {definition};"
                ))
                .with_context(|| format!("failed to add {table}.{column}"))?;
            }
        }
    }
    tx.execute(
        r#"
        UPDATE materials
        SET public_id = 'mat_' || lower(hex(randomblob(16)))
        WHERE public_id IS NULL
        "#,
        [],
    )
    .context("failed to backfill immutable material public ids")?;
    tx.execute_batch(
        r#"
        CREATE UNIQUE INDEX IF NOT EXISTS idx_materials_public_id
            ON materials(public_id) WHERE public_id IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_material_aliases_dataset
            ON material_aliases(material_id, dataset_key, source_release_id);
        CREATE INDEX IF NOT EXISTS idx_material_evidence_dataset
            ON material_evidence(material_id, dataset_key, source_release_id);
        DROP TRIGGER IF EXISTS materials_public_id_after_insert;
        DROP TRIGGER IF EXISTS minerals_registry_insert;
        CREATE TRIGGER minerals_registry_insert AFTER INSERT ON minerals BEGIN
            INSERT OR IGNORE INTO materials (
                public_id, source_mineral_id, slug, record_type, canonical_name, formula, description,
                mineral_family, properties_json, search_text, verification_status,
                data_quality_score, source_kind, license_spdx, image_id
            ) VALUES (
                'mat_' || lower(hex(randomblob(16))),
                new.id, new.slug, 'mineral', new.common_name, new.formula, new.description,
                new.mineral_family,
                json_object(
                    'hardness_mohs', new.hardness_mohs,
                    'density_g_cm3', new.density_g_cm3,
                    'crystal_system', new.crystal_system,
                    'color', new.color,
                    'streak', new.streak,
                    'luster', new.luster,
                    'major_elements_pct', json(new.major_elements_pct_json)
                ),
                new.common_name || ' ' || new.formula || ' ' || new.mineral_family,
                'generated', 0.35, 'legacy_catalog', 'NOASSERTION', new.image_id
            );
            UPDATE materials
            SET source_mineral_id = new.id
            WHERE slug = new.slug AND source_mineral_id IS NULL;
        END;
        CREATE TRIGGER IF NOT EXISTS materials_public_id_immutable
        BEFORE UPDATE OF public_id ON materials
        WHEN old.public_id IS NOT NULL AND new.public_id IS NOT old.public_id BEGIN
            SELECT RAISE(ABORT, 'material public_id is immutable');
        END;
        "#,
    )
    .context("failed to index bulk mineral ownership metadata")?;
    tx.execute(
        "INSERT OR IGNORE INTO schema_migrations(name) VALUES (?1)",
        params![BULK_MINERAL_INGESTION_MIGRATION],
    )
    .context("failed to record bulk mineral ingestion migration")?;

    // Older registries predate the review workflow. Treat every existing live
    // row as already published so deploying this migration cannot make the
    // legacy catalog (or previously imported records) disappear. New mineral
    // imports are staged in mineral_review_revisions and never rely on this
    // default.
    if !table_has_column(&tx, "materials", "publication_status")? {
        tx.execute_batch(
            r#"
            ALTER TABLE materials ADD COLUMN publication_status TEXT NOT NULL DEFAULT 'published'
                CHECK(publication_status IN ('published', 'withdrawn'));
            "#,
        )
        .context("failed to add material publication state")?;
    }
    tx.execute_batch(
        r#"
        CREATE INDEX IF NOT EXISTS idx_materials_publication
            ON materials(publication_status, record_type, canonical_name);
        CREATE INDEX IF NOT EXISTS idx_materials_public_browse
            ON materials(
                publication_status, record_type, is_valid_species,
                data_quality_score DESC, canonical_name COLLATE NOCASE, slug
            );
        "#,
    )
    .context("failed to index material publication state")?;
    for (column, definition) in [
        ("withdrawal_note", "TEXT NOT NULL DEFAULT ''"),
        ("withdrawn_at", "TEXT"),
    ] {
        if !table_has_column(&tx, "materials", column)? {
            tx.execute_batch(&format!(
                "ALTER TABLE materials ADD COLUMN {column} {definition};"
            ))
            .with_context(|| format!("failed to add material withdrawal column '{column}'"))?;
        }
    }
    tx.execute(
        "INSERT OR IGNORE INTO schema_migrations(name) VALUES (?1)",
        params![MINERAL_WITHDRAWAL_MIGRATION],
    )
    .context("failed to record mineral withdrawal migration")?;
    tx.execute(
        "INSERT OR IGNORE INTO schema_migrations(name) VALUES (?1)",
        params![MINERAL_REVIEW_MIGRATION],
    )
    .context("failed to record mineral review workflow migration")?;

    // Evidence-source rows are globally de-duplicated by canonical URL, but
    // descriptive metadata belongs to the material/source claim as observed
    // at publication time. NULL values on migrated rows intentionally fall
    // back to the pre-existing global metadata when read.
    for (column, definition) in [
        ("source_title", "TEXT"),
        ("source_publisher", "TEXT"),
        ("source_license_spdx", "TEXT"),
        ("source_retrieved_at", "TEXT"),
        ("source_content_hash", "TEXT"),
    ] {
        if !table_has_column(&tx, "material_evidence", column)? {
            tx.execute_batch(&format!(
                "ALTER TABLE material_evidence ADD COLUMN {column} {definition};"
            ))
            .with_context(|| format!("failed to add material evidence column '{column}'"))?;
        }
    }
    tx.execute(
        "INSERT OR IGNORE INTO schema_migrations(name) VALUES (?1)",
        params![EVIDENCE_SNAPSHOT_MIGRATION],
    )
    .context("failed to record material evidence snapshot migration")?;

    // Source-level evidence rows are de-duplicated and therefore mutable
    // metadata cannot be the publication authority. Every attribution value
    // displayed for a material is snapshotted on its evidence association.
    for (column, definition) in [
        ("source_attribution_party", "TEXT"),
        ("source_work_title", "TEXT"),
        ("source_work_url", "TEXT"),
        ("source_license_url", "TEXT"),
        ("source_changes_notice", "TEXT"),
        ("source_no_endorsement_notice", "TEXT"),
        ("source_derived_output_license_spdx", "TEXT"),
    ] {
        if !table_has_column(&tx, "material_evidence", column)? {
            tx.execute_batch(&format!(
                "ALTER TABLE material_evidence ADD COLUMN {column} {definition};"
            ))
            .with_context(|| {
                format!("failed to add material evidence attribution column '{column}'")
            })?;
        }
    }
    tx.execute(
        "INSERT OR IGNORE INTO schema_migrations(name) VALUES (?1)",
        params![EVIDENCE_ATTRIBUTION_SNAPSHOT_MIGRATION],
    )
    .context("failed to record material evidence attribution snapshot migration")?;

    let registry_images_detached: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE name = ?1)",
            params![REGISTRY_IMAGE_DETACH_MIGRATION],
            |row| row.get::<_, i64>(0),
        )
        .context("failed to inspect registry image detach migration")?
        == 1;
    if !registry_images_detached {
        tx.execute(
            r#"
            UPDATE materials
            SET image_id = NULL, updated_at = CURRENT_TIMESTAMP
            WHERE record_type = 'mineral'
              AND publication_status = 'published'
              AND source_kind = 'registry_import'
              AND image_id IS NOT NULL
            "#,
            [],
        )
        .context("failed to detach inherited images from registry imports")?;
        tx.execute(
            "INSERT INTO schema_migrations(name) VALUES (?1)",
            params![REGISTRY_IMAGE_DETACH_MIGRATION],
        )
        .context("failed to record registry image detach migration")?;
    }

    let migration_applied: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE name = ?1)",
            params![REGISTRY_MIGRATION],
            |row| row.get::<_, i64>(0),
        )
        .context("failed to inspect registry migration state")?
        == 1;

    if backfill_legacy {
        tx.execute_batch(
            r#"
        INSERT OR IGNORE INTO materials (
            public_id, source_mineral_id, slug, record_type, canonical_name, formula, description,
            mineral_family, properties_json, search_text, verification_status,
            data_quality_score, source_kind, license_spdx, image_id
        )
        SELECT
            'mat_' || lower(hex(randomblob(16))),
            m.id, m.slug, 'mineral', m.common_name, m.formula, m.description,
            m.mineral_family,
            json_object(
                'hardness_mohs', m.hardness_mohs,
                'density_g_cm3', m.density_g_cm3,
                'crystal_system', m.crystal_system,
                'color', m.color,
                'streak', m.streak,
                'luster', m.luster,
                'major_elements_pct', json(m.major_elements_pct_json)
            ),
            m.common_name || ' ' || m.formula || ' ' || m.mineral_family,
            'generated', 0.35, 'legacy_catalog', 'NOASSERTION', m.image_id
        FROM minerals m;
            "#,
        )
        .context("failed to backfill legacy minerals into material registry")?;
    }

    if !migration_applied {
        tx.execute(
            "INSERT INTO material_search(material_search) VALUES ('rebuild')",
            [],
        )
        .context("failed to build material full-text index")?;
        tx.execute(
            "INSERT INTO schema_migrations(name) VALUES (?1)",
            params![REGISTRY_MIGRATION],
        )
        .context("failed to record registry migration")?;
    }
    tx.execute(
        "INSERT OR IGNORE INTO schema_migrations(name) VALUES (?1)",
        params![BULK_MINERAL_INGESTION_SAFETY_MIGRATION],
    )
    .context("failed to record bulk mineral ingestion safety migration")?;
    tx.execute(
        "INSERT OR IGNORE INTO schema_migrations(name) VALUES (?1)",
        params![MINERAL_DATASET_FACTS_MIGRATION],
    )
    .context("failed to record dataset-owned mineral facts migration")?;

    tx.commit()
        .context("failed to commit registry migration transaction")?;
    Ok(())
}

pub fn sync_legacy_minerals(data_root: &Path) -> Result<()> {
    let conn = open_connection(data_root, true)?;
    conn.execute_batch(
        r#"
        INSERT OR IGNORE INTO materials (
            public_id, source_mineral_id, slug, record_type, canonical_name, formula, description,
            mineral_family, properties_json, search_text, verification_status,
            data_quality_score, source_kind, license_spdx, image_id
        )
        SELECT
            'mat_' || lower(hex(randomblob(16))),
            m.id, m.slug, 'mineral', m.common_name, m.formula, m.description,
            m.mineral_family,
            json_object(
                'hardness_mohs', m.hardness_mohs,
                'density_g_cm3', m.density_g_cm3,
                'crystal_system', m.crystal_system,
                'color', m.color,
                'streak', m.streak,
                'luster', m.luster,
                'major_elements_pct', json(m.major_elements_pct_json)
            ),
            m.common_name || ' ' || m.formula || ' ' || m.mineral_family,
            'generated', 0.35, 'legacy_catalog', 'NOASSERTION', m.image_id
        FROM minerals m;
        "#,
    )
    .context("failed to synchronize legacy minerals into material registry")?;
    Ok(())
}

pub fn registry_stats(data_root: &Path) -> Result<RegistryStats> {
    let mut conn = open_connection(data_root, false)?;
    let tx = conn
        .transaction()
        .context("failed to start registry statistics snapshot")?;
    let stats = load_registry_stats(&tx)?;
    tx.commit()
        .context("failed to finish registry statistics snapshot")?;
    Ok(stats)
}

fn load_registry_stats(conn: &Connection) -> Result<RegistryStats> {
    let active_offer_sql = format!(
        "SELECT COUNT(*) FROM offers o JOIN providers p ON p.id = o.provider_id JOIN materials m ON m.id = o.material_id WHERE m.publication_status = 'published' AND (m.record_type = 'compound' OR (m.record_type = 'mineral' AND m.is_valid_species = 1)) AND {ACTIVE_OFFER_PREDICATE}"
    );
    Ok(RegistryStats {
        material_count: count(
            conn,
            "SELECT COUNT(*) FROM materials WHERE publication_status = 'published' AND (record_type <> 'mineral' OR is_valid_species = 1)",
        )?,
        mineral_count: count(
            conn,
            "SELECT COUNT(*) FROM materials WHERE record_type = 'mineral' AND publication_status = 'published' AND is_valid_species = 1",
        )?,
        compound_count: count(
            conn,
            "SELECT COUNT(*) FROM materials WHERE record_type = 'compound' AND publication_status = 'published'",
        )?,
        evidence_count: count(
            conn,
            "SELECT COUNT(*) FROM material_evidence me JOIN materials m ON m.id = me.material_id WHERE m.publication_status = 'published' AND (m.record_type <> 'mineral' OR m.is_valid_species = 1)",
        )?,
        active_offer_count: count(conn, &active_offer_sql)?,
        provider_count: count(
            conn,
            "SELECT COUNT(*) FROM providers WHERE active = 1 AND verification_status <> 'suspended'",
        )?,
    })
}

/// Returns legacy-projected mineral slugs that are still public and have not
/// been replaced by an approved registry record.
pub fn published_legacy_mineral_slugs(data_root: &Path) -> Result<HashSet<String>> {
    let conn = open_connection(data_root, false)?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT slug
            FROM materials
            WHERE record_type = 'mineral'
              AND publication_status = 'published'
              AND is_valid_species = 1
              AND source_kind <> 'registry_import'
              AND NOT EXISTS (
                  SELECT 1 FROM mineral_external_identities mei
                  WHERE mei.material_id = materials.id
              )
            "#,
        )
        .context("failed to prepare published legacy mineral query")?;
    let slugs = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<HashSet<_>>>()?;
    Ok(slugs)
}

/// Whether a registered media file is reachable from at least one currently
/// published record. Legacy catalog-specific images count only while their
/// legacy projection remains public (not when registry content shadows it).
pub fn registered_image_is_public(data_root: &Path, stored_name: &str) -> Result<bool> {
    if stored_name.trim().is_empty() {
        return Ok(false);
    }
    let conn = open_connection(data_root, false)?;
    let direct: bool = conn
        .query_row(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM images i
                JOIN materials m ON m.image_id = i.id
                WHERE i.stored_name = ?1
                  AND m.record_type = 'mineral'
                  AND m.publication_status = 'published'
                  AND m.is_valid_species = 1
                  AND m.source_kind <> 'registry_import'
                  AND NOT EXISTS (
                      SELECT 1 FROM mineral_external_identities mei
                      WHERE mei.material_id = m.id
                  )
                UNION ALL
                SELECT 1
                FROM images i
                JOIN material_media mm ON mm.image_id = i.id
                JOIN materials m ON m.id = mm.material_id
                WHERE i.stored_name = ?1
                  AND m.record_type = 'mineral'
                  AND m.publication_status = 'published'
                  AND m.is_valid_species = 1
            )
            "#,
            params![stored_name],
            |row| row.get::<_, i64>(0),
        )
        .context("failed to inspect published material image")?
        == 1;
    if direct || !table_has_column(&conn, "catalog", "folder_name")? {
        return Ok(direct);
    }
    let legacy_catalog: bool = conn
        .query_row(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM images i
                JOIN catalog c ON c.image_id = i.id
                JOIN materials m ON m.source_mineral_id = c.source_mineral_id
                WHERE i.stored_name = ?1
                  AND m.record_type = 'mineral'
                  AND m.publication_status = 'published'
                  AND m.is_valid_species = 1
                  AND m.source_kind <> 'registry_import'
                  AND NOT EXISTS (
                      SELECT 1 FROM mineral_external_identities mei
                      WHERE mei.material_id = m.id
                  )
            )
            "#,
            params![stored_name],
            |row| row.get::<_, i64>(0),
        )
        .context("failed to inspect published legacy catalog image")?
        == 1;
    Ok(legacy_catalog)
}

/// Whether a legacy report folder still belongs to a public legacy mineral.
pub fn legacy_report_folder_is_public(data_root: &Path, folder: &str) -> Result<bool> {
    if folder.trim().is_empty() {
        return Ok(false);
    }
    let conn = open_connection(data_root, false)?;
    if !table_has_column(&conn, "catalog", "folder_name")? {
        return Ok(false);
    }
    let public: bool = conn
        .query_row(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM catalog c
                JOIN materials m ON m.source_mineral_id = c.source_mineral_id
                WHERE c.folder_name = ?1
                  AND m.record_type = 'mineral'
                  AND m.publication_status = 'published'
                  AND m.is_valid_species = 1
                  AND m.source_kind <> 'registry_import'
                  AND NOT EXISTS (
                      SELECT 1 FROM mineral_external_identities mei
                      WHERE mei.material_id = m.id
                  )
            )
            "#,
            params![folder],
            |row| row.get::<_, i64>(0),
        )
        .context("failed to inspect legacy report publication state")?
        == 1;
    Ok(public)
}

pub fn registry_is_healthy(data_root: &Path) -> Result<()> {
    let conn = open_connection(data_root, false)?;
    let result: i64 = conn
        .query_row("SELECT 1", [], |row| row.get(0))
        .context("registry health query failed")?;
    if result != 1 {
        bail!("registry health query returned an unexpected result");
    }
    Ok(())
}

/// Startup/readiness gate for the writable registry contract. This is
/// intentionally stricter than liveness and fails on partial bulk migrations.
pub fn registry_is_ready(data_root: &Path) -> Result<()> {
    mineral_ingestion_limits()?;
    let conn = open_connection(data_root, false)?;
    let foreign_keys: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .context("failed to inspect SQLite foreign key mode")?;
    if foreign_keys != 1 {
        bail!("SQLite foreign key enforcement is disabled");
    }
    let migration_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE name IN (?1, ?2, ?3, ?4)",
            params![
                BULK_MINERAL_INGESTION_MIGRATION,
                BULK_MINERAL_INGESTION_SAFETY_MIGRATION,
                EVIDENCE_ATTRIBUTION_SNAPSHOT_MIGRATION,
                MINERAL_DATASET_FACTS_MIGRATION
            ],
            |row| row.get(0),
        )
        .context("failed to inspect bulk mineral migration markers")?;
    if migration_count != 4 {
        bail!("bulk mineral ingestion safety migrations are not fully applied");
    }
    for table in [
        "mineral_ingestion_batches",
        "mineral_ingestion_chunks",
        "mineral_ingestion_items",
        "mineral_ingestion_report_items",
        "mineral_external_identities",
        "mineral_dataset_identifiers",
        "mineral_dataset_facts",
        "mineral_dataset_heads",
        "mineral_ingestion_authorities",
        "mineral_ingestion_events",
        "mineral_ingestion_backups",
        "material_search",
    ] {
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = ?1)",
                params![table],
                |row| row.get::<_, i64>(0),
            )
            .with_context(|| format!("failed to inspect required table '{table}'"))?
            == 1;
        if !exists {
            bail!("required registry table '{table}' is missing");
        }
    }
    for (table, columns) in [
        (
            "materials",
            &["public_id", "nomenclature_status", "is_valid_species"][..],
        ),
        (
            "material_aliases",
            &["dataset_key", "source_release_id"][..],
        ),
        (
            "material_evidence",
            &[
                "dataset_key",
                "source_release_id",
                "source_attribution_party",
                "source_work_title",
                "source_work_url",
                "source_license_url",
                "source_changes_notice",
                "source_no_endorsement_notice",
                "source_derived_output_license_spdx",
            ][..],
        ),
        ("mineral_ingestion_chunks", &["payload_bytes"][..]),
        (
            "mineral_ingestion_report_items",
            &["target_baseline_hash"][..],
        ),
        (
            "mineral_ingestion_batches",
            &[
                "compacted_chunk_count",
                "compacted_record_count",
                "compacted_payload_bytes",
            ][..],
        ),
    ] {
        for column in columns {
            if !table_has_column(&conn, table, column)? {
                bail!("required registry column '{table}.{column}' is missing");
            }
        }
    }
    let missing_public_ids: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM materials WHERE public_id IS NULL OR public_id = ''",
            [],
            |row| row.get(0),
        )
        .context("failed to verify immutable material public ids")?;
    if missing_public_ids != 0 {
        bail!("registry contains materials without immutable public ids");
    }
    let insert_trigger: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'trigger' AND name = 'minerals_registry_insert'",
            [],
            |row| row.get(0),
        )
        .context("legacy mineral public-id trigger is missing")?;
    if !insert_trigger.contains("public_id") || !insert_trigger.contains("randomblob(16)") {
        bail!("legacy mineral insert trigger does not allocate immutable public ids");
    }
    let authority_violations: i64 = conn
        .query_row(
            r#"
            SELECT COUNT(*)
            FROM mineral_ingestion_batches b
            LEFT JOIN mineral_ingestion_authorities a
              ON a.policy = b.policy
             AND a.dataset_key = b.dataset_key
             AND a.source_key = b.source_key
            WHERE b.policy = 'ima_identity_v1'
              AND b.status = 'approved'
              AND a.policy IS NULL
            "#,
            [],
            |row| row.get(0),
        )
        .context("failed to verify IMA authority history")?;
    if authority_violations != 0 {
        bail!("approved IMA history is not covered by the immutable authority binding");
    }
    conn.query_row("SELECT COUNT(*) FROM material_search", [], |row| {
        row.get::<_, i64>(0)
    })
    .context("material full-text index is unavailable")?;

    probe_data_root_writable(data_root)?;
    Ok(())
}

/// A startup/deployment acceptance check that deliberately acquires the
/// SQLite writer. Do not call this from a frequent steady-state readyz probe.
pub fn registry_accepts_writes(data_root: &Path) -> Result<()> {
    let mut conn = open_connection(data_root, true)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .context("registry cannot acquire a write transaction")?;
    tx.execute(
        "UPDATE schema_migrations SET applied_at = applied_at WHERE name = ?1",
        params![BULK_MINERAL_INGESTION_MIGRATION],
    )
    .context("registry failed its writable acceptance probe")?;
    tx.rollback()
        .context("failed to roll back registry write acceptance probe")?;
    Ok(())
}

fn probe_data_root_writable(data_root: &Path) -> Result<()> {
    let mut nonce = [0_u8; 8];
    getrandom::getrandom(&mut nonce)
        .map_err(|error| anyhow::anyhow!("failed to generate readiness probe nonce: {error}"))?;
    let name = nonce
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let path = data_root.join(format!(".registry-ready-{name}.tmp"));
    let result = (|| -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .with_context(|| format!("data root {} is not writable", data_root.display()))?;
        file.write_all(b"ready")
            .context("failed to write data-root readiness probe")?;
        Ok(())
    })();
    let cleanup = fs::remove_file(&path);
    result?;
    cleanup.context("failed to remove data-root readiness probe")?;
    Ok(())
}

pub fn search_materials(
    data_root: &Path,
    query: &str,
    record_type: Option<&str>,
    limit: usize,
) -> Result<Vec<MaterialSearchItem>> {
    Ok(search_materials_page(data_root, query, record_type, limit, 0)?.items)
}

pub fn search_materials_page(
    data_root: &Path,
    query: &str,
    record_type: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<MaterialSearchPage> {
    if query.chars().count() > 500 {
        bail!("material search query exceeds 500 characters");
    }
    let mut conn = open_connection(data_root, false)?;
    let limit = limit.clamp(1, MAX_SEARCH_RESULTS);
    let sql_limit = i64::try_from(limit).context("material search limit is too large")?;
    let sql_offset = i64::try_from(offset).context("material search offset is too large")?;
    let kind = record_type.unwrap_or("").trim().to_ascii_lowercase();
    if !kind.is_empty() && !matches!(kind.as_str(), "mineral" | "compound") {
        bail!("record_type must be 'mineral' or 'compound'");
    }

    let fts_query = make_fts_query(query);
    let transaction = conn
        .transaction()
        .context("failed to start material search transaction")?;
    let (count_sql, sql, uses_fts) = if fts_query.is_empty() {
        (
            r#"
            SELECT COUNT(*)
            FROM materials m
            WHERE m.publication_status = 'published'
              AND (m.record_type <> 'mineral' OR m.is_valid_species = 1)
              AND (?1 = '' OR m.record_type = ?1)
            "#,
            format!(
                r#"
            SELECT
                m.slug, m.record_type, m.canonical_name, m.formula, m.description,
                m.mineral_family, m.verification_status, m.data_quality_score,
                (SELECT COUNT(*) FROM material_evidence me WHERE me.material_id = m.id),
                (SELECT COUNT(*) FROM offers o
                    JOIN providers p ON p.id = o.provider_id
                    WHERE o.material_id = m.id AND {ACTIVE_OFFER_PREDICATE}),
                CASE WHEN m.source_kind = 'registry_import' OR EXISTS(
                    SELECT 1 FROM mineral_external_identities mei WHERE mei.material_id = m.id
                ) THEN 1 ELSE 0 END,
                m.public_id, m.nomenclature_status, m.is_valid_species,
                (SELECT me.source_derived_output_license_spdx
                   FROM material_evidence me
                  WHERE me.material_id = m.id
                    AND COALESCE(me.source_attribution_party, '') <> ''
                    AND COALESCE(me.source_derived_output_license_spdx, '') <> ''
                  ORDER BY me.id ASC LIMIT 1)
            FROM materials m
            WHERE m.publication_status = 'published'
              AND (m.record_type <> 'mineral' OR m.is_valid_species = 1)
              AND (?1 = '' OR m.record_type = ?1)
            ORDER BY m.data_quality_score DESC, m.canonical_name COLLATE NOCASE ASC, m.slug ASC
            LIMIT ?2 OFFSET ?3
            "#
            ),
            false,
        )
    } else {
        (
            r#"
            SELECT COUNT(*)
            FROM material_search
            JOIN materials m ON m.id = material_search.rowid
            WHERE material_search MATCH ?1
              AND m.publication_status = 'published'
              AND (m.record_type <> 'mineral' OR m.is_valid_species = 1)
              AND (?2 = '' OR m.record_type = ?2)
            "#,
            format!(
                r#"
            SELECT
                m.slug, m.record_type, m.canonical_name, m.formula, m.description,
                m.mineral_family, m.verification_status, m.data_quality_score,
                (SELECT COUNT(*) FROM material_evidence me WHERE me.material_id = m.id),
                (SELECT COUNT(*) FROM offers o
                    JOIN providers p ON p.id = o.provider_id
                    WHERE o.material_id = m.id AND {ACTIVE_OFFER_PREDICATE}),
                CASE WHEN m.source_kind = 'registry_import' OR EXISTS(
                    SELECT 1 FROM mineral_external_identities mei WHERE mei.material_id = m.id
                ) THEN 1 ELSE 0 END,
                m.public_id, m.nomenclature_status, m.is_valid_species,
                (SELECT me.source_derived_output_license_spdx
                   FROM material_evidence me
                  WHERE me.material_id = m.id
                    AND COALESCE(me.source_attribution_party, '') <> ''
                    AND COALESCE(me.source_derived_output_license_spdx, '') <> ''
                  ORDER BY me.id ASC LIMIT 1)
            FROM material_search
            JOIN materials m ON m.id = material_search.rowid
            WHERE material_search MATCH ?1
              AND m.publication_status = 'published'
              AND (m.record_type <> 'mineral' OR m.is_valid_species = 1)
              AND (?2 = '' OR m.record_type = ?2)
            ORDER BY bm25(material_search), m.data_quality_score DESC,
                     m.canonical_name COLLATE NOCASE ASC, m.slug ASC
            LIMIT ?3 OFFSET ?4
            "#
            ),
            true,
        )
    };

    let total_count = if uses_fts {
        transaction.query_row(count_sql, params![fts_query, kind], |row| {
            row.get::<_, i64>(0)
        })?
    } else {
        transaction.query_row(count_sql, params![kind], |row| row.get::<_, i64>(0))?
    };
    let total_count = usize::try_from(total_count).context("invalid material result count")?;

    let mut stmt = transaction
        .prepare(&sql)
        .context("failed to prepare material search")?;
    let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<MaterialSearchItem> {
        let slug: String = row.get(0)?;
        let description: String = row.get(4)?;
        let attribution_license_spdx: Option<String> = row.get(14)?;
        Ok(MaterialSearchItem {
            detail_path: format!("/minerals/{slug}"),
            attribution_path: attribution_license_spdx
                .as_ref()
                .map(|_| format!("/minerals/{slug}#attribution")),
            attribution_license_spdx,
            slug,
            record_type: row.get(1)?,
            canonical_name: row.get(2)?,
            formula: row.get(3)?,
            description_excerpt: material_description_excerpt(&description),
            description,
            mineral_family: row.get(5)?,
            verification_status: row.get(6)?,
            data_quality_score: row.get(7)?,
            evidence_count: row.get::<_, i64>(8)? as usize,
            active_offer_count: row.get::<_, i64>(9)? as usize,
            registry_authoritative: row.get::<_, i64>(10)? == 1,
            public_id: row.get(11)?,
            nomenclature_status: row.get(12)?,
            is_valid_species: row.get::<_, i64>(13)? == 1,
        })
    };

    let rows = if uses_fts {
        stmt.query_map(params![fts_query, kind, sql_limit, sql_offset], map_row)
            .context("failed to search the material registry")?
            .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        stmt.query_map(params![kind, sql_limit, sql_offset], map_row)
            .context("failed to list the material registry")?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    drop(stmt);
    transaction
        .commit()
        .context("failed to finish material search transaction")?;
    Ok(MaterialSearchPage {
        items: rows,
        total_count,
        limit,
        offset,
    })
}

pub fn get_material_detail(data_root: &Path, slug: &str) -> Result<Option<MaterialDetail>> {
    let mut conn = open_connection(data_root, false)?;
    let tx = conn
        .transaction()
        .context("failed to start material detail snapshot")?;
    let detail = load_material_detail(&tx, slug)?;
    tx.commit()
        .context("failed to finish material detail snapshot")?;
    Ok(detail)
}

fn load_material_detail(conn: &Connection, slug: &str) -> Result<Option<MaterialDetail>> {
    let base = conn
        .query_row(
            r#"
            SELECT
                id, slug, record_type, canonical_name, formula, description,
                mineral_family, verification_status, data_quality_score,
                source_kind, license_spdx, cas_number, identifiers_json,
                properties_json, safety_json, source_mineral_id, public_id,
                nomenclature_status, is_valid_species,
                EXISTS(SELECT 1 FROM mineral_external_identities mei WHERE mei.material_id = materials.id)
            FROM materials
            WHERE slug = ?1
              AND publication_status = 'published'
              AND (record_type <> 'mineral' OR is_valid_species = 1)
            "#,
            params![slug],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, f64>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, Option<String>>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, String>(13)?,
                    row.get::<_, String>(14)?,
                    row.get::<_, Option<i64>>(15)?,
                    row.get::<_, String>(16)?,
                    row.get::<_, String>(17)?,
                    row.get::<_, i64>(18)?,
                    row.get::<_, i64>(19)?,
                ))
            },
        )
        .optional()
        .context("failed to load material detail")?;

    let Some((
        material_id,
        slug,
        record_type,
        canonical_name,
        formula,
        description,
        mineral_family,
        verification_status,
        data_quality_score,
        source_kind,
        license_spdx,
        cas_number,
        identifiers_json,
        properties_json,
        safety_json,
        source_mineral_id,
        public_id,
        nomenclature_status,
        is_valid_species,
        has_bulk_mapping,
    )) = base
    else {
        return Ok(None);
    };

    let mut identifiers = json_object_to_facts(&identifiers_json);
    if cas_number.is_some() {
        identifiers.retain(|fact| !matches!(fact.key.as_str(), "cas" | "cas_number"));
    }
    let properties = json_object_to_facts(&properties_json);
    let safety = json_object_to_facts(&safety_json);
    let official_facts = load_material_official_facts(conn, material_id)?;
    let registry_authoritative = has_bulk_mapping == 1 || source_kind == "registry_import";
    let legacy_profile_path = if registry_authoritative {
        None
    } else {
        source_mineral_id.map(|_| format!("/minerals/{slug}"))
    };
    let properties_json = pretty_json(&properties_json);
    let safety_json = pretty_json(&safety_json);
    let evidence = load_evidence(conn, material_id)?;
    let offers = load_offers(conn, material_id)?;
    Ok(Some(MaterialDetail {
        public_id,
        slug,
        record_type,
        canonical_name,
        formula,
        description,
        mineral_family,
        nomenclature_status,
        is_valid_species: is_valid_species == 1,
        official_facts,
        verification_status,
        data_quality_score,
        source_kind,
        registry_authoritative,
        license_spdx,
        cas_number,
        identifiers,
        properties,
        safety,
        legacy_profile_path,
        properties_json,
        safety_json,
        evidence,
        offers,
    }))
}

fn load_material_official_facts(
    conn: &Connection,
    material_id: i64,
) -> Result<MineralOfficialFacts> {
    let mut facts = MineralOfficialFacts::default();
    let mut stmt = conn
        .prepare(
            r#"
            SELECT f.fact_key, f.fact_value
            FROM mineral_dataset_facts f
            JOIN mineral_ingestion_authorities a
              ON a.dataset_key = f.dataset_key
             AND a.policy = 'ima_identity_v1'
            WHERE f.material_id = ?1
            ORDER BY f.fact_key
            "#,
        )
        .context("failed to prepare official mineral facts query")?;
    let rows = stmt.query_map(params![material_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (key, value) = row?;
        match key.as_str() {
            "discovery_country" => facts.discovery_country = value,
            "first_reference" => facts.first_reference = value,
            "second_reference" => facts.second_reference = value,
            "source_status" => facts.source_status = value,
            _ => bail!("unsupported persisted official mineral fact key '{key}'"),
        }
    }
    Ok(facts)
}

pub fn offers_for_material(data_root: &Path, slug: &str) -> Result<Option<Vec<ProviderOffer>>> {
    let mut conn = open_connection(data_root, false)?;
    let tx = conn
        .transaction()
        .context("failed to start provider offer snapshot")?;
    let material_id = tx
        .query_row(
            "SELECT id FROM materials WHERE slug = ?1 AND publication_status = 'published' AND (record_type = 'compound' OR (record_type = 'mineral' AND is_valid_species = 1))",
            params![slug],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .context("failed to resolve material for provider offers")?;
    let offers = material_id.map(|id| load_offers(&tx, id)).transpose()?;
    tx.commit()
        .context("failed to finish provider offer snapshot")?;
    Ok(offers)
}

pub fn validate_import(record: &MaterialImport) -> Result<()> {
    if !is_valid_registry_slug(&record.slug) {
        bail!(
            "invalid slug '{}': use lowercase ASCII letters, digits, '.', '-' or '_'",
            record.slug
        );
    }
    if !matches!(record.record_type.as_str(), "mineral" | "compound") {
        bail!("record_type must be 'mineral' or 'compound'");
    }
    validate_text("canonical_name", &record.canonical_name, 1, 200)?;
    validate_text("formula", &record.formula, 0, 240)?;
    validate_text("description", &record.description, 0, 20_000)?;
    validate_text("license_spdx", &record.license_spdx, 1, 120)?;
    ensure_json_object("identifiers", &record.identifiers)?;
    ensure_json_object("properties", &record.properties)?;
    ensure_json_object("safety", &record.safety)?;
    for (label, value) in [
        ("identifiers", &record.identifiers),
        ("properties", &record.properties),
        ("safety", &record.safety),
    ] {
        if serde_json::to_vec(value)?.len() > 250_000 {
            bail!("{label} exceeds 250000 encoded bytes");
        }
        if json_depth(value) > 16 {
            bail!("{label} exceeds 16 nested JSON levels");
        }
    }
    if !(0.0..=1.0).contains(&record.data_quality_score) {
        bail!("data_quality_score must be between 0 and 1");
    }
    if !matches!(
        record.verification_status.as_str(),
        "draft" | "generated" | "sourced" | "reviewed" | "verified" | "disputed"
    ) {
        bail!(
            "unsupported verification_status '{}'",
            record.verification_status
        );
    }

    if let Some(cas) = record.cas_number.as_deref() {
        if !is_valid_cas_number(cas) {
            bail!("invalid CAS Registry Number checksum: '{cas}'");
        }
    }

    if record.synonyms.len() > 200 {
        bail!("material import exceeds 200 synonyms");
    }
    if record.sources.len() > 100 {
        bail!("material import exceeds 100 evidence sources");
    }
    for synonym in &record.synonyms {
        validate_text("synonym", synonym, 1, 240)?;
    }
    let mut evidence_claims = HashSet::new();
    for source in &record.sources {
        validate_evidence(source)?;
        let canonical_url = canonicalize_evidence_url(&source.url)?;
        let claim_scope = normalize_claim_scope(&source.claim_scope)?;
        if !evidence_claims.insert((canonical_url.clone(), claim_scope.clone())) {
            bail!(
                "duplicate evidence claim for source '{canonical_url}' and scope '{claim_scope}'"
            );
        }
    }

    // Publication gates are deliberately based on canonical, independently
    // reviewed source URLs. "sourced" may contain an unreviewed citation, a
    // "reviewed" record needs a reviewed citation, and a "verified" record
    // needs two reviewed citations with at least one independently verified.
    let qualified_sources = |minimum_review: bool| -> Result<HashSet<String>> {
        record
            .sources
            .iter()
            .filter(|source| {
                source.review_status != "disputed"
                    && (!minimum_review
                        || matches!(source.review_status.as_str(), "reviewed" | "verified"))
            })
            .map(|source| canonicalize_evidence_url(&source.url))
            .collect()
    };
    match record.verification_status.as_str() {
        "sourced" if qualified_sources(false)?.is_empty() => {
            bail!("verification_status 'sourced' requires a non-disputed source")
        }
        "reviewed" if qualified_sources(true)?.is_empty() => {
            bail!("verification_status 'reviewed' requires a reviewed source")
        }
        "verified" => {
            if qualified_sources(true)?.len() < 2 {
                bail!(
                    "verification_status 'verified' requires at least two distinct reviewed sources"
                );
            }
            if !record
                .sources
                .iter()
                .any(|source| source.review_status == "verified")
            {
                bail!("verification_status 'verified' requires a verified source");
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn import_material_batch(
    data_root: &Path,
    source_label: &str,
    records: &[MaterialImport],
) -> Result<ImportSummary> {
    if records.is_empty() {
        bail!("import contains no material records");
    }
    if records.len() > 10_000 {
        bail!("material import exceeds the 10000-record batch limit");
    }
    validate_text("source_label", source_label, 1, 240)?;
    let mut slugs = std::collections::HashSet::new();
    for (index, record) in records.iter().enumerate() {
        if !slugs.insert(record.slug.as_str()) {
            bail!("material import contains duplicate slug '{}'", record.slug);
        }
        validate_import(record)
            .with_context(|| format!("record {} ({})", index + 1, record.slug))?;
    }

    let mut conn = open_connection(data_root, true)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .context("failed to start registry import transaction")?;
    tx.execute(
        "INSERT INTO ingestion_runs(source_label, item_count) VALUES (?1, ?2)",
        params![source_label.trim(), records.len() as i64],
    )
    .context("failed to create ingestion run")?;
    let run_id = tx.last_insert_rowid();
    let mut evidence_count = 0usize;
    let mut review_ids = Vec::new();

    for record in records {
        let outcome = if record.record_type == "mineral" {
            review_ids.push(stage_mineral_review(
                &tx,
                run_id,
                source_label.trim(),
                record,
            )?);
            "pending_review"
        } else {
            // Kept for compatibility with older internal import clients. The
            // public application is mineral-only, and all mineral revisions
            // must pass through the operator review workflow.
            upsert_material(&tx, record)?;
            "upserted"
        };
        evidence_count += record.sources.len();
        tx.execute(
            "INSERT INTO ingestion_items(run_id, material_slug, outcome) VALUES (?1, ?2, ?3)",
            params![run_id, record.slug, outcome],
        )
        .context("failed to record imported material")?;
    }

    tx.execute(
        r#"
        UPDATE ingestion_runs
        SET status = 'completed', imported_count = ?1, evidence_count = ?2,
            completed_at = CURRENT_TIMESTAMP
        WHERE id = ?3
        "#,
        params![records.len() as i64, evidence_count as i64, run_id],
    )
    .context("failed to finish ingestion run")?;
    tx.commit()
        .context("failed to commit registry import transaction")?;

    let queued_count = review_ids.len();
    Ok(ImportSummary {
        run_id,
        imported_count: records.len(),
        evidence_count,
        queued_count,
        published_count: records.len() - queued_count,
        review_ids,
    })
}

/// Returns the content address used as the immutable batch id input. Hashes
/// use compact canonical JSON: object keys are recursively sorted, arrays keep
/// their order, and the bytes are UTF-8.
pub fn canonical_mineral_manifest_hash(manifest: &MineralDatasetManifest) -> Result<String> {
    Ok(sha256_bytes(&canonical_json_bytes(manifest)?))
}

pub fn canonical_mineral_chunk_hash(chunk: &MineralIngestionChunk) -> Result<String> {
    Ok(sha256_bytes(&canonical_json_bytes(chunk)?))
}

pub fn canonical_mineral_records_hash(items: &[MineralIngestionItem]) -> Result<String> {
    Ok(sha256_bytes(&canonical_json_bytes(items)?))
}

/// Creates an immutable quarantine batch. Retrying the identical manifest is
/// idempotent. `actor` is trusted server context, never a client payload.
pub fn create_mineral_ingestion_batch(
    data_root: &Path,
    actor: &str,
    manifest: &MineralDatasetManifest,
) -> Result<MineralIngestionBatchDetail> {
    let limits = mineral_ingestion_limits()?;
    validate_mineral_ingestion_actor(actor).map_err(|error| {
        mineral_ingestion_problem(
            MineralIngestionProblemKind::Invalid,
            "invalid_actor",
            error.to_string(),
        )
    })?;
    validate_mineral_manifest(manifest).map_err(|error| {
        mineral_ingestion_problem(
            MineralIngestionProblemKind::Invalid,
            "invalid_manifest",
            error.to_string(),
        )
    })?;
    let manifest_hash = canonical_mineral_manifest_hash(manifest)?;
    let batch_id = format!("batch_{}", &manifest_hash[7..]);
    let manifest_json = String::from_utf8(canonical_json_bytes(manifest)?)
        .context("canonical manifest is not UTF-8")?;

    let mut conn = open_connection(data_root, true)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .context("failed to start mineral batch creation transaction")?;
    expire_abandoned_mineral_ingestion_batches_in_tx(
        &tx,
        "system:quarantine-retention",
        limits.abandoned_hours,
    )?;
    if let Some(existing) = load_mineral_ingestion_batch(&tx, &batch_id)? {
        if existing.manifest_hash != manifest_hash {
            return Err(mineral_ingestion_problem(
                MineralIngestionProblemKind::Conflict,
                "batch_id_collision",
                "the content-addressed batch id is already bound to another manifest",
            ));
        }
        tx.commit()
            .context("failed to finish idempotent batch creation")?;
        return Ok(existing);
    }

    let head = mineral_dataset_head(&tx, &manifest.dataset.key)?;
    if head != manifest.base_batch_id {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Conflict,
            "base_batch_mismatch",
            format!(
                "dataset '{}' currently has base {:?}, not {:?}",
                manifest.dataset.key, head, manifest.base_batch_id
            ),
        ));
    }
    tx.execute(
        r#"
        INSERT INTO mineral_ingestion_batches(
            batch_id, manifest_hash, manifest_json, dataset_key, source_key,
            release_version, artifact_sha256, parser_name, parser_version,
            parser_code_revision, parser_configuration_sha256, policy,
            snapshot_kind, expected_record_count, expected_chunk_count,
            expected_records_sha256, base_batch_id
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
            ?14, ?15, ?16, ?17
        )
        "#,
        params![
            batch_id,
            manifest_hash,
            manifest_json,
            manifest.dataset.key,
            manifest.source.key,
            manifest.release.version,
            manifest.artifact.sha256,
            manifest.parser.name,
            manifest.parser.version,
            manifest.parser.code_revision,
            manifest.parser.configuration_sha256,
            manifest.policy.as_str(),
            manifest.snapshot_kind.as_str(),
            manifest.expected_record_count as i64,
            manifest.expected_chunk_count as i64,
            manifest.records_sha256,
            manifest.base_batch_id,
        ],
    )
    .context("failed to create mineral ingestion batch")?;
    append_mineral_ingestion_event(
        &tx,
        &batch_id,
        "batch_created",
        actor,
        manifest.policy,
        &manifest_hash,
        None,
        &json!({
            "expected_record_count": manifest.expected_record_count,
            "expected_chunk_count": manifest.expected_chunk_count,
        }),
    )?;
    let detail = load_mineral_ingestion_batch(&tx, &batch_id)?
        .context("new mineral ingestion batch disappeared")?;
    tx.commit()
        .context("failed to commit mineral batch creation")?;
    Ok(detail)
}

/// Stores one content-addressed chunk. The supplied hash is over the
/// canonical full chunk object, not the transport's raw body bytes.
pub fn put_mineral_ingestion_chunk(
    data_root: &Path,
    batch_id: &str,
    actor: &str,
    content_hash: &str,
    chunk: &MineralIngestionChunk,
) -> Result<MineralChunkReceipt> {
    let limits = mineral_ingestion_limits()?;
    validate_mineral_ingestion_actor(actor).map_err(|error| {
        mineral_ingestion_problem(
            MineralIngestionProblemKind::Invalid,
            "invalid_actor",
            error.to_string(),
        )
    })?;
    validate_batch_id(batch_id).map_err(|error| {
        mineral_ingestion_problem(
            MineralIngestionProblemKind::Invalid,
            "invalid_batch_id",
            error.to_string(),
        )
    })?;
    validate_sha256("chunk content_hash", content_hash).map_err(|error| {
        mineral_ingestion_problem(
            MineralIngestionProblemKind::Invalid,
            "invalid_chunk_hash",
            error.to_string(),
        )
    })?;
    validate_mineral_chunk(chunk).map_err(|error| {
        mineral_ingestion_problem(
            MineralIngestionProblemKind::Invalid,
            "invalid_chunk",
            error.to_string(),
        )
    })?;
    let actual_hash = canonical_mineral_chunk_hash(chunk)?;
    if actual_hash != content_hash {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Invalid,
            "chunk_hash_mismatch",
            format!("chunk hash is {actual_hash}, not {content_hash}"),
        ));
    }
    let payload_json =
        String::from_utf8(canonical_json_bytes(chunk)?).context("canonical chunk is not UTF-8")?;
    let mut item_payloads = Vec::with_capacity(chunk.items.len());
    let mut payload_bytes =
        u64::try_from(payload_json.len()).context("chunk payload is too large")?;
    for item in &chunk.items {
        let item_json = String::from_utf8(canonical_json_bytes(item)?)
            .context("canonical ingestion item is not UTF-8")?;
        payload_bytes = payload_bytes
            .checked_add(u64::try_from(item_json.len()).context("item payload is too large")?)
            .context("serialized quarantine payload size overflow")?;
        let item_hash = sha256_bytes(item_json.as_bytes());
        item_payloads.push((item_json, item_hash));
    }

    let mut conn = open_connection(data_root, true)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .context("failed to start chunk transaction")?;
    expire_abandoned_mineral_ingestion_batches_in_tx(
        &tx,
        "system:quarantine-retention",
        limits.abandoned_hours,
    )?;
    let stored = load_stored_batch(&tx, batch_id)?.ok_or_else(|| {
        mineral_ingestion_problem(
            MineralIngestionProblemKind::NotFound,
            "batch_not_found",
            format!("mineral ingestion batch '{batch_id}' does not exist"),
        )
    })?;
    validate_mineral_manifest(&stored.manifest).map_err(|error| {
        mineral_ingestion_problem(
            MineralIngestionProblemKind::Invalid,
            "manifest_requires_restage",
            error.to_string(),
        )
    })?;
    if chunk.chunk_index >= stored.manifest.expected_chunk_count {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Invalid,
            "chunk_index_out_of_range",
            format!(
                "chunk index {} is outside expected range 0..{}",
                chunk.chunk_index, stored.manifest.expected_chunk_count
            ),
        ));
    }

    let existing_hash = tx
        .query_row(
            "SELECT content_hash FROM mineral_ingestion_chunks WHERE batch_id = ?1 AND chunk_index = ?2",
            params![batch_id, chunk.chunk_index as i64],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .context("failed to inspect existing ingestion chunk")?;
    if let Some(existing_hash) = existing_hash {
        if existing_hash != content_hash {
            return Err(mineral_ingestion_problem(
                MineralIngestionProblemKind::Conflict,
                "chunk_replay_conflict",
                format!(
                    "chunk {} is already stored with a different content hash",
                    chunk.chunk_index
                ),
            ));
        }
        let receipt = mineral_chunk_receipt(&tx, batch_id, chunk, content_hash, false)?;
        tx.commit()
            .context("failed to finish idempotent chunk retry")?;
        return Ok(receipt);
    }
    if stored.status != MineralIngestionBatchStatus::Receiving {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Conflict,
            "batch_not_receiving",
            format!("batch '{batch_id}' is already {}", stored.status.as_str()),
        ));
    }
    if stored.manifest.policy == MineralIngestionPolicy::CreateOnlyV1
        && chunk
            .items
            .iter()
            .any(|item| !item.official_identifiers.is_empty())
    {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Invalid,
            "policy_forbids_authority_identifiers",
            "create_only_v1 cannot assert authority-owned IMA identifiers",
        ));
    }
    if stored.manifest.policy == MineralIngestionPolicy::CreateOnlyV1
        && chunk
            .items
            .iter()
            .any(|item| !item.official_facts.is_empty())
    {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Invalid,
            "policy_forbids_authority_facts",
            "create_only_v1 cannot assert authority-owned mineral facts",
        ));
    }

    let batch_bytes = quarantine_payload_bytes(&tx, Some(batch_id), false)?;
    let global_bytes = quarantine_payload_bytes(&tx, None, true)?;
    enforce_quarantine_limits(batch_bytes, global_bytes, payload_bytes, limits)?;

    tx.execute(
        r#"
        INSERT INTO mineral_ingestion_chunks(
            batch_id, chunk_index, content_hash, payload_json, payload_bytes, item_count
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
        params![
            batch_id,
            chunk.chunk_index as i64,
            content_hash,
            payload_json,
            i64::try_from(payload_bytes).context("quarantine payload is too large")?,
            chunk.items.len() as i64,
        ],
    )
    .context("failed to store mineral ingestion chunk")?;
    for (item_index, (item, (item_json, item_hash))) in
        chunk.items.iter().zip(item_payloads).enumerate()
    {
        tx.execute(
            r#"
            INSERT INTO mineral_ingestion_items(
                batch_id, chunk_index, item_index, source_record_id,
                proposed_slug, normalized_name, item_hash, payload_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                batch_id,
                chunk.chunk_index as i64,
                item_index as i64,
                item.source_record_id,
                item.slug,
                normalize_identity_text(&item.canonical_name),
                item_hash,
                item_json,
            ],
        )
        .with_context(|| format!("failed to quarantine item {item_index}"))?;
    }
    append_mineral_ingestion_event(
        &tx,
        batch_id,
        "chunk_stored",
        actor,
        stored.manifest.policy,
        &stored.manifest_hash,
        None,
        &json!({
            "chunk_index": chunk.chunk_index,
            "content_hash": content_hash,
            "item_count": chunk.items.len(),
        }),
    )?;
    let receipt = mineral_chunk_receipt(&tx, batch_id, chunk, content_hash, true)?;
    tx.commit().context("failed to commit ingestion chunk")?;
    Ok(receipt)
}

/// Reclaims payloads from never-finalized receiving batches whose most recent
/// chunk activity is older than `older_than_hours`. The immutable manifest and
/// append-only audit events remain as a durable tombstone.
pub fn expire_abandoned_mineral_ingestion_batches(
    data_root: &Path,
    actor: &str,
    older_than_hours: u64,
) -> Result<usize> {
    validate_mineral_ingestion_actor(actor).map_err(|error| {
        mineral_ingestion_problem(
            MineralIngestionProblemKind::Invalid,
            "invalid_actor",
            error.to_string(),
        )
    })?;
    if !(1..=MAX_INGESTION_ABANDONED_HOURS).contains(&older_than_hours) {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Invalid,
            "invalid_retention_window",
            format!("retention must be between 1 and {MAX_INGESTION_ABANDONED_HOURS} hours"),
        ));
    }
    let mut conn = open_connection(data_root, true)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .context("failed to start quarantine expiry transaction")?;
    let expired = expire_abandoned_mineral_ingestion_batches_in_tx(&tx, actor, older_than_hours)?;
    tx.commit()
        .context("failed to commit quarantine expiry transaction")?;
    Ok(expired)
}

fn expire_abandoned_mineral_ingestion_batches_in_tx(
    conn: &Connection,
    actor: &str,
    older_than_hours: u64,
) -> Result<usize> {
    let hours = i64::try_from(older_than_hours).context("retention window is too large")?;
    let cutoff =
        (Utc::now() - ChronoDuration::hours(hours)).to_rfc3339_opts(SecondsFormat::Secs, true);
    let batch_ids = {
        let mut stmt = conn
            .prepare(
                r#"
                SELECT b.batch_id
                FROM mineral_ingestion_batches b
                LEFT JOIN mineral_ingestion_chunks c ON c.batch_id = b.batch_id
                WHERE b.status = 'receiving'
                GROUP BY b.batch_id, b.created_at
                HAVING datetime(COALESCE(MAX(c.created_at), b.created_at)) <= datetime(?1)
                ORDER BY b.created_at, b.batch_id
                "#,
            )
            .context("failed to prepare abandoned quarantine query")?;
        let rows = stmt.query_map(params![cutoff], |row| row.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for batch_id in &batch_ids {
        let stored = load_stored_batch(conn, batch_id)?
            .with_context(|| format!("abandoned batch '{batch_id}' disappeared"))?;
        let changed = conn.execute(
            r#"
            UPDATE mineral_ingestion_batches
            SET status = 'rejected', decision_actor = ?1,
                decision_note = 'expired_abandoned_batch',
                decided_at = CURRENT_TIMESTAMP
            WHERE batch_id = ?2 AND status = 'receiving' AND report_hash IS NULL
            "#,
            params![actor, batch_id],
        )?;
        if changed == 0 {
            continue;
        }
        let reclaimed_bytes = quarantine_payload_bytes(conn, Some(batch_id), false)?;
        let (chunk_count, record_count): (i64, i64) = conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(item_count), 0) FROM mineral_ingestion_chunks WHERE batch_id = ?1",
            params![batch_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        conn.execute(
            r#"
            UPDATE mineral_ingestion_batches
            SET compacted_chunk_count = compacted_chunk_count + ?1,
                compacted_record_count = compacted_record_count + ?2,
                compacted_payload_bytes = compacted_payload_bytes + ?3
            WHERE batch_id = ?4
            "#,
            params![
                chunk_count,
                record_count,
                i64::try_from(reclaimed_bytes).context("expired payload is too large")?,
                batch_id,
            ],
        )?;
        append_mineral_ingestion_event(
            conn,
            batch_id,
            "batch_expired",
            actor,
            stored.manifest.policy,
            &stored.manifest_hash,
            None,
            &json!({
                "retention_hours": older_than_hours,
                "reclaimed_payload_bytes": reclaimed_bytes,
            }),
        )?;
        conn.execute(
            "DELETE FROM mineral_ingestion_items WHERE batch_id = ?1",
            params![batch_id],
        )
        .context("failed to reclaim expired quarantine items")?;
        conn.execute(
            "DELETE FROM mineral_ingestion_chunks WHERE batch_id = ?1",
            params![batch_id],
        )
        .context("failed to reclaim expired quarantine chunks")?;
    }
    Ok(batch_ids.len())
}

fn compact_terminal_mineral_ingestion_payload(
    conn: &Connection,
    stored: &StoredMineralBatch,
    actor: &str,
) -> Result<u64> {
    let (chunk_count, record_count): (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(item_count), 0) FROM mineral_ingestion_chunks WHERE batch_id = ?1",
        params![stored.batch_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if chunk_count == 0 {
        return Ok(0);
    }
    let reclaimed_bytes = quarantine_payload_bytes(conn, Some(&stored.batch_id), false)?;
    let changed = conn
        .execute(
            r#"
        UPDATE mineral_ingestion_batches
        SET compacted_chunk_count = compacted_chunk_count + ?1,
            compacted_record_count = compacted_record_count + ?2,
            compacted_payload_bytes = compacted_payload_bytes + ?3
        WHERE batch_id = ?4
          AND status IN ('approved', 'rejected')
          AND decided_at IS NOT NULL
        "#,
            params![
                chunk_count,
                record_count,
                i64::try_from(reclaimed_bytes).context("terminal payload is too large")?,
                stored.batch_id,
            ],
        )
        .context("failed to persist terminal payload accounting")?;
    if changed != 1 {
        bail!("terminal payload compaction requires a durable terminal decision");
    }
    conn.execute(
        "DELETE FROM mineral_ingestion_items WHERE batch_id = ?1",
        params![stored.batch_id],
    )
    .context("failed to compact terminal quarantine items")?;
    conn.execute(
        "DELETE FROM mineral_ingestion_chunks WHERE batch_id = ?1",
        params![stored.batch_id],
    )
    .context("failed to compact terminal quarantine chunks")?;
    append_mineral_ingestion_event(
        conn,
        &stored.batch_id,
        "terminal_payload_compacted",
        actor,
        stored.manifest.policy,
        &stored.manifest_hash,
        stored.report_hash.as_deref(),
        &json!({
            "chunk_count": chunk_count,
            "record_count": record_count,
            "reclaimed_payload_bytes": reclaimed_bytes,
        }),
    )?;
    Ok(reclaimed_bytes)
}

fn quarantine_payload_bytes(
    conn: &Connection,
    batch_id: Option<&str>,
    active_only: bool,
) -> Result<u64> {
    let bytes: i64 = match (batch_id, active_only) {
        (Some(batch_id), _) => conn.query_row(
            r#"
            SELECT COALESCE(SUM(
                CASE WHEN c.payload_bytes > 0 THEN c.payload_bytes ELSE
                    length(CAST(c.payload_json AS BLOB)) + COALESCE((
                        SELECT SUM(length(CAST(i.payload_json AS BLOB)))
                        FROM mineral_ingestion_items i
                        WHERE i.batch_id = c.batch_id AND i.chunk_index = c.chunk_index
                    ), 0)
                END
            ), 0)
            FROM mineral_ingestion_chunks c
            WHERE c.batch_id = ?1
            "#,
            params![batch_id],
            |row| row.get(0),
        )?,
        (None, true) => conn.query_row(
            r#"
            SELECT COALESCE(SUM(
                CASE WHEN c.payload_bytes > 0 THEN c.payload_bytes ELSE
                    length(CAST(c.payload_json AS BLOB)) + COALESCE((
                        SELECT SUM(length(CAST(i.payload_json AS BLOB)))
                        FROM mineral_ingestion_items i
                        WHERE i.batch_id = c.batch_id AND i.chunk_index = c.chunk_index
                    ), 0)
                END
            ), 0)
            FROM mineral_ingestion_chunks c
            JOIN mineral_ingestion_batches b ON b.batch_id = c.batch_id
            WHERE b.status IN ('receiving', 'ready', 'needs_attention')
            "#,
            [],
            |row| row.get(0),
        )?,
        (None, false) => conn.query_row(
            r#"
            SELECT COALESCE(SUM(
                CASE WHEN c.payload_bytes > 0 THEN c.payload_bytes ELSE
                    length(CAST(c.payload_json AS BLOB)) + COALESCE((
                        SELECT SUM(length(CAST(i.payload_json AS BLOB)))
                        FROM mineral_ingestion_items i
                        WHERE i.batch_id = c.batch_id AND i.chunk_index = c.chunk_index
                    ), 0)
                END
            ), 0)
            FROM mineral_ingestion_chunks c
            "#,
            [],
            |row| row.get(0),
        )?,
    };
    u64::try_from(bytes).context("invalid quarantine payload byte count")
}

fn enforce_quarantine_limits(
    batch_bytes: u64,
    global_bytes: u64,
    payload_bytes: u64,
    limits: MineralIngestionLimits,
) -> Result<()> {
    if batch_bytes
        .checked_add(payload_bytes)
        .is_none_or(|total| total > limits.batch_max_bytes)
    {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Invalid,
            "batch_quota_exceeded",
            format!(
                "chunk would exceed the {}-byte per-batch quarantine limit",
                limits.batch_max_bytes
            ),
        ));
    }
    if global_bytes
        .checked_add(payload_bytes)
        .is_none_or(|total| total > limits.quarantine_max_bytes)
    {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Conflict,
            "quarantine_quota_exceeded",
            format!(
                "chunk would exceed the {}-byte active quarantine limit",
                limits.quarantine_max_bytes
            ),
        ));
    }
    Ok(())
}

pub fn get_mineral_ingestion_batch(
    data_root: &Path,
    batch_id: &str,
) -> Result<Option<MineralIngestionBatchDetail>> {
    validate_batch_id(batch_id).map_err(|error| {
        mineral_ingestion_problem(
            MineralIngestionProblemKind::Invalid,
            "invalid_batch_id",
            error.to_string(),
        )
    })?;
    let conn = open_connection(data_root, false)?;
    load_mineral_ingestion_batch(&conn, batch_id)
}

pub fn list_mineral_ingestion_batches(
    data_root: &Path,
    limit: usize,
    offset: usize,
) -> Result<MineralIngestionBatchPage> {
    let mut conn = open_connection(data_root, false)?;
    let limit = limit.clamp(1, MAX_SEARCH_RESULTS);
    let tx = conn
        .transaction()
        .context("failed to start mineral batch list transaction")?;
    let total: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM mineral_ingestion_batches",
            [],
            |row| row.get(0),
        )
        .context("failed to count mineral ingestion batches")?;
    let mut stmt = tx
        .prepare(
            r#"
            SELECT batch_id
            FROM mineral_ingestion_batches
            ORDER BY created_at DESC, batch_id DESC
            LIMIT ?1 OFFSET ?2
            "#,
        )
        .context("failed to prepare mineral batch list")?;
    let ids = stmt
        .query_map(params![limit as i64, offset as i64], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    let items = ids
        .iter()
        .map(|id| {
            load_mineral_ingestion_batch(&tx, id)?
                .with_context(|| format!("listed batch '{id}' disappeared"))
        })
        .collect::<Result<Vec<_>>>()?;
    tx.commit().context("failed to finish mineral batch list")?;
    Ok(MineralIngestionBatchPage {
        items,
        total_count: usize::try_from(total).context("invalid mineral batch count")?,
        limit,
        offset,
    })
}

/// Validates a complete quarantine batch and freezes its exact diff report.
/// It never changes public mineral records.
pub fn finalize_mineral_ingestion_batch(
    data_root: &Path,
    batch_id: &str,
    actor: &str,
) -> Result<MineralIngestionBatchDetail> {
    validate_batch_id(batch_id).map_err(|error| {
        mineral_ingestion_problem(
            MineralIngestionProblemKind::Invalid,
            "invalid_batch_id",
            error.to_string(),
        )
    })?;
    validate_mineral_ingestion_actor(actor).map_err(|error| {
        mineral_ingestion_problem(
            MineralIngestionProblemKind::Invalid,
            "invalid_actor",
            error.to_string(),
        )
    })?;
    let mut conn = open_connection(data_root, true)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .context("failed to start mineral batch finalization transaction")?;
    let stored = load_stored_batch(&tx, batch_id)?.ok_or_else(|| {
        mineral_ingestion_problem(
            MineralIngestionProblemKind::NotFound,
            "batch_not_found",
            format!("mineral ingestion batch '{batch_id}' does not exist"),
        )
    })?;
    // Never treat a finalize request for the pre-attribution schema as an
    // idempotent success. Historical terminal v1 rows remain available via
    // the read APIs, but every finalize attempt must be restaged as v2.
    validate_mineral_manifest(&stored.manifest).map_err(|error| {
        mineral_ingestion_problem(
            MineralIngestionProblemKind::Invalid,
            "manifest_requires_restage",
            error.to_string(),
        )
    })?;
    if stored.status != MineralIngestionBatchStatus::Receiving {
        if stored.report_hash.is_some() {
            let detail = load_mineral_ingestion_batch(&tx, batch_id)?
                .context("finalized batch disappeared")?;
            tx.commit()
                .context("failed to finish idempotent batch finalization")?;
            return Ok(detail);
        }
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Conflict,
            "batch_not_receiving",
            format!("batch '{batch_id}' is already {}", stored.status.as_str()),
        ));
    }
    let (chunk_count, item_count): (i64, i64) = tx
        .query_row(
            r#"
            SELECT COUNT(*), COALESCE(SUM(item_count), 0)
            FROM mineral_ingestion_chunks WHERE batch_id = ?1
            "#,
            params![batch_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .context("failed to count mineral ingestion payload")?;
    if usize::try_from(chunk_count).ok() != Some(stored.manifest.expected_chunk_count) {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Conflict,
            "incomplete_chunk_set",
            format!(
                "received {chunk_count} of {} expected chunks",
                stored.manifest.expected_chunk_count
            ),
        ));
    }
    if usize::try_from(item_count).ok() != Some(stored.manifest.expected_record_count) {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Conflict,
            "record_count_mismatch",
            format!(
                "received {item_count} of {} expected records",
                stored.manifest.expected_record_count
            ),
        ));
    }
    let mut chunk_stmt = tx
        .prepare(
            "SELECT chunk_index FROM mineral_ingestion_chunks WHERE batch_id = ?1 ORDER BY chunk_index",
        )
        .context("failed to inspect mineral chunk sequence")?;
    let chunk_indices = chunk_stmt
        .query_map(params![batch_id], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(chunk_stmt);
    if chunk_indices
        .iter()
        .enumerate()
        .any(|(expected, actual)| *actual != expected as i64)
    {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Conflict,
            "non_contiguous_chunks",
            "received chunks must cover every index from zero",
        ));
    }

    let items = load_quarantined_mineral_items(&tx, batch_id)?;
    let actual_records_hash = canonical_mineral_records_hash(&items)?;
    if actual_records_hash != stored.manifest.records_sha256 {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Conflict,
            "records_digest_mismatch",
            format!(
                "canonical records hash is {actual_records_hash}, not {}",
                stored.manifest.records_sha256
            ),
        ));
    }

    let mut report = build_mineral_ingestion_report(&tx, &stored, &items)?;
    report.generated_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let report_json = String::from_utf8(canonical_json_bytes(&report)?)
        .context("canonical ingestion report is not UTF-8")?;
    let report_hash = sha256_bytes(report_json.as_bytes());
    for report_item in &report.items {
        let material_id = report_item
            .material_public_id
            .as_deref()
            .map(|public_id| {
                tx.query_row(
                    "SELECT id FROM materials WHERE public_id = ?1",
                    params![public_id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .context("failed to resolve report material")
            })
            .transpose()?
            .flatten();
        tx.execute(
            r#"
            INSERT INTO mineral_ingestion_report_items(
                batch_id, source_record_id, proposed_slug, resolved_slug,
                material_id, target_baseline_hash, classification, severity, code, message,
                critical_formula_change, critical_validity_change
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
            params![
                batch_id,
                report_item.source_record_id,
                report_item.proposed_slug,
                report_item.resolved_slug,
                material_id,
                report_item.target_baseline_hash,
                report_item.classification.as_str(),
                report_item.severity,
                report_item.code,
                report_item.message,
                i64::from(report_item.critical_formula_change),
                i64::from(report_item.critical_validity_change),
            ],
        )
        .context("failed to persist immutable report item")?;
    }
    let status = if report.summary.conflict_count > 0 {
        MineralIngestionBatchStatus::NeedsAttention
    } else {
        MineralIngestionBatchStatus::Ready
    };
    let changed = tx
        .execute(
            r#"
            UPDATE mineral_ingestion_batches
            SET status = ?1, report_hash = ?2, report_json = ?3,
                finalized_at = CURRENT_TIMESTAMP
            WHERE batch_id = ?4 AND status = 'receiving' AND report_hash IS NULL
            "#,
            params![status.as_str(), report_hash, report_json, batch_id],
        )
        .context("failed to freeze mineral ingestion report")?;
    if changed != 1 {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Conflict,
            "batch_changed_during_finalize",
            "the ingestion batch changed during finalization",
        ));
    }
    append_mineral_ingestion_event(
        &tx,
        batch_id,
        "batch_finalized",
        actor,
        stored.manifest.policy,
        &stored.manifest_hash,
        Some(&report_hash),
        &serde_json::to_value(&report.summary)?,
    )?;
    let detail = load_mineral_ingestion_batch(&tx, batch_id)?
        .context("finalized mineral ingestion batch disappeared")?;
    tx.commit()
        .context("failed to commit mineral batch finalization")?;
    Ok(detail)
}

/// Activates the exact reviewed report in one IMMEDIATE transaction. A
/// separate read connection takes the verified SQLite snapshot while that
/// transaction holds the sole writer reservation and before public changes.
pub fn approve_mineral_ingestion_batch(
    data_root: &Path,
    batch_id: &str,
    actor: &str,
    request: &MineralBatchDecisionRequest,
) -> Result<MineralBatchDecisionOutcome> {
    validate_batch_decision(batch_id, actor, request)?;
    let backups_root = data_root.join("backups");
    fs::create_dir_all(&backups_root).with_context(|| {
        format!(
            "failed to create private backup directory {}",
            backups_root.display()
        )
    })?;

    let mut conn = open_connection(data_root, true)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .context("failed to start mineral batch activation transaction")?;
    let stored = load_stored_batch(&tx, batch_id)?.ok_or_else(|| {
        mineral_ingestion_problem(
            MineralIngestionProblemKind::NotFound,
            "batch_not_found",
            format!("mineral ingestion batch '{batch_id}' does not exist"),
        )
    })?;
    verify_batch_decision_contract(&stored, request)?;
    if stored.status == MineralIngestionBatchStatus::Approved {
        let outcome = load_idempotent_decision_outcome(&tx, &stored, false)?;
        tx.commit()
            .context("failed to finish idempotent mineral batch approval")?;
        return Ok(outcome);
    }
    if stored.status == MineralIngestionBatchStatus::Rejected {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Conflict,
            "decision_conflict",
            "this batch was already rejected",
        ));
    }
    if stored.status != MineralIngestionBatchStatus::Ready {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Conflict,
            "batch_not_approvable",
            format!(
                "batch '{batch_id}' must be ready and is {}",
                stored.status.as_str()
            ),
        ));
    }
    validate_mineral_manifest(&stored.manifest).map_err(|error| {
        mineral_ingestion_problem(
            MineralIngestionProblemKind::Invalid,
            "manifest_requires_restage",
            error.to_string(),
        )
    })?;
    let current_head = mineral_dataset_head(&tx, &stored.manifest.dataset.key)?;
    if current_head != stored.manifest.base_batch_id || current_head != request.base_batch_id {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Conflict,
            "stale_base_batch",
            format!("dataset head is now {current_head:?}"),
        ));
    }
    let report = load_verified_ingestion_report(&stored)?;
    if report.summary.conflict_count > 0 {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Conflict,
            "report_contains_conflicts",
            "a report containing conflicts cannot be activated",
        ));
    }
    verify_mineral_ingestion_authority(&tx, &stored)?;
    let items = load_quarantined_mineral_items(&tx, batch_id)?;
    if report
        .items
        .iter()
        .any(|item| item.target_baseline_hash.is_none())
    {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Conflict,
            "stale_report",
            "the frozen report predates target preconditions and must be restaged",
        ));
    }
    let fresh_report = build_mineral_ingestion_report(&tx, &stored, &items)?;
    let frozen_semantics = canonical_json_bytes(&(report.summary.clone(), &report.items))?;
    let fresh_semantics =
        canonical_json_bytes(&(fresh_report.summary.clone(), &fresh_report.items))?;
    if frozen_semantics != fresh_semantics {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Conflict,
            "stale_report",
            "a reviewed target or collision precondition changed after finalization",
        ));
    }
    // SQLite cannot back up from the same connection while its write
    // transaction is active. A separate read-only connection can snapshot the
    // last committed state while this IMMEDIATE transaction holds the sole
    // writer reservation, so there is no write gap before activation.
    let backup_source = open_connection(data_root, false)?;
    let mut backup = create_pre_activation_backup(
        &backup_source,
        &backups_root,
        batch_id,
        &request.report_hash,
    )?;
    drop(backup_source);
    tx.execute(
        r#"
        INSERT INTO mineral_ingestion_backups(batch_id, relative_path, sha256, status)
        VALUES (?1, ?2, ?3, 'completed')
        "#,
        params![batch_id, backup.relative_path, backup.sha256],
    )
    .context("failed to record pre-activation backup")?;
    append_mineral_ingestion_event(
        &tx,
        batch_id,
        "pre_activation_backup_completed",
        actor,
        stored.manifest.policy,
        &stored.manifest_hash,
        stored.report_hash.as_deref(),
        &json!({
            "relative_path": backup.relative_path,
            "sha256": backup.sha256,
        }),
    )?;

    bind_mineral_ingestion_authority(&tx, &stored)?;
    let items_by_source = items
        .iter()
        .map(|item| (item.source_record_id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let mut applied_create_count = 0usize;
    let mut applied_adopt_count = 0usize;
    let mut applied_update_count = 0usize;
    let mut unchanged_count = 0usize;
    let mut retired_offer_count = 0usize;
    for report_item in &report.items {
        let Some(item) = items_by_source
            .get(report_item.source_record_id.as_str())
            .copied()
        else {
            if report_item.classification == MineralIngestionClassification::Missing {
                continue;
            }
            bail!(
                "report item '{}' has no quarantined payload",
                report_item.source_record_id
            );
        };
        match report_item.classification {
            MineralIngestionClassification::Create => {
                let material_id = create_bulk_mineral(&tx, &stored, item)?;
                insert_external_identity_mapping(&tx, &stored, item, material_id)?;
                apply_bulk_owned_fields(&tx, &stored, item, material_id, None)?;
                applied_create_count += 1;
            }
            MineralIngestionClassification::Adopt => {
                let material_id = resolve_report_material_id(&tx, report_item)?;
                insert_external_identity_mapping(&tx, &stored, item, material_id)?;
                let retired =
                    apply_bulk_owned_fields(&tx, &stored, item, material_id, Some(report_item))?;
                retired_offer_count += retired;
                applied_adopt_count += 1;
            }
            MineralIngestionClassification::Update => {
                let material_id = resolve_report_material_id(&tx, report_item)?;
                let retired =
                    apply_bulk_owned_fields(&tx, &stored, item, material_id, Some(report_item))?;
                retired_offer_count += retired;
                applied_update_count += 1;
            }
            MineralIngestionClassification::Unchanged => {
                let material_id = resolve_report_material_id(&tx, report_item)?;
                refresh_unchanged_bulk_provenance(&tx, &stored, item, material_id)?;
                unchanged_count += 1;
            }
            MineralIngestionClassification::Missing => {}
            MineralIngestionClassification::Conflict => {
                bail!("a conflict reached the activation loop")
            }
        }
    }

    tx.execute(
        r#"
        INSERT INTO mineral_dataset_heads(dataset_key, batch_id)
        VALUES (?1, ?2)
        ON CONFLICT(dataset_key) DO UPDATE SET
            batch_id = excluded.batch_id,
            updated_at = CURRENT_TIMESTAMP
        "#,
        params![stored.manifest.dataset.key, batch_id],
    )
    .context("failed to advance mineral dataset head")?;
    let changed = tx
        .execute(
            r#"
            UPDATE mineral_ingestion_batches
            SET status = 'approved', decision_actor = ?1, decision_note = ?2,
                decided_at = CURRENT_TIMESTAMP
            WHERE batch_id = ?3 AND status = 'ready'
            "#,
            params![actor, request.note.trim(), batch_id],
        )
        .context("failed to approve mineral ingestion batch")?;
    if changed != 1 {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Conflict,
            "batch_changed_during_activation",
            "the batch changed during activation",
        ));
    }
    append_mineral_ingestion_event(
        &tx,
        batch_id,
        "batch_approved",
        actor,
        stored.manifest.policy,
        &stored.manifest_hash,
        stored.report_hash.as_deref(),
        &json!({
            "create_count": applied_create_count,
            "adopt_count": applied_adopt_count,
            "update_count": applied_update_count,
            "unchanged_count": unchanged_count,
            "retired_offer_count": retired_offer_count,
            "base_batch_id": request.base_batch_id,
        }),
    )?;
    compact_terminal_mineral_ingestion_payload(&tx, &stored, actor)?;
    let decided_at: String = tx
        .query_row(
            "SELECT decided_at FROM mineral_ingestion_batches WHERE batch_id = ?1",
            params![batch_id],
            |row| row.get(0),
        )
        .context("failed to load approval timestamp")?;
    let outcome = MineralBatchDecisionOutcome {
        batch_id: batch_id.to_string(),
        status: MineralIngestionBatchStatus::Approved,
        changed: true,
        applied_create_count,
        applied_adopt_count,
        applied_update_count,
        unchanged_count,
        retired_offer_count,
        backup_path: Some(backup.relative_path.clone()),
        backup_sha256: Some(backup.sha256.clone()),
        decided_at,
    };
    tx.commit()
        .context("failed to commit mineral batch activation")?;
    backup.keep = true;
    if let Err(error) = prune_pre_activation_backups(data_root, PRE_ACTIVATION_BACKUP_RETENTION) {
        tracing::warn!(error = %error, "pre-activation backup retention cleanup failed");
    }
    Ok(outcome)
}

pub fn reject_mineral_ingestion_batch(
    data_root: &Path,
    batch_id: &str,
    actor: &str,
    request: &MineralBatchDecisionRequest,
) -> Result<MineralBatchDecisionOutcome> {
    validate_batch_decision(batch_id, actor, request)?;
    let mut conn = open_connection(data_root, true)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .context("failed to start mineral batch rejection transaction")?;
    let stored = load_stored_batch(&tx, batch_id)?.ok_or_else(|| {
        mineral_ingestion_problem(
            MineralIngestionProblemKind::NotFound,
            "batch_not_found",
            format!("mineral ingestion batch '{batch_id}' does not exist"),
        )
    })?;
    verify_batch_decision_contract(&stored, request)?;
    if stored.status == MineralIngestionBatchStatus::Rejected {
        let outcome = load_idempotent_decision_outcome(&tx, &stored, false)?;
        tx.commit()
            .context("failed to finish idempotent mineral batch rejection")?;
        return Ok(outcome);
    }
    if stored.status == MineralIngestionBatchStatus::Approved {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Conflict,
            "decision_conflict",
            "this batch was already approved",
        ));
    }
    if !matches!(
        stored.status,
        MineralIngestionBatchStatus::Ready | MineralIngestionBatchStatus::NeedsAttention
    ) {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Conflict,
            "batch_not_decidable",
            "only a finalized batch can be rejected",
        ));
    }
    let report = load_verified_ingestion_report(&stored)?;
    let changed = tx
        .execute(
            r#"
            UPDATE mineral_ingestion_batches
            SET status = 'rejected', decision_actor = ?1, decision_note = ?2,
                decided_at = CURRENT_TIMESTAMP
            WHERE batch_id = ?3 AND status IN ('ready', 'needs_attention')
            "#,
            params![actor, request.note.trim(), batch_id],
        )
        .context("failed to reject mineral ingestion batch")?;
    if changed != 1 {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Conflict,
            "batch_changed_during_rejection",
            "the batch changed during rejection",
        ));
    }
    append_mineral_ingestion_event(
        &tx,
        batch_id,
        "batch_rejected",
        actor,
        stored.manifest.policy,
        &stored.manifest_hash,
        stored.report_hash.as_deref(),
        &json!({
            "base_batch_id": request.base_batch_id,
            "report_summary": report.summary,
        }),
    )?;
    compact_terminal_mineral_ingestion_payload(&tx, &stored, actor)?;
    let rejected = load_stored_batch(&tx, batch_id)?.context("rejected batch disappeared")?;
    let outcome = load_idempotent_decision_outcome(&tx, &rejected, true)?;
    tx.commit()
        .context("failed to commit mineral batch rejection")?;
    Ok(outcome)
}

fn validate_batch_decision(
    batch_id: &str,
    actor: &str,
    request: &MineralBatchDecisionRequest,
) -> Result<()> {
    validate_batch_id(batch_id).map_err(|error| {
        mineral_ingestion_problem(
            MineralIngestionProblemKind::Invalid,
            "invalid_decision",
            error.to_string(),
        )
    })?;
    validate_mineral_ingestion_actor(actor).map_err(|error| {
        mineral_ingestion_problem(
            MineralIngestionProblemKind::Invalid,
            "invalid_decision",
            error.to_string(),
        )
    })?;
    validate_sha256("decision manifest_hash", &request.manifest_hash).map_err(|error| {
        mineral_ingestion_problem(
            MineralIngestionProblemKind::Invalid,
            "invalid_decision",
            error.to_string(),
        )
    })?;
    validate_sha256("decision report_hash", &request.report_hash).map_err(|error| {
        mineral_ingestion_problem(
            MineralIngestionProblemKind::Invalid,
            "invalid_decision",
            error.to_string(),
        )
    })?;
    if let Some(base) = request.base_batch_id.as_deref() {
        validate_batch_id(base).map_err(|error| {
            mineral_ingestion_problem(
                MineralIngestionProblemKind::Invalid,
                "invalid_decision",
                error.to_string(),
            )
        })?;
    }
    validate_text("decision note", &request.note, 1, 2_000).map_err(|error| {
        mineral_ingestion_problem(
            MineralIngestionProblemKind::Invalid,
            "invalid_decision",
            error.to_string(),
        )
    })?;
    Ok(())
}

fn verify_batch_decision_contract(
    stored: &StoredMineralBatch,
    request: &MineralBatchDecisionRequest,
) -> Result<()> {
    if stored.manifest_hash != request.manifest_hash {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Conflict,
            "manifest_hash_mismatch",
            "decision does not name the stored manifest hash",
        ));
    }
    if stored.report_hash.as_deref() != Some(request.report_hash.as_str()) {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Conflict,
            "report_hash_mismatch",
            "decision does not name the stored report hash",
        ));
    }
    if stored.manifest.base_batch_id != request.base_batch_id {
        return Err(mineral_ingestion_problem(
            MineralIngestionProblemKind::Conflict,
            "base_batch_mismatch",
            "decision does not acknowledge the manifest base batch",
        ));
    }
    Ok(())
}

fn load_verified_ingestion_report(stored: &StoredMineralBatch) -> Result<MineralIngestionReport> {
    let report_json = stored
        .report_json
        .as_deref()
        .context("finalized batch has no report payload")?;
    let report_hash = stored
        .report_hash
        .as_deref()
        .context("finalized batch has no report hash")?;
    if sha256_bytes(report_json.as_bytes()) != report_hash {
        bail!("stored mineral ingestion report content address is corrupt");
    }
    let report: MineralIngestionReport =
        serde_json::from_str(report_json).context("stored mineral ingestion report is invalid")?;
    if report.batch_id != stored.batch_id || report.manifest_hash != stored.manifest_hash {
        bail!("stored mineral ingestion report identity is inconsistent");
    }
    Ok(report)
}

fn create_bulk_mineral(
    conn: &Connection,
    stored: &StoredMineralBatch,
    item: &MineralIngestionItem,
) -> Result<i64> {
    let attribution = required_manifest_attribution(&stored.manifest)?;
    let verification_status = match stored.manifest.policy {
        MineralIngestionPolicy::ImaIdentityV1 => "sourced",
        MineralIngestionPolicy::CreateOnlyV1 => "draft",
    };
    conn.execute(
        r#"
        INSERT INTO materials(
            public_id, slug, record_type, canonical_name, formula, description,
            mineral_family, identifiers_json, synonyms_json, properties_json,
            safety_json, search_text, verification_status, data_quality_score,
            source_kind, license_spdx, publication_status,
            nomenclature_status, is_valid_species
        ) VALUES (
            'mat_' || lower(hex(randomblob(16))),
            ?1, 'mineral', ?2, ?3, '', '', '{}', '[]', '{}', '{}', ?4,
            ?5, 0.0, 'registry_import', ?6, 'published', ?7, ?8
        )
        "#,
        params![
            item.slug,
            item.canonical_name.trim(),
            item.formula.trim(),
            format!("{} {}", item.canonical_name.trim(), item.formula.trim()),
            verification_status,
            attribution.derived_output_license_spdx,
            item.nomenclature_status,
            i64::from(item.is_valid_species),
        ],
    )
    .with_context(|| format!("failed to create mineral '{}'", item.slug))?;
    Ok(conn.last_insert_rowid())
}

fn insert_external_identity_mapping(
    conn: &Connection,
    stored: &StoredMineralBatch,
    item: &MineralIngestionItem,
    material_id: i64,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO mineral_external_identities(
            dataset_key, source_record_id, material_id, created_batch_id
        ) VALUES (?1, ?2, ?3, ?4)
        "#,
        params![
            stored.manifest.dataset.key,
            item.source_record_id,
            material_id,
            stored.batch_id,
        ],
    )
    .context("failed to create stable external identity mapping")?;
    Ok(())
}

fn resolve_report_material_id(
    conn: &Connection,
    report_item: &MineralIngestionReportItem,
) -> Result<i64> {
    let public_id = report_item
        .material_public_id
        .as_deref()
        .context("existing report item has no immutable public id")?;
    conn.query_row(
        "SELECT id FROM materials WHERE public_id = ?1 AND record_type = 'mineral'",
        params![public_id],
        |row| row.get(0),
    )
    .optional()
    .context("failed to resolve report material")?
    .with_context(|| format!("report material '{public_id}' no longer exists"))
}

fn apply_bulk_owned_fields(
    conn: &Connection,
    stored: &StoredMineralBatch,
    item: &MineralIngestionItem,
    material_id: i64,
    report_item: Option<&MineralIngestionReportItem>,
) -> Result<usize> {
    let (old_name, identifiers_json): (String, String) = conn
        .query_row(
            "SELECT canonical_name, identifiers_json FROM materials WHERE id = ?1",
            params![material_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .context("failed to load existing owned mineral fields")?;
    let old_identifier_keys = conn
        .prepare(
            r#"
            SELECT identifier_key FROM mineral_dataset_identifiers
            WHERE dataset_key = ?1 AND material_id = ?2
            "#,
        )?
        .query_map(params![stored.manifest.dataset.key, material_id], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut identifiers = serde_json::from_str::<Value>(&identifiers_json)
        .context("material identifiers are invalid JSON")?;
    let object = identifiers
        .as_object_mut()
        .context("material identifiers must be a JSON object")?;
    for key in old_identifier_keys {
        object.remove(&key);
    }
    for (key, value) in &item.official_identifiers {
        object.insert(key.clone(), Value::String(value.trim().to_string()));
    }
    let identifiers_json = serde_json::to_string(&identifiers)?;

    if normalize_identity_text(&old_name) != normalize_identity_text(&item.canonical_name) {
        conn.execute(
            r#"
            INSERT OR IGNORE INTO material_aliases(
                material_id, alias, alias_normalized, alias_type, origin,
                dataset_key, source_release_id
            ) VALUES (?1, ?2, ?3, 'former_name', 'bulk_history', ?4, ?5)
            "#,
            params![
                material_id,
                old_name.trim(),
                normalize_alias(&old_name),
                stored.manifest.dataset.key,
                stored.batch_id,
            ],
        )
        .context("failed to retain former canonical mineral name")?;
    }
    conn.execute(
        r#"
        UPDATE materials
        SET canonical_name = ?1, formula = ?2, nomenclature_status = ?3,
            is_valid_species = ?4, identifiers_json = ?5,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = ?6 AND record_type = 'mineral'
        "#,
        params![
            item.canonical_name.trim(),
            item.formula.trim(),
            item.nomenclature_status,
            i64::from(item.is_valid_species),
            identifiers_json,
            material_id,
        ],
    )
    .context("failed to apply server-owned mineral identity fields")?;

    conn.execute(
        "DELETE FROM mineral_dataset_identifiers WHERE dataset_key = ?1 AND material_id = ?2",
        params![stored.manifest.dataset.key, material_id],
    )
    .context("failed to replace dataset-owned identifiers")?;
    for (key, value) in &item.official_identifiers {
        conn.execute(
            r#"
            INSERT INTO mineral_dataset_identifiers(
                dataset_key, material_id, identifier_key, identifier_value,
                normalized_value, source_release_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                stored.manifest.dataset.key,
                material_id,
                key,
                value.trim(),
                normalize_authority_identifier(key, value),
                stored.batch_id,
            ],
        )
        .context("failed to store dataset-owned identifier")?;
    }

    conn.execute(
        r#"
        DELETE FROM material_aliases
        WHERE material_id = ?1 AND origin = 'bulk_dataset' AND dataset_key = ?2
        "#,
        params![material_id, stored.manifest.dataset.key],
    )
    .context("failed to replace dataset-owned aliases")?;
    for synonym in &item.synonyms {
        conn.execute(
            r#"
            INSERT OR IGNORE INTO material_aliases(
                material_id, alias, alias_normalized, alias_type, origin,
                dataset_key, source_release_id
            ) VALUES (?1, ?2, ?3, 'official_synonym', 'bulk_dataset', ?4, ?5)
            "#,
            params![
                material_id,
                synonym.trim(),
                normalize_alias(synonym),
                stored.manifest.dataset.key,
                stored.batch_id,
            ],
        )
        .context("failed to store dataset-owned synonym")?;
    }
    replace_bulk_official_facts(conn, stored, item, material_id)?;
    replace_bulk_identity_evidence(conn, stored, item, material_id)?;
    rebuild_material_search_text(conn, material_id)?;

    let retire_offers = report_item.is_some_and(|report_item| {
        report_item.critical_formula_change || report_item.critical_validity_change
    });
    if retire_offers {
        conn.execute(
            r#"
            UPDATE offers
            SET active = 0, updated_at = CURRENT_TIMESTAMP
            WHERE material_id = ?1 AND active = 1
            "#,
            params![material_id],
        )
        .context("failed to retire offers after identity-critical change")
    } else {
        Ok(0)
    }
}

fn replace_bulk_official_facts(
    conn: &Connection,
    stored: &StoredMineralBatch,
    item: &MineralIngestionItem,
    material_id: i64,
) -> Result<()> {
    conn.execute(
        "DELETE FROM mineral_dataset_facts WHERE dataset_key = ?1 AND material_id = ?2",
        params![stored.manifest.dataset.key, material_id],
    )
    .context("failed to replace dataset-owned mineral facts")?;
    for (key, value) in item.official_facts.as_nonempty_map() {
        conn.execute(
            r#"
            INSERT INTO mineral_dataset_facts(
                dataset_key, material_id, fact_key, fact_value, source_release_id
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                stored.manifest.dataset.key,
                material_id,
                key,
                value,
                stored.batch_id,
            ],
        )
        .context("failed to store dataset-owned mineral fact")?;
    }
    Ok(())
}

fn replace_bulk_identity_evidence(
    conn: &Connection,
    stored: &StoredMineralBatch,
    item: &MineralIngestionItem,
    material_id: i64,
) -> Result<()> {
    let attribution = required_manifest_attribution(&stored.manifest)?;
    conn.execute(
        "DELETE FROM material_evidence WHERE material_id = ?1 AND dataset_key = ?2",
        params![material_id, stored.manifest.dataset.key],
    )
    .context("failed to replace dataset-owned identity evidence")?;
    let canonical_url = canonicalize_evidence_url(&stored.manifest.artifact.url)?;
    let retrieved_at = normalize_timestamp(
        "manifest.retrieval.retrieved_at",
        &stored.manifest.retrieval.retrieved_at,
    )?;
    let source_title = attribution.work_title.trim();
    conn.execute(
        r#"
        INSERT INTO evidence_sources(
            canonical_url, title, publisher, license_spdx, retrieved_at,
            content_hash
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(canonical_url) DO NOTHING
        "#,
        params![
            canonical_url,
            source_title,
            attribution.attribution_party,
            stored.manifest.source.license_spdx,
            retrieved_at,
            stored.manifest.artifact.sha256,
        ],
    )
    .context("failed to store official release evidence source")?;
    let source_id: i64 = conn
        .query_row(
            "SELECT id FROM evidence_sources WHERE canonical_url = ?1",
            params![canonical_url],
            |row| row.get(0),
        )
        .context("failed to resolve official release evidence source")?;
    let claim_scope = dataset_claim_scope(&stored.manifest.dataset.key);
    let claim_json = serde_json::to_string(&json!({
        "source_record_id": item.source_record_id,
        "source_locator": item.source_locator,
        "canonical_name": item.canonical_name,
        "formula": item.formula,
        "nomenclature_status": item.nomenclature_status,
        "is_valid_species": item.is_valid_species,
        "official_facts": item.official_facts,
        "release_version": stored.manifest.release.version,
    }))?;
    conn.execute(
        r#"
        INSERT INTO material_evidence(
            material_id, source_id, claim_scope, claim_json, confidence,
            review_status, source_title, source_publisher,
            source_license_spdx, source_retrieved_at, source_content_hash,
            source_attribution_party, source_work_title, source_work_url,
            source_license_url, source_changes_notice,
            source_no_endorsement_notice,
            source_derived_output_license_spdx,
            dataset_key, source_release_id
        ) VALUES (
            ?1, ?2, ?3, ?4, 0.95, 'reviewed', ?5, ?6, ?7, ?8, ?9,
            ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18
        )
        "#,
        params![
            material_id,
            source_id,
            claim_scope,
            claim_json,
            source_title,
            attribution.attribution_party,
            stored.manifest.source.license_spdx,
            retrieved_at,
            stored.manifest.artifact.sha256,
            attribution.attribution_party,
            attribution.work_title,
            attribution.work_url,
            attribution.license_url,
            attribution.changes_notice,
            attribution.no_endorsement_notice,
            attribution.derived_output_license_spdx,
            stored.manifest.dataset.key,
            stored.batch_id,
        ],
    )
    .context("failed to associate official release evidence")?;
    Ok(())
}

fn refresh_unchanged_bulk_provenance(
    conn: &Connection,
    stored: &StoredMineralBatch,
    item: &MineralIngestionItem,
    material_id: i64,
) -> Result<()> {
    conn.execute(
        r#"
        UPDATE mineral_dataset_identifiers
        SET source_release_id = ?1
        WHERE dataset_key = ?2 AND material_id = ?3
        "#,
        params![stored.batch_id, stored.manifest.dataset.key, material_id],
    )
    .context("failed to refresh identifier release provenance")?;
    conn.execute(
        r#"
        UPDATE material_aliases
        SET source_release_id = ?1
        WHERE material_id = ?2 AND origin = 'bulk_dataset' AND dataset_key = ?3
        "#,
        params![stored.batch_id, material_id, stored.manifest.dataset.key],
    )
    .context("failed to refresh synonym release provenance")?;
    conn.execute(
        r#"
        UPDATE mineral_dataset_facts
        SET source_release_id = ?1, updated_at = CURRENT_TIMESTAMP
        WHERE dataset_key = ?2 AND material_id = ?3
        "#,
        params![stored.batch_id, stored.manifest.dataset.key, material_id],
    )
    .context("failed to refresh official fact release provenance")?;
    replace_bulk_identity_evidence(conn, stored, item, material_id)
}

fn dataset_claim_scope(dataset_key: &str) -> String {
    let mut key = dataset_key
        .chars()
        .map(|character| {
            if character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .take(80)
        .collect::<String>();
    if key.is_empty() {
        key.push_str("dataset");
    }
    let hash = sha256_bytes(dataset_key.as_bytes());
    format!("identifiers.{key}_{}", &hash[7..15])
}

fn rebuild_material_search_text(conn: &Connection, material_id: i64) -> Result<()> {
    let (name, formula, family, cas, identifiers): (
        String,
        String,
        String,
        Option<String>,
        String,
    ) = conn
        .query_row(
            r#"
            SELECT canonical_name, formula, mineral_family, cas_number,
                   identifiers_json
            FROM materials WHERE id = ?1
            "#,
            params![material_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .context("failed to load material search fields")?;
    let mut alias_stmt = conn
        .prepare("SELECT alias FROM material_aliases WHERE material_id = ?1 ORDER BY id")
        .context("failed to prepare material alias search query")?;
    let aliases = alias_stmt
        .query_map(params![material_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let official_fact_text: String = conn
        .query_row(
            r#"
            SELECT COALESCE(group_concat(fact_value, ' '), '')
            FROM mineral_dataset_facts
            WHERE material_id = ?1 AND fact_key = 'discovery_country'
            "#,
            params![material_id],
            |row| row.get(0),
        )
        .context("failed to load official mineral search facts")?;
    let search_text = [
        name,
        formula,
        family,
        cas.unwrap_or_default(),
        identifiers,
        aliases.join(" "),
        official_fact_text,
    ]
    .join(" ");
    conn.execute(
        "UPDATE materials SET search_text = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
        params![search_text, material_id],
    )
    .context("failed to rebuild material search text")?;
    Ok(())
}

#[derive(Debug)]
struct PreActivationBackup {
    path: std::path::PathBuf,
    relative_path: String,
    sha256: String,
    keep: bool,
}

impl Drop for PreActivationBackup {
    fn drop(&mut self) {
        if !self.keep {
            remove_sqlite_backup_files(&self.path);
        }
    }
}

fn remove_sqlite_backup_files(path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let mut candidate = path.as_os_str().to_os_string();
        candidate.push(suffix);
        let _ = fs::remove_file(std::path::PathBuf::from(candidate));
    }
}

fn create_pre_activation_backup(
    conn: &Connection,
    backups_root: &Path,
    batch_id: &str,
    report_hash: &str,
) -> Result<PreActivationBackup> {
    let mut nonce = [0_u8; 8];
    getrandom::getrandom(&mut nonce)
        .map_err(|error| anyhow::anyhow!("failed to generate backup filename nonce: {error}"))?;
    let nonce = nonce
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let filename = format!(
        "pre-activation-{}-{}-{nonce}.db",
        &batch_id[6..18],
        &report_hash[7..19]
    );
    let path = backups_root.join(&filename);
    let mut backup = PreActivationBackup {
        path,
        relative_path: format!("backups/{filename}"),
        sha256: String::new(),
        keep: false,
    };
    conn.backup(DatabaseName::Main, &backup.path, None)
        .with_context(|| {
            format!(
                "failed to create pre-activation backup {}",
                backup.path.display()
            )
        })?;
    let backup_conn = Connection::open_with_flags(
        &backup.path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| {
        format!(
            "failed to open pre-activation backup {}",
            backup.path.display()
        )
    })?;
    // A database-wide quick_check invokes FTS5's write-style integrity hook,
    // which SQLite refuses on a read-only verification connection. Check every
    // durable core table explicitly, then prove the FTS table is readable.
    for table in [
        "materials",
        "mineral_ingestion_batches",
        "mineral_ingestion_chunks",
        "mineral_ingestion_items",
        "mineral_external_identities",
        "mineral_ingestion_authorities",
        "material_evidence",
    ] {
        let quick_check: String = backup_conn
            .query_row(&format!("PRAGMA quick_check('{table}')"), [], |row| {
                row.get(0)
            })
            .with_context(|| format!("pre-activation backup quick_check failed for {table}"))?;
        if quick_check != "ok" {
            bail!("pre-activation backup failed quick_check for {table}: {quick_check}");
        }
    }
    backup_conn
        .query_row("SELECT COUNT(*) FROM material_search", [], |row| {
            row.get::<_, i64>(0)
        })
        .context("pre-activation backup full-text index is unreadable")?;
    drop(backup_conn);
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = backup.path.as_os_str().to_os_string();
        sidecar.push(suffix);
        let _ = fs::remove_file(std::path::PathBuf::from(sidecar));
    }
    backup.sha256 = sha256_file(&backup.path)?;
    Ok(backup)
}

fn load_idempotent_decision_outcome(
    conn: &Connection,
    stored: &StoredMineralBatch,
    changed: bool,
) -> Result<MineralBatchDecisionOutcome> {
    let report = load_verified_ingestion_report(stored)?;
    let counts = if stored.status == MineralIngestionBatchStatus::Approved {
        conn.query_row(
            r#"
            SELECT detail_json
            FROM mineral_ingestion_events
            WHERE batch_id = ?1 AND event_type = 'batch_approved'
            ORDER BY id DESC LIMIT 1
            "#,
            params![stored.batch_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .context("failed to load approval event")?
        .map(|raw| serde_json::from_str::<Value>(&raw))
        .transpose()
        .context("approval event detail is invalid")?
    } else {
        None
    };
    let count = |key: &str| -> usize {
        counts
            .as_ref()
            .and_then(|value| value.get(key))
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(0)
    };
    let backup = conn
        .query_row(
            "SELECT relative_path, sha256 FROM mineral_ingestion_backups WHERE batch_id = ?1 AND status = 'completed'",
            params![stored.batch_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .context("failed to load decision backup")?;
    Ok(MineralBatchDecisionOutcome {
        batch_id: stored.batch_id.clone(),
        status: stored.status,
        changed,
        applied_create_count: count("create_count"),
        applied_adopt_count: count("adopt_count"),
        applied_update_count: count("update_count"),
        unchanged_count: if stored.status == MineralIngestionBatchStatus::Approved {
            count("unchanged_count")
        } else {
            report.summary.unchanged_count
        },
        retired_offer_count: count("retired_offer_count"),
        backup_path: backup.as_ref().map(|value| value.0.clone()),
        backup_sha256: backup.map(|value| value.1),
        decided_at: stored
            .decided_at
            .clone()
            .context("terminal batch has no decision timestamp")?,
    })
}

fn prune_pre_activation_backups(data_root: &Path, retention: usize) -> Result<()> {
    let backups_root = data_root.join("backups");
    let conn = open_connection(data_root, true)?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT batch_id, relative_path
            FROM mineral_ingestion_backups
            WHERE status = 'completed'
            ORDER BY created_at DESC, id DESC
            LIMIT -1 OFFSET ?1
            "#,
        )
        .context("failed to prepare backup retention query")?;
    let old = stmt
        .query_map(params![retention as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    for (batch_id, relative_path) in old {
        let Some(filename) = relative_path.strip_prefix("backups/") else {
            continue;
        };
        if filename.contains('/') || filename.contains('\\') || filename.is_empty() {
            continue;
        }
        let path = backups_root.join(filename);
        match fs::remove_file(&path) {
            Ok(()) => {
                remove_sqlite_backup_files(&path);
                conn.execute(
                    "UPDATE mineral_ingestion_backups SET status = 'pruned' WHERE batch_id = ?1 AND status = 'completed'",
                    params![batch_id],
                )
                .context("failed to record pruned backup")?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                remove_sqlite_backup_files(&path);
                conn.execute(
                    "UPDATE mineral_ingestion_backups SET status = 'pruned' WHERE batch_id = ?1 AND status = 'completed'",
                    params![batch_id],
                )
                .context("failed to record missing retained backup")?;
            }
            Err(_) => {}
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct ExistingBulkMineral {
    id: i64,
    public_id: String,
    slug: String,
    canonical_name: String,
    formula: String,
    nomenclature_status: String,
    is_valid_species: bool,
    record_type: String,
    publication_status: String,
    identifiers_json: String,
    cas_number: Option<String>,
    description: String,
    mineral_family: String,
    synonyms_json: String,
    properties_json: String,
    safety_json: String,
    verification_status: String,
    data_quality_score: f64,
    source_kind: String,
    license_spdx: String,
    image_id: Option<i64>,
    updated_at: String,
    search_text: String,
    target_baseline_hash: String,
}

fn load_quarantined_mineral_items(
    conn: &Connection,
    batch_id: &str,
) -> Result<Vec<MineralIngestionItem>> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT payload_json
            FROM mineral_ingestion_items
            WHERE batch_id = ?1
            ORDER BY chunk_index, item_index
            "#,
        )
        .context("failed to prepare quarantined mineral item query")?;
    let mut rows = stmt.query(params![batch_id])?;
    let mut items = Vec::new();
    while let Some(row) = rows.next()? {
        let payload = row.get::<_, String>(0)?;
        let index = items.len();
        let item: MineralIngestionItem = serde_json::from_str(&payload)
            .with_context(|| format!("quarantined item {} is invalid", index + 1))?;
        validate_mineral_ingestion_item(&item)
            .with_context(|| format!("quarantined item {} failed validation", index + 1))?;
        items.push(item);
    }
    Ok(items)
}

fn build_mineral_ingestion_report(
    conn: &Connection,
    stored: &StoredMineralBatch,
    items: &[MineralIngestionItem],
) -> Result<MineralIngestionReport> {
    let mut existing_by_id = HashMap::new();
    let mut existing_by_slug = HashMap::new();
    let mut existing_by_name: HashMap<String, Vec<i64>> = HashMap::new();
    let mut existing_by_identifier: HashMap<(String, String), Vec<i64>> = HashMap::new();
    let mut material_stmt = conn
        .prepare(
            r#"
            SELECT id, public_id, slug, canonical_name, formula,
                   nomenclature_status, is_valid_species, record_type,
                   publication_status, identifiers_json, cas_number,
                   description, mineral_family, synonyms_json, properties_json,
                   safety_json, verification_status, data_quality_score,
                   source_kind, license_spdx, image_id, updated_at, search_text
            FROM materials
            WHERE record_type = 'mineral'
            "#,
        )
        .context("failed to prepare current mineral identity query")?;
    let material_rows = material_stmt.query_map([], |row| {
        Ok(ExistingBulkMineral {
            id: row.get(0)?,
            public_id: row.get(1)?,
            slug: row.get(2)?,
            canonical_name: row.get(3)?,
            formula: row.get(4)?,
            nomenclature_status: row.get(5)?,
            is_valid_species: row.get::<_, i64>(6)? == 1,
            record_type: row.get(7)?,
            publication_status: row.get(8)?,
            identifiers_json: row.get(9)?,
            cas_number: row.get(10)?,
            description: row.get(11)?,
            mineral_family: row.get(12)?,
            synonyms_json: row.get(13)?,
            properties_json: row.get(14)?,
            safety_json: row.get(15)?,
            verification_status: row.get(16)?,
            data_quality_score: row.get(17)?,
            source_kind: row.get(18)?,
            license_spdx: row.get(19)?,
            image_id: row.get(20)?,
            updated_at: row.get(21)?,
            search_text: row.get(22)?,
            target_baseline_hash: String::new(),
        })
    })?;
    for material in material_rows {
        let mut material = material?;
        material.target_baseline_hash = compute_mineral_target_baseline_hash(&material);
        // Only identity/index fields remain resident for the full diff. The
        // potentially large curator-owned enrichment values are hashed one
        // row at a time and immediately released.
        material.cas_number = None;
        material.description = String::new();
        material.mineral_family = String::new();
        material.synonyms_json = String::new();
        material.properties_json = String::new();
        material.safety_json = String::new();
        material.verification_status = String::new();
        material.source_kind = String::new();
        material.license_spdx = String::new();
        material.image_id = None;
        material.updated_at = String::new();
        material.search_text = String::new();
        if let Ok(Value::Object(identifiers)) = serde_json::from_str(&material.identifiers_json) {
            for key in ["ima_number", "ima_symbol"] {
                if let Some(value) = identifiers.get(key).and_then(Value::as_str) {
                    existing_by_identifier
                        .entry((key.to_string(), normalize_authority_identifier(key, value)))
                        .or_default()
                        .push(material.id);
                }
            }
        }
        existing_by_slug.insert(material.slug.clone(), material.id);
        existing_by_name
            .entry(normalize_identity_text(&material.canonical_name))
            .or_default()
            .push(material.id);
        existing_by_id.insert(material.id, material);
    }
    let mut non_mineral_stmt = conn
        .prepare(
            r#"
            SELECT id, slug, canonical_name, identifiers_json
            FROM materials
            WHERE record_type <> 'mineral'
            "#,
        )
        .context("failed to prepare non-mineral collision query")?;
    let non_minerals = non_mineral_stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    for non_mineral in non_minerals {
        let (id, slug, canonical_name, identifiers_json) = non_mineral?;
        if let Ok(Value::Object(identifiers)) = serde_json::from_str(&identifiers_json) {
            for key in ["ima_number", "ima_symbol"] {
                if let Some(value) = identifiers.get(key).and_then(Value::as_str) {
                    existing_by_identifier
                        .entry((key.to_string(), normalize_authority_identifier(key, value)))
                        .or_default()
                        .push(id);
                }
            }
        }
        existing_by_slug.insert(slug, id);
        existing_by_name
            .entry(normalize_identity_text(&canonical_name))
            .or_default()
            .push(id);
    }

    let mut mapping_by_source = HashMap::new();
    let mut mapped_material_ids = HashSet::new();
    let mut mapping_stmt = conn
        .prepare(
            r#"
            SELECT source_record_id, material_id
            FROM mineral_external_identities
            WHERE dataset_key = ?1
            "#,
        )
        .context("failed to prepare external identity mapping query")?;
    let mappings = mapping_stmt
        .query_map(params![stored.manifest.dataset.key], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(mapping_stmt);
    for (source_record_id, material_id) in mappings {
        mapping_by_source.insert(source_record_id, material_id);
        mapped_material_ids.insert(material_id);
    }

    let owned_identifiers = load_dataset_identifiers(conn, &stored.manifest.dataset.key)?;
    let owned_aliases = load_dataset_aliases(conn, &stored.manifest.dataset.key)?;
    let owned_facts = load_dataset_facts(conn, &stored.manifest.dataset.key)?;
    let association_hashes =
        load_material_target_association_hashes(conn, &stored.manifest.dataset.key)?;
    for material in existing_by_id.values_mut() {
        let association_hash = association_hashes
            .get(&material.id)
            .map(String::as_str)
            .unwrap_or("sha256:no-associated-state");
        material.target_baseline_hash = sha256_bytes(
            format!("{}\u{1f}{association_hash}", material.target_baseline_hash).as_bytes(),
        );
    }
    let source_counts = count_strings(items.iter().map(|item| item.source_record_id.as_str()));
    let slug_counts = count_strings(items.iter().map(|item| item.slug.as_str()));
    let name_counts = count_strings(
        items
            .iter()
            .map(|item| normalize_identity_text(&item.canonical_name)),
    );
    let identifier_counts = count_strings(items.iter().flat_map(|item| {
        item.official_identifiers.iter().map(|(key, value)| {
            format!("{key}\u{1f}{}", normalize_authority_identifier(key, value))
        })
    }));

    let mut summary = MineralIngestionReportSummary::default();
    let mut report_items = Vec::with_capacity(items.len() + mapping_by_source.len());
    if stored.manifest.policy == MineralIngestionPolicy::ImaIdentityV1 {
        if let Some((bound_dataset, bound_source)) =
            mineral_ingestion_authority(conn, stored.manifest.policy)?
        {
            if bound_dataset != stored.manifest.dataset.key
                || bound_source != stored.manifest.source.key
            {
                summary.conflict_count += 1;
                report_items.push(MineralIngestionReportItem {
                    source_record_id: "__authority__".to_string(),
                    proposed_slug: String::new(),
                    resolved_slug: None,
                    material_public_id: None,
                    target_baseline_hash: Some(authority_binding_hash(
                        &bound_dataset,
                        &bound_source,
                    )),
                    classification: MineralIngestionClassification::Conflict,
                    severity: "error".to_string(),
                    code: "authority_binding_conflict".to_string(),
                    message: format!(
                        "policy is bound to dataset/source '{bound_dataset}'/'{bound_source}'"
                    ),
                    critical_formula_change: false,
                    critical_validity_change: false,
                });
            }
        }
    }
    let mut seen_source_ids = HashSet::new();
    for item in items {
        seen_source_ids.insert(item.source_record_id.clone());
        let duplicate_identifier = item.official_identifiers.iter().any(|(key, value)| {
            identifier_counts
                .get(&format!(
                    "{key}\u{1f}{}",
                    normalize_authority_identifier(key, value)
                ))
                .copied()
                .unwrap_or(0)
                > 1
        });
        let duplicate = source_counts
            .get(item.source_record_id.as_str())
            .copied()
            .unwrap_or(0)
            > 1
            || slug_counts.get(item.slug.as_str()).copied().unwrap_or(0) > 1
            || name_counts
                .get(normalize_identity_text(&item.canonical_name).as_str())
                .copied()
                .unwrap_or(0)
                > 1
            || duplicate_identifier;
        if duplicate {
            summary.conflict_count += 1;
            report_items.push(report_item(
                item,
                None,
                None,
                MineralIngestionClassification::Conflict,
                "error",
                "duplicate_batch_identity",
                "duplicate source id, slug, canonical name, or authority identifier in batch",
                false,
                false,
            ));
            continue;
        }

        if stored.manifest.policy == MineralIngestionPolicy::CreateOnlyV1
            && !item.official_identifiers.is_empty()
        {
            summary.conflict_count += 1;
            report_items.push(report_item(
                item,
                None,
                None,
                MineralIngestionClassification::Conflict,
                "error",
                "policy_forbids_authority_identifiers",
                "create_only_v1 cannot assert authority-owned IMA identifiers",
                false,
                false,
            ));
            continue;
        }

        if stored.manifest.policy == MineralIngestionPolicy::CreateOnlyV1
            && !item.official_facts.is_empty()
        {
            summary.conflict_count += 1;
            report_items.push(report_item(
                item,
                None,
                None,
                MineralIngestionClassification::Conflict,
                "error",
                "policy_forbids_authority_facts",
                "create_only_v1 cannot assert authority-owned mineral facts",
                false,
                false,
            ));
            continue;
        }

        if stored.manifest.policy == MineralIngestionPolicy::CreateOnlyV1
            && mapping_by_source.contains_key(&item.source_record_id)
        {
            let material = mapping_by_source
                .get(&item.source_record_id)
                .and_then(|material_id| existing_by_id.get(material_id));
            summary.conflict_count += 1;
            report_items.push(report_item(
                item,
                material,
                material.map(|value| value.slug.clone()),
                MineralIngestionClassification::Conflict,
                "error",
                "create_only_existing_identity",
                "create_only_v1 cannot adopt, update, or replay an existing identity",
                false,
                false,
            ));
            continue;
        }

        if let Some(material_id) = mapping_by_source.get(&item.source_record_id).copied() {
            let Some(material) = existing_by_id.get(&material_id) else {
                summary.conflict_count += 1;
                report_items.push(report_item(
                    item,
                    None,
                    None,
                    MineralIngestionClassification::Conflict,
                    "error",
                    "mapped_material_missing",
                    "stable external identity points to a missing material",
                    false,
                    false,
                ));
                continue;
            };
            let authority_matches = matching_authority_materials(item, &existing_by_identifier);
            if material.record_type != "mineral"
                || material.publication_status != "published"
                || authority_matches.iter().any(|id| *id != material_id)
                || (item.slug != material.slug
                    && existing_by_slug
                        .get(&item.slug)
                        .is_some_and(|id| *id != material_id))
            {
                summary.conflict_count += 1;
                report_items.push(report_item(
                    item,
                    Some(material),
                    Some(material.slug.clone()),
                    MineralIngestionClassification::Conflict,
                    "error",
                    "mapped_identity_collision",
                    "mapped identity collides with another public route or non-mineral record",
                    false,
                    false,
                ));
                continue;
            }
            let expected_identifiers = normalized_identifier_map(&item.official_identifiers);
            let current_identifiers = owned_identifiers
                .get(&material_id)
                .cloned()
                .unwrap_or_default();
            let expected_aliases = normalized_alias_set(&item.synonyms);
            let current_aliases = owned_aliases.get(&material_id).cloned().unwrap_or_default();
            let expected_facts = item.official_facts.as_nonempty_map();
            let current_facts = owned_facts.get(&material_id).cloned().unwrap_or_default();
            let formula_change =
                normalize_formula(&material.formula) != normalize_formula(&item.formula);
            let validity_change = material.is_valid_species != item.is_valid_species
                || item.nomenclature_status == "discredited";
            let changed = normalize_owned_text(&material.canonical_name)
                != normalize_owned_text(&item.canonical_name)
                || formula_change
                || material.nomenclature_status != item.nomenclature_status
                || material.is_valid_species != item.is_valid_species
                || current_identifiers != expected_identifiers
                || current_aliases != expected_aliases
                || current_facts != expected_facts;
            if changed {
                summary.update_count += 1;
                if formula_change || validity_change {
                    summary.identity_critical_warning_count += 1;
                }
                report_items.push(report_item(
                    item,
                    Some(material),
                    Some(material.slug.clone()),
                    MineralIngestionClassification::Update,
                    if formula_change || validity_change {
                        "warning"
                    } else {
                        "info"
                    },
                    if formula_change || validity_change {
                        "identity_critical_update"
                    } else {
                        "owned_fields_changed"
                    },
                    "server-owned identity or nomenclature fields will be refreshed",
                    formula_change,
                    validity_change,
                ));
            } else {
                summary.unchanged_count += 1;
                report_items.push(report_item(
                    item,
                    Some(material),
                    Some(material.slug.clone()),
                    MineralIngestionClassification::Unchanged,
                    "info",
                    "unchanged",
                    "owned fields match the current material",
                    false,
                    false,
                ));
            }
            continue;
        }

        let slug_match = existing_by_slug.get(&item.slug).copied();
        let name_matches = existing_by_name
            .get(&normalize_identity_text(&item.canonical_name))
            .cloned()
            .unwrap_or_default();
        let authority_matches = matching_authority_materials(item, &existing_by_identifier);
        let classification = match stored.manifest.policy {
            MineralIngestionPolicy::CreateOnlyV1 => {
                if slug_match.is_some() || !name_matches.is_empty() || !authority_matches.is_empty()
                {
                    MineralIngestionClassification::Conflict
                } else {
                    MineralIngestionClassification::Create
                }
            }
            MineralIngestionPolicy::ImaIdentityV1 if authority_matches.len() == 1 => {
                let material_id = authority_matches[0];
                let can_adopt = !mapped_material_ids.contains(&material_id)
                    && existing_by_id.get(&material_id).is_some_and(|material| {
                        material.record_type == "mineral"
                            && material.publication_status == "published"
                    })
                    && slug_match.is_none_or(|slug_id| slug_id == material_id);
                if can_adopt {
                    MineralIngestionClassification::Adopt
                } else {
                    MineralIngestionClassification::Conflict
                }
            }
            MineralIngestionPolicy::ImaIdentityV1 if authority_matches.len() > 1 => {
                MineralIngestionClassification::Conflict
            }
            MineralIngestionPolicy::ImaIdentityV1 => match slug_match {
                Some(material_id)
                    if !mapped_material_ids.contains(&material_id)
                        && existing_by_id.get(&material_id).is_some_and(|material| {
                            material.record_type == "mineral"
                                && material.publication_status == "published"
                                && normalize_identity_text(&material.canonical_name)
                                    == normalize_identity_text(&item.canonical_name)
                        }) =>
                {
                    MineralIngestionClassification::Adopt
                }
                Some(_) => MineralIngestionClassification::Conflict,
                None if name_matches.is_empty() => MineralIngestionClassification::Create,
                None => MineralIngestionClassification::Conflict,
            },
        };
        match classification {
            MineralIngestionClassification::Create => {
                summary.create_count += 1;
                report_items.push(report_item(
                    item,
                    None,
                    Some(item.slug.clone()),
                    classification,
                    "info",
                    "new_identity",
                    "a new public mineral identity will be created",
                    false,
                    false,
                ));
            }
            MineralIngestionClassification::Adopt => {
                let material = authority_matches
                    .first()
                    .and_then(|id| existing_by_id.get(id))
                    .or_else(|| slug_match.and_then(|id| existing_by_id.get(&id)));
                let formula_change = material.is_some_and(|value| {
                    normalize_formula(&value.formula) != normalize_formula(&item.formula)
                });
                let validity_change = material.is_some_and(|value| {
                    value.is_valid_species != item.is_valid_species
                        || item.nomenclature_status == "discredited"
                });
                summary.adopt_count += 1;
                if formula_change || validity_change {
                    summary.identity_critical_warning_count += 1;
                }
                report_items.push(report_item(
                    item,
                    material,
                    material.map(|value| value.slug.clone()),
                    classification,
                    "warning",
                    if formula_change || validity_change {
                        "identity_critical_adoption"
                    } else {
                        "adopt_existing_route"
                    },
                    "an exact existing mineral route will receive this stable external mapping",
                    formula_change,
                    validity_change,
                ));
            }
            MineralIngestionClassification::Conflict => {
                summary.conflict_count += 1;
                let material = slug_match.and_then(|id| existing_by_id.get(&id));
                report_items.push(report_item(
                    item,
                    material,
                    material.map(|value| value.slug.clone()),
                    classification,
                    "error",
                    "unmapped_identity_collision",
                    "candidate collides with an existing name, route, or mapped identity",
                    false,
                    false,
                ));
            }
            _ => unreachable!("new mapping classification is exhaustive"),
        }
    }

    if stored.manifest.snapshot_kind == MineralSnapshotKind::Complete {
        for (source_record_id, material_id) in mapping_by_source {
            if seen_source_ids.contains(&source_record_id) {
                continue;
            }
            let material = existing_by_id.get(&material_id);
            summary.missing_count += 1;
            report_items.push(MineralIngestionReportItem {
                source_record_id: source_record_id.clone(),
                proposed_slug: material.map(|value| value.slug.clone()).unwrap_or_default(),
                resolved_slug: material.map(|value| value.slug.clone()),
                material_public_id: material.map(|value| value.public_id.clone()),
                target_baseline_hash: Some(material.map_or_else(
                    || mineral_absence_baseline_hash_values(&source_record_id, "", "", &[]),
                    mineral_target_baseline_hash,
                )),
                classification: MineralIngestionClassification::Missing,
                severity: "warning".to_string(),
                code: "missing_from_complete_snapshot".to_string(),
                message: "existing mapped mineral is absent; no withdrawal will be performed"
                    .to_string(),
                critical_formula_change: false,
                critical_validity_change: false,
            });
        }
    }

    let head = mineral_dataset_head(conn, &stored.manifest.dataset.key)?;
    if head != stored.manifest.base_batch_id {
        summary.conflict_count += 1;
        report_items.push(MineralIngestionReportItem {
            source_record_id: "__batch__".to_string(),
            proposed_slug: String::new(),
            resolved_slug: None,
            material_public_id: None,
            target_baseline_hash: Some(sha256_bytes(format!("batch-head:{head:?}").as_bytes())),
            classification: MineralIngestionClassification::Conflict,
            severity: "error".to_string(),
            code: "stale_base_batch".to_string(),
            message: format!(
                "dataset head changed from {:?} to {:?}",
                stored.manifest.base_batch_id, head
            ),
            critical_formula_change: false,
            critical_validity_change: false,
        });
    }

    report_items.sort_by(|left, right| {
        left.source_record_id
            .cmp(&right.source_record_id)
            .then_with(|| left.proposed_slug.cmp(&right.proposed_slug))
            .then_with(|| left.code.cmp(&right.code))
    });

    Ok(MineralIngestionReport {
        schema_version: MINERAL_INGESTION_SCHEMA_VERSION,
        batch_id: stored.batch_id.clone(),
        manifest_hash: stored.manifest_hash.clone(),
        records_sha256: stored.manifest.records_sha256.clone(),
        base_batch_id: stored.manifest.base_batch_id.clone(),
        generated_at: String::new(),
        summary,
        items: report_items,
    })
}

#[allow(clippy::too_many_arguments)] // Centralizes immutable report row construction.
fn report_item(
    item: &MineralIngestionItem,
    material: Option<&ExistingBulkMineral>,
    resolved_slug: Option<String>,
    classification: MineralIngestionClassification,
    severity: &str,
    code: &str,
    message: &str,
    critical_formula_change: bool,
    critical_validity_change: bool,
) -> MineralIngestionReportItem {
    MineralIngestionReportItem {
        source_record_id: item.source_record_id.clone(),
        proposed_slug: item.slug.clone(),
        resolved_slug,
        material_public_id: material.map(|value| value.public_id.clone()),
        target_baseline_hash: Some(material.map_or_else(
            || mineral_absence_baseline_hash(item),
            mineral_target_baseline_hash,
        )),
        classification,
        severity: severity.to_string(),
        code: code.to_string(),
        message: message.to_string(),
        critical_formula_change,
        critical_validity_change,
    }
}

fn mineral_target_baseline_hash(material: &ExistingBulkMineral) -> String {
    material.target_baseline_hash.clone()
}

fn compute_mineral_target_baseline_hash(material: &ExistingBulkMineral) -> String {
    // Length-prefix every component so the precondition is unambiguous even
    // when source text itself contains separators. CAS and updated_at are
    // included deliberately: a curator edit outside the bulk lane must make a
    // frozen approval stale rather than be silently raced.
    let components = vec![
        material.public_id.clone(),
        material.slug.clone(),
        material.canonical_name.clone(),
        material.formula.clone(),
        material.nomenclature_status.clone(),
        i64::from(material.is_valid_species).to_string(),
        material.record_type.clone(),
        material.publication_status.clone(),
        material.identifiers_json.clone(),
        material.cas_number.clone().unwrap_or_default(),
        material.description.clone(),
        material.mineral_family.clone(),
        material.synonyms_json.clone(),
        material.properties_json.clone(),
        material.safety_json.clone(),
        material.verification_status.clone(),
        material.data_quality_score.to_string(),
        material.source_kind.clone(),
        material.license_spdx.clone(),
        material
            .image_id
            .map_or_else(String::new, |id| id.to_string()),
        material.updated_at.clone(),
        material.search_text.clone(),
    ];
    let mut bytes = Vec::new();
    for value in components {
        bytes.extend_from_slice(value.len().to_string().as_bytes());
        bytes.push(b':');
        bytes.extend_from_slice(value.as_bytes());
        bytes.push(b';');
    }
    sha256_bytes(&bytes)
}

fn mineral_absence_baseline_hash(item: &MineralIngestionItem) -> String {
    let identifiers = item
        .official_identifiers
        .iter()
        .map(|(key, value)| format!("{key}={}", normalize_authority_identifier(key, value)))
        .collect::<Vec<_>>();
    mineral_absence_baseline_hash_values(
        &item.source_record_id,
        &item.slug,
        &normalize_identity_text(&item.canonical_name),
        &identifiers,
    )
}

fn mineral_absence_baseline_hash_values(
    source_record_id: &str,
    slug: &str,
    normalized_name: &str,
    identifiers: &[String],
) -> String {
    let mut bytes = b"absent;".to_vec();
    for value in std::iter::once(source_record_id)
        .chain(std::iter::once(slug))
        .chain(std::iter::once(normalized_name))
        .chain(identifiers.iter().map(String::as_str))
    {
        bytes.extend_from_slice(value.len().to_string().as_bytes());
        bytes.push(b':');
        bytes.extend_from_slice(value.as_bytes());
        bytes.push(b';');
    }
    sha256_bytes(&bytes)
}

fn count_strings<I, S>(values: I) -> HashMap<String, usize>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut counts = HashMap::new();
    for value in values {
        *counts.entry(value.as_ref().to_string()).or_insert(0) += 1;
    }
    counts
}

fn normalized_identifier_map(values: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(key, value)| (key.clone(), normalize_authority_identifier(key, value)))
        .collect()
}

fn normalize_authority_identifier(key: &str, value: &str) -> String {
    match key {
        "ima_symbol" => normalize_owned_text(value),
        _ => normalize_identity_text(value),
    }
}

fn matching_authority_materials(
    item: &MineralIngestionItem,
    existing: &HashMap<(String, String), Vec<i64>>,
) -> Vec<i64> {
    let mut matches = BTreeSet::new();
    for (key, value) in &item.official_identifiers {
        if let Some(material_ids) =
            existing.get(&(key.clone(), normalize_authority_identifier(key, value)))
        {
            matches.extend(material_ids.iter().copied());
        }
    }
    matches.into_iter().collect()
}

fn normalized_alias_set(values: &[String]) -> BTreeSet<String> {
    values
        .iter()
        .map(|value| normalize_identity_text(value))
        .collect()
}

fn load_dataset_identifiers(
    conn: &Connection,
    dataset_key: &str,
) -> Result<HashMap<i64, BTreeMap<String, String>>> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT material_id, identifier_key, normalized_value
            FROM mineral_dataset_identifiers
            WHERE dataset_key = ?1
            "#,
        )
        .context("failed to prepare dataset identifier query")?;
    let rows = stmt
        .query_map(params![dataset_key], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut values: HashMap<i64, BTreeMap<String, String>> = HashMap::new();
    for (material_id, key, value) in rows {
        values.entry(material_id).or_default().insert(key, value);
    }
    Ok(values)
}

fn load_dataset_aliases(
    conn: &Connection,
    dataset_key: &str,
) -> Result<HashMap<i64, BTreeSet<String>>> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT material_id, alias_normalized
            FROM material_aliases
            WHERE origin = 'bulk_dataset' AND dataset_key = ?1
            "#,
        )
        .context("failed to prepare dataset alias query")?;
    let rows = stmt
        .query_map(params![dataset_key], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut values: HashMap<i64, BTreeSet<String>> = HashMap::new();
    for (material_id, value) in rows {
        values.entry(material_id).or_default().insert(value);
    }
    Ok(values)
}

fn load_dataset_facts(
    conn: &Connection,
    dataset_key: &str,
) -> Result<HashMap<i64, BTreeMap<String, String>>> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT material_id, fact_key, fact_value
            FROM mineral_dataset_facts
            WHERE dataset_key = ?1
            "#,
        )
        .context("failed to prepare dataset mineral facts query")?;
    let rows = stmt
        .query_map(params![dataset_key], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut values: HashMap<i64, BTreeMap<String, String>> = HashMap::new();
    for (material_id, key, value) in rows {
        values.entry(material_id).or_default().insert(key, value);
    }
    Ok(values)
}

fn load_material_target_association_hashes(
    conn: &Connection,
    dataset_key: &str,
) -> Result<HashMap<i64, String>> {
    // These are the side tables activation can replace or otherwise mutate.
    // Rows are deterministically ordered and hashed as a stream so the full
    // association payload never has to be resident in memory.
    let mut stmt = conn
        .prepare(
            r#"
            SELECT material_id, state_kind, state_json
            FROM (
                SELECT material_id, 'identifier' AS state_kind,
                       json_array(identifier_key, identifier_value,
                                  normalized_value, source_release_id) AS state_json
                FROM mineral_dataset_identifiers
                WHERE dataset_key = ?1
                UNION ALL
                SELECT material_id, 'official_fact' AS state_kind,
                       json_array(fact_key, fact_value, source_release_id) AS state_json
                FROM mineral_dataset_facts
                WHERE dataset_key = ?1
                UNION ALL
                SELECT material_id, 'alias' AS state_kind,
                       json_array(id, alias, alias_normalized, language_code,
                                  alias_type, origin, dataset_key, source_release_id) AS state_json
                FROM material_aliases
                UNION ALL
                SELECT material_id, 'evidence' AS state_kind,
                       json_array(id, source_id, claim_scope, claim_json,
                                  confidence, review_status, source_title,
                                  source_publisher, source_license_spdx,
                                  source_retrieved_at, source_content_hash,
                                  dataset_key, source_release_id) AS state_json
                FROM material_evidence
                WHERE dataset_key = ?1
                UNION ALL
                SELECT material_id, 'active_offer' AS state_kind,
                       json_array(id, provider_id, external_id, active, updated_at) AS state_json
                FROM offers
                WHERE active = 1
            )
            ORDER BY material_id, state_kind, state_json
            "#,
        )
        .context("failed to prepare activation side-state baseline")?;
    let mut rows = stmt.query(params![dataset_key])?;
    let mut hashes = HashMap::new();
    let mut current_material_id = None;
    let mut context = DigestContext::new(&SHA256);
    while let Some(row) = rows.next()? {
        let material_id = row.get::<_, i64>(0)?;
        if current_material_id.is_some_and(|current| current != material_id) {
            let completed = std::mem::replace(&mut context, DigestContext::new(&SHA256));
            hashes.insert(
                current_material_id.expect("material id is set"),
                format_sha256_digest(completed.finish().as_ref()),
            );
        }
        current_material_id = Some(material_id);
        for value in [row.get::<_, String>(1)?, row.get::<_, String>(2)?] {
            context.update(value.len().to_string().as_bytes());
            context.update(b":");
            context.update(value.as_bytes());
            context.update(b";");
        }
    }
    if let Some(material_id) = current_material_id {
        hashes.insert(material_id, format_sha256_digest(context.finish().as_ref()));
    }
    Ok(hashes)
}

#[derive(Debug)]
struct StoredMineralBatch {
    batch_id: String,
    manifest_hash: String,
    manifest: MineralDatasetManifest,
    status: MineralIngestionBatchStatus,
    report_hash: Option<String>,
    report_json: Option<String>,
    decision_actor: Option<String>,
    decision_note: String,
    created_at: String,
    finalized_at: Option<String>,
    decided_at: Option<String>,
}

fn load_stored_batch(conn: &Connection, batch_id: &str) -> Result<Option<StoredMineralBatch>> {
    let row = conn
        .query_row(
            r#"
            SELECT batch_id, manifest_hash, manifest_json, status, report_hash,
                   report_json, decision_actor, decision_note, created_at,
                   finalized_at, decided_at
            FROM mineral_ingestion_batches
            WHERE batch_id = ?1
            "#,
            params![batch_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, Option<String>>(10)?,
                ))
            },
        )
        .optional()
        .context("failed to load mineral ingestion batch")?;
    row.map(
        |(
            batch_id,
            manifest_hash,
            manifest_json,
            status,
            report_hash,
            report_json,
            decision_actor,
            decision_note,
            created_at,
            finalized_at,
            decided_at,
        )| {
            let manifest: MineralDatasetManifest = serde_json::from_str(&manifest_json)
                .with_context(|| format!("batch '{batch_id}' contains an invalid manifest"))?;
            let recalculated = canonical_mineral_manifest_hash(&manifest)?;
            if recalculated != manifest_hash {
                bail!("batch '{batch_id}' manifest content address is corrupt");
            }
            Ok(StoredMineralBatch {
                batch_id,
                manifest_hash,
                manifest,
                status: MineralIngestionBatchStatus::from_database(&status)?,
                report_hash,
                report_json,
                decision_actor,
                decision_note,
                created_at,
                finalized_at,
                decided_at,
            })
        },
    )
    .transpose()
}

fn load_mineral_ingestion_batch(
    conn: &Connection,
    batch_id: &str,
) -> Result<Option<MineralIngestionBatchDetail>> {
    let Some(stored) = load_stored_batch(conn, batch_id)? else {
        return Ok(None);
    };
    let (live_chunk_count, live_record_count): (i64, i64) = conn
        .query_row(
            r#"
            SELECT COUNT(*), COALESCE(SUM(item_count), 0)
            FROM mineral_ingestion_chunks
            WHERE batch_id = ?1
            "#,
            params![batch_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .context("failed to count received mineral chunks")?;
    let (compacted_chunk_count, compacted_record_count): (i64, i64) = conn
        .query_row(
            r#"
            SELECT compacted_chunk_count, compacted_record_count
            FROM mineral_ingestion_batches WHERE batch_id = ?1
            "#,
            params![batch_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .context("failed to load compacted mineral payload counts")?;
    let chunk_count = live_chunk_count
        .checked_add(compacted_chunk_count)
        .context("mineral chunk count overflow")?;
    let record_count = live_record_count
        .checked_add(compacted_record_count)
        .context("mineral record count overflow")?;
    let report_summary = stored
        .report_json
        .as_deref()
        .map(|raw| {
            serde_json::from_str::<MineralIngestionReport>(raw)
                .context("stored mineral ingestion report is invalid")
                .map(|report| report.summary)
        })
        .transpose()?;
    let review_samples = load_mineral_review_samples(
        conn,
        batch_id,
        usize::try_from(live_record_count).context("invalid live record count")?,
    )?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT ri.source_record_id, ri.proposed_slug, ri.resolved_slug,
                   m.public_id, ri.target_baseline_hash,
                   ri.classification, ri.severity, ri.code,
                   ri.message, ri.critical_formula_change,
                   ri.critical_validity_change
            FROM mineral_ingestion_report_items ri
            LEFT JOIN materials m ON m.id = ri.material_id
            WHERE ri.batch_id = ?1
              AND (ri.severity <> 'info'
                   OR ri.critical_formula_change = 1
                   OR ri.critical_validity_change = 1)
            ORDER BY
                CASE ri.severity WHEN 'error' THEN 0 WHEN 'warning' THEN 1 ELSE 2 END,
                ri.id
            LIMIT 50
            "#,
        )
        .context("failed to prepare ingestion anomaly samples")?;
    let anomaly_rows = stmt
        .query_map(params![batch_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, i64>(10)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    let anomaly_samples = anomaly_rows
        .into_iter()
        .map(
            |(
                source_record_id,
                proposed_slug,
                resolved_slug,
                material_public_id,
                target_baseline_hash,
                classification,
                severity,
                code,
                message,
                critical_formula_change,
                critical_validity_change,
            )| {
                Ok(MineralIngestionReportItem {
                    source_record_id,
                    proposed_slug,
                    resolved_slug,
                    material_public_id,
                    target_baseline_hash,
                    classification: MineralIngestionClassification::from_database(&classification)?,
                    severity,
                    code,
                    message,
                    critical_formula_change: critical_formula_change == 1,
                    critical_validity_change: critical_validity_change == 1,
                })
            },
        )
        .collect::<Result<Vec<_>>>()?;
    let backup = conn
        .query_row(
            r#"
            SELECT relative_path, sha256
            FROM mineral_ingestion_backups
            WHERE batch_id = ?1 AND status = 'completed'
            "#,
            params![batch_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .context("failed to load pre-activation backup")?;

    Ok(Some(MineralIngestionBatchDetail {
        batch_id: stored.batch_id,
        status: stored.status,
        manifest_hash: stored.manifest_hash,
        report_hash: stored.report_hash,
        manifest: stored.manifest,
        received_chunk_count: usize::try_from(chunk_count).context("invalid chunk count")?,
        received_record_count: usize::try_from(record_count).context("invalid record count")?,
        report_summary,
        review_samples,
        anomaly_samples,
        created_at: stored.created_at,
        finalized_at: stored.finalized_at,
        decided_at: stored.decided_at,
        decision_actor: stored.decision_actor,
        decision_note: stored.decision_note,
        backup_path: backup.as_ref().map(|row| row.0.clone()),
        backup_sha256: backup.map(|row| row.1),
    }))
}

fn load_mineral_review_samples(
    conn: &Connection,
    batch_id: &str,
    record_count: usize,
) -> Result<Vec<MineralIngestionItem>> {
    const SAMPLE_LIMIT: usize = 25;
    if record_count == 0 {
        return Ok(Vec::new());
    }
    let sample_count = record_count.min(SAMPLE_LIMIT);
    let offsets = if sample_count == 1 {
        vec![0]
    } else {
        (0..sample_count)
            .map(|index| index * (record_count - 1) / (sample_count - 1))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    };
    offsets
        .into_iter()
        .map(|offset| {
            let payload: String = conn
                .query_row(
                    r#"
                    SELECT payload_json FROM mineral_ingestion_items
                    WHERE batch_id = ?1
                    ORDER BY chunk_index, item_index
                    LIMIT 1 OFFSET ?2
                    "#,
                    params![batch_id, offset as i64],
                    |row| row.get(0),
                )
                .with_context(|| format!("failed to load review sample at offset {offset}"))?;
            serde_json::from_str(&payload).context("stored review sample is invalid")
        })
        .collect()
}

fn mineral_chunk_receipt(
    conn: &Connection,
    batch_id: &str,
    chunk: &MineralIngestionChunk,
    content_hash: &str,
    stored: bool,
) -> Result<MineralChunkReceipt> {
    let (chunk_count, record_count): (i64, i64) = conn
        .query_row(
            r#"
            SELECT COUNT(*), COALESCE(SUM(item_count), 0)
            FROM mineral_ingestion_chunks
            WHERE batch_id = ?1
            "#,
            params![batch_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .context("failed to count received chunks")?;
    Ok(MineralChunkReceipt {
        batch_id: batch_id.to_string(),
        chunk_index: chunk.chunk_index,
        content_hash: content_hash.to_string(),
        item_count: chunk.items.len(),
        stored,
        received_chunk_count: usize::try_from(chunk_count).context("invalid chunk count")?,
        received_record_count: usize::try_from(record_count).context("invalid record count")?,
    })
}

#[allow(clippy::too_many_arguments)] // Mirrors the append-only event schema explicitly.
fn append_mineral_ingestion_event(
    conn: &Connection,
    batch_id: &str,
    event_type: &str,
    actor: &str,
    policy: MineralIngestionPolicy,
    manifest_hash: &str,
    report_hash: Option<&str>,
    detail: &Value,
) -> Result<()> {
    let detail_json = String::from_utf8(canonical_json_bytes(detail)?)
        .context("canonical ingestion event is not UTF-8")?;
    conn.execute(
        r#"
        INSERT INTO mineral_ingestion_events(
            batch_id, event_type, actor, policy_version, manifest_hash,
            report_hash, detail_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
        params![
            batch_id,
            event_type,
            actor.trim(),
            policy.as_str(),
            manifest_hash,
            report_hash,
            detail_json,
        ],
    )
    .context("failed to append mineral ingestion event")?;
    Ok(())
}

fn mineral_dataset_head(conn: &Connection, dataset_key: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT batch_id FROM mineral_dataset_heads WHERE dataset_key = ?1",
        params![dataset_key],
        |row| row.get(0),
    )
    .optional()
    .context("failed to load mineral dataset head")
}

fn mineral_ingestion_authority(
    conn: &Connection,
    policy: MineralIngestionPolicy,
) -> Result<Option<(String, String)>> {
    conn.query_row(
        r#"
        SELECT dataset_key, source_key
        FROM mineral_ingestion_authorities
        WHERE policy = ?1
        "#,
        params![policy.as_str()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
    .context("failed to load mineral ingestion authority binding")
}

fn authority_binding_hash(dataset_key: &str, source_key: &str) -> String {
    sha256_bytes(format!("authority:{dataset_key}\u{1f}{source_key}").as_bytes())
}

fn verify_mineral_ingestion_authority(
    conn: &Connection,
    stored: &StoredMineralBatch,
) -> Result<()> {
    if stored.manifest.policy != MineralIngestionPolicy::ImaIdentityV1 {
        return Ok(());
    }
    if let Some((dataset_key, source_key)) =
        mineral_ingestion_authority(conn, stored.manifest.policy)?
    {
        if dataset_key != stored.manifest.dataset.key || source_key != stored.manifest.source.key {
            return Err(mineral_ingestion_problem(
                MineralIngestionProblemKind::Conflict,
                "authority_binding_conflict",
                format!(
                    "ima_identity_v1 is bound to dataset/source '{dataset_key}'/'{source_key}'"
                ),
            ));
        }
    }
    Ok(())
}

fn bind_mineral_ingestion_authority(conn: &Connection, stored: &StoredMineralBatch) -> Result<()> {
    if stored.manifest.policy != MineralIngestionPolicy::ImaIdentityV1 {
        return Ok(());
    }
    conn.execute(
        r#"
        INSERT INTO mineral_ingestion_authorities(
            policy, dataset_key, source_key, bound_batch_id
        ) VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT(policy) DO NOTHING
        "#,
        params![
            stored.manifest.policy.as_str(),
            stored.manifest.dataset.key,
            stored.manifest.source.key,
            stored.batch_id,
        ],
    )
    .context("failed to bind mineral ingestion authority")?;
    verify_mineral_ingestion_authority(conn, stored)
}

/// Lists the exact mineral revisions awaiting an operator decision.
pub fn list_pending_mineral_reviews(
    data_root: &Path,
    limit: usize,
    offset: usize,
) -> Result<PendingMineralReviewPage> {
    let mut conn = open_connection(data_root, false)?;
    let limit = limit.clamp(1, MAX_SEARCH_RESULTS);
    let sql_limit = i64::try_from(limit).context("mineral review limit is too large")?;
    let sql_offset = i64::try_from(offset).context("mineral review offset is too large")?;
    let tx = conn
        .transaction()
        .context("failed to start mineral review queue transaction")?;
    let total_count: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM mineral_review_revisions WHERE status = 'pending'",
            [],
            |row| row.get(0),
        )
        .context("failed to count pending mineral reviews")?;
    let mut stmt = tx
        .prepare(
            r#"
            SELECT id, revision, source_label, submitted_at, material_slug, payload_json
            FROM mineral_review_revisions
            WHERE status = 'pending'
            ORDER BY submitted_at ASC, id ASC
            LIMIT ?1 OFFSET ?2
            "#,
        )
        .context("failed to prepare pending mineral review query")?;
    let rows = stmt
        .query_map(params![sql_limit, sql_offset], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    let items = rows
        .into_iter()
        .map(
            |(review_id, revision, source_label, submitted_at, stored_slug, payload_json)| {
                let record: MaterialImport = serde_json::from_str(&payload_json)
                    .with_context(|| format!("review {review_id} contains an invalid payload"))?;
                if record.record_type != "mineral" || record.slug != stored_slug {
                    bail!("review {review_id} contains inconsistent mineral identity");
                }
                Ok(PendingMineralReview {
                    review_id,
                    revision: usize::try_from(revision)
                        .context("invalid mineral review revision")?,
                    source_label,
                    submitted_at,
                    record,
                })
            },
        )
        .collect::<Result<Vec<_>>>()?;
    tx.commit()
        .context("failed to finish mineral review queue transaction")?;

    Ok(PendingMineralReviewPage {
        items,
        total_count: usize::try_from(total_count).context("invalid mineral review count")?,
        limit,
        offset,
    })
}

/// Publishes exactly the staged revision identified by `review_id`.
/// Repeating an already successful approval is an idempotent no-op.
pub fn approve_mineral_review(
    data_root: &Path,
    review_id: i64,
    operator_note: &str,
) -> Result<MineralReviewOutcome> {
    decide_mineral_review(
        data_root,
        review_id,
        MineralReviewStatus::Approved,
        operator_note,
    )
}

/// Rejects exactly the staged revision identified by `review_id` without
/// changing any currently published version of that mineral.
/// Repeating an already successful rejection is an idempotent no-op.
pub fn reject_mineral_review(
    data_root: &Path,
    review_id: i64,
    operator_note: &str,
) -> Result<MineralReviewOutcome> {
    decide_mineral_review(
        data_root,
        review_id,
        MineralReviewStatus::Rejected,
        operator_note,
    )
}

/// Withdraws a public mineral and every commercial listing tied to that
/// published identity. The operator note is retained with the live row.
/// Returns true when the public state changed and false when it was already
/// withdrawn. Pending revisions are superseded in either case.
pub fn withdraw_mineral(data_root: &Path, slug: &str, operator_note: &str) -> Result<bool> {
    if !is_valid_registry_slug(slug) {
        bail!("invalid mineral slug '{slug}'");
    }
    validate_text("operator_note", operator_note, 1, 2_000)?;
    let operator_note = operator_note.trim();
    let mut conn = open_connection(data_root, true)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .context("failed to start mineral withdrawal transaction")?;
    let material = tx
        .query_row(
            "SELECT id, record_type, publication_status FROM materials WHERE slug = ?1",
            params![slug],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .context("failed to load mineral for withdrawal")?
        .with_context(|| format!("mineral '{slug}' does not exist"))?;
    if material.1 != "mineral" {
        bail!("record '{slug}' is not a mineral");
    }
    if !matches!(material.2.as_str(), "published" | "withdrawn") {
        bail!("mineral '{slug}' has an unsupported publication state");
    }

    let changed = material.2 == "published";
    if changed {
        let updated = tx
            .execute(
                r#"
                UPDATE materials
                SET publication_status = 'withdrawn', withdrawal_note = ?1,
                    withdrawn_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP
                WHERE id = ?2 AND publication_status = 'published'
                "#,
                params![operator_note, material.0],
            )
            .context("failed to withdraw mineral")?;
        if updated != 1 {
            bail!("mineral '{slug}' changed during withdrawal");
        }
    }

    tx.execute(
        "UPDATE offers SET active = 0, updated_at = CURRENT_TIMESTAMP WHERE material_id = ?1 AND active = 1",
        params![material.0],
    )
    .context("failed to retire withdrawn mineral offers")?;
    tx.execute(
        r#"
        UPDATE mineral_review_revisions
        SET status = 'superseded', operator_note = ?1, reviewed_at = CURRENT_TIMESTAMP
        WHERE material_slug = ?2 AND status = 'pending'
        "#,
        params![operator_note, slug],
    )
    .context("failed to supersede pending revisions for withdrawn mineral")?;
    tx.commit().context("failed to commit mineral withdrawal")?;
    Ok(changed)
}

pub fn import_provider(
    data_root: &Path,
    provider: &ProviderImport,
) -> Result<ProviderImportSummary> {
    validate_provider_import(provider)?;
    let mut conn = open_connection(data_root, true)?;
    let tx = conn
        .transaction()
        .context("failed to start provider import transaction")?;

    tx.execute(
        r#"
        INSERT INTO providers(
            slug, name, website_url, network_kind, country_code,
            verification_status, trust_score, active
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ON CONFLICT(slug) DO UPDATE SET
            name = excluded.name,
            website_url = excluded.website_url,
            network_kind = excluded.network_kind,
            country_code = excluded.country_code,
            verification_status = excluded.verification_status,
            trust_score = excluded.trust_score,
            active = excluded.active,
            updated_at = CURRENT_TIMESTAMP
        "#,
        params![
            provider.slug,
            provider.name.trim(),
            provider.website_url.trim(),
            provider.network_kind.trim(),
            provider.country_code.trim().to_ascii_uppercase(),
            provider.verification_status,
            provider.trust_score,
            i64::from(provider.verification_status != "suspended"),
        ],
    )
    .context("failed to upsert provider")?;
    let provider_id: i64 = tx.query_row(
        "SELECT id FROM providers WHERE slug = ?1",
        params![provider.slug],
        |row| row.get(0),
    )?;
    // A provider import is a complete current-offer snapshot. Any listing not
    // observed in this run is retired before submitted rows are reactivated.
    tx.execute(
        "UPDATE offers SET active = 0, updated_at = CURRENT_TIMESTAMP WHERE provider_id = ?1",
        params![provider_id],
    )
    .context("failed to retire prior provider offers")?;

    for offer in &provider.offers {
        let material_id: i64 = tx
            .query_row(
                "SELECT id FROM materials WHERE slug = ?1 AND publication_status = 'published' AND (record_type = 'compound' OR (record_type = 'mineral' AND is_valid_species = 1))",
                params![offer.material_slug],
                |row| row.get(0),
            )
            .with_context(|| {
                format!(
                    "offer '{}' references unknown material '{}'",
                    offer.external_id, offer.material_slug
                )
            })?;

        let evidence_source_id = match offer.evidence_url.as_deref() {
            Some(url) if !url.trim().is_empty() => {
                let canonical_url = canonicalize_evidence_url(url)?;
                tx.execute(
                    r#"
                    INSERT INTO evidence_sources(
                        canonical_url, title, publisher, license_spdx, metadata_json
                    ) VALUES (?1, ?2, ?3, 'NOASSERTION', ?4)
                    ON CONFLICT(canonical_url) DO NOTHING
                    "#,
                    params![
                        canonical_url,
                        offer.title.trim(),
                        provider.name.trim(),
                        json!({"kind": "provider_offer"}).to_string(),
                    ],
                )?;
                Some(tx.query_row(
                    "SELECT id FROM evidence_sources WHERE canonical_url = ?1",
                    params![canonical_url],
                    |row| row.get::<_, i64>(0),
                )?)
            }
            _ => None,
        };

        let last_checked_at = if offer.last_checked_at.trim().is_empty() {
            None
        } else {
            Some(normalize_timestamp(
                "offer.last_checked_at",
                &offer.last_checked_at,
            )?)
        };
        let expires_at = offer
            .expires_at
            .as_deref()
            .map(|value| normalize_timestamp("offer.expires_at", value))
            .transpose()?;
        tx.execute(
            r#"
            INSERT INTO offers(
                material_id, provider_id, external_id, title, product_url,
                currency_code, price_minor, currency_exponent, pricing_basis,
                minimum_order_quantity, minimum_order_unit, available_quantity,
                available_quantity_unit, stock_status, purity_text, grade,
                origin_country_code, provider_claims_json, evidence_source_id,
                verification_status, last_checked_at, expires_at, active
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                ?14, ?15, ?16, ?17, ?18, ?19, ?20,
                COALESCE(?21, CURRENT_TIMESTAMP), ?22, ?23
            )
            ON CONFLICT(provider_id, external_id) DO UPDATE SET
                material_id = excluded.material_id,
                title = excluded.title,
                product_url = excluded.product_url,
                currency_code = excluded.currency_code,
                price_minor = excluded.price_minor,
                currency_exponent = excluded.currency_exponent,
                pricing_basis = excluded.pricing_basis,
                minimum_order_quantity = excluded.minimum_order_quantity,
                minimum_order_unit = excluded.minimum_order_unit,
                available_quantity = excluded.available_quantity,
                available_quantity_unit = excluded.available_quantity_unit,
                stock_status = excluded.stock_status,
                purity_text = excluded.purity_text,
                grade = excluded.grade,
                origin_country_code = excluded.origin_country_code,
                provider_claims_json = excluded.provider_claims_json,
                evidence_source_id = excluded.evidence_source_id,
                verification_status = excluded.verification_status,
                last_checked_at = excluded.last_checked_at,
                expires_at = excluded.expires_at,
                active = excluded.active,
                updated_at = CURRENT_TIMESTAMP
            "#,
            params![
                material_id,
                provider_id,
                offer.external_id.trim(),
                offer.title.trim(),
                offer.product_url.trim(),
                offer.currency_code.trim().to_ascii_uppercase(),
                offer.price_minor,
                offer.currency_exponent,
                offer.pricing_basis.trim(),
                offer.minimum_order_quantity,
                offer.minimum_order_unit.trim(),
                offer.available_quantity,
                offer.available_quantity_unit.trim(),
                offer.stock_status,
                offer.purity_text.trim(),
                offer.grade.trim(),
                offer.origin_country_code.trim().to_ascii_uppercase(),
                serde_json::to_string(&offer.provider_claims)?,
                evidence_source_id,
                offer.verification_status,
                last_checked_at.as_deref(),
                expires_at.as_deref(),
                i64::from(offer.active),
            ],
        )
        .with_context(|| format!("failed to upsert offer '{}'", offer.external_id))?;
    }

    tx.commit()
        .context("failed to commit provider import transaction")?;
    Ok(ProviderImportSummary {
        provider_slug: provider.slug.clone(),
        offers_upserted: provider.offers.len(),
    })
}

fn stage_mineral_review(
    tx: &Transaction<'_>,
    ingestion_run_id: i64,
    source_label: &str,
    record: &MaterialImport,
) -> Result<i64> {
    if record.record_type != "mineral" {
        bail!("only mineral records can enter the mineral review queue");
    }
    let next_revision: i64 = tx
        .query_row(
            r#"
            SELECT COALESCE(MAX(revision), 0) + 1
            FROM mineral_review_revisions
            WHERE material_slug = ?1
            "#,
            params![record.slug],
            |row| row.get(0),
        )
        .context("failed to allocate mineral review revision")?;

    tx.execute(
        r#"
        UPDATE mineral_review_revisions
        SET status = 'superseded',
            operator_note = ?1,
            reviewed_at = CURRENT_TIMESTAMP
        WHERE material_slug = ?2 AND status = 'pending'
        "#,
        params![
            format!("Superseded by imported revision {next_revision}."),
            record.slug
        ],
    )
    .context("failed to supersede the prior pending mineral revision")?;

    let payload_json = serde_json::to_string(record)?;
    tx.execute(
        r#"
        INSERT INTO mineral_review_revisions(
            material_slug, revision, ingestion_run_id, source_label, payload_json
        ) VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![
            record.slug,
            next_revision,
            ingestion_run_id,
            source_label,
            payload_json
        ],
    )
    .with_context(|| format!("failed to stage mineral '{}' for review", record.slug))?;
    Ok(tx.last_insert_rowid())
}

struct StoredMineralReview {
    review_id: i64,
    revision: i64,
    mineral_slug: String,
    payload_json: String,
    status: MineralReviewStatus,
    operator_note: String,
    submitted_at: String,
    reviewed_at: Option<String>,
}

fn load_stored_mineral_review(
    tx: &Transaction<'_>,
    review_id: i64,
) -> Result<Option<StoredMineralReview>> {
    let row = tx
        .query_row(
            r#"
            SELECT
                id, revision, material_slug, payload_json, status,
                operator_note, submitted_at, reviewed_at
            FROM mineral_review_revisions
            WHERE id = ?1
            "#,
            params![review_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            },
        )
        .optional()
        .context("failed to load mineral review revision")?;
    row.map(
        |(
            review_id,
            revision,
            mineral_slug,
            payload_json,
            status,
            operator_note,
            submitted_at,
            reviewed_at,
        )| {
            Ok(StoredMineralReview {
                review_id,
                revision,
                mineral_slug,
                payload_json,
                status: MineralReviewStatus::from_database(&status)?,
                operator_note,
                submitted_at,
                reviewed_at,
            })
        },
    )
    .transpose()
}

fn decide_mineral_review(
    data_root: &Path,
    review_id: i64,
    decision: MineralReviewStatus,
    operator_note: &str,
) -> Result<MineralReviewOutcome> {
    if review_id <= 0 {
        bail!("mineral review id must be positive");
    }
    if !matches!(
        decision,
        MineralReviewStatus::Approved | MineralReviewStatus::Rejected
    ) {
        bail!("mineral review decision must be approved or rejected");
    }
    validate_text("operator_note", operator_note, 1, 2_000)?;
    let operator_note = operator_note.trim();

    let mut conn = open_connection(data_root, true)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .context("failed to start mineral review decision transaction")?;
    let stored = load_stored_mineral_review(&tx, review_id)?
        .with_context(|| format!("mineral review {review_id} does not exist"))?;

    if stored.status == decision {
        let outcome = stored.into_outcome(false)?;
        tx.commit()
            .context("failed to finish idempotent mineral review decision")?;
        return Ok(outcome);
    }
    if stored.status != MineralReviewStatus::Pending {
        bail!(
            "mineral review {review_id} is already {} and cannot be {}",
            stored.status.as_str(),
            decision.as_str()
        );
    }

    if decision == MineralReviewStatus::Approved {
        let record: MaterialImport = serde_json::from_str(&stored.payload_json)
            .with_context(|| format!("review {review_id} contains an invalid payload"))?;
        if record.record_type != "mineral" || record.slug != stored.mineral_slug {
            bail!("review {review_id} contains inconsistent mineral identity");
        }
        validate_import(&record)
            .with_context(|| format!("review {review_id} contains an invalid mineral payload"))?;
        // Applying the payload and recording approval share one IMMEDIATE
        // transaction: either both the public record and its evidence snapshot
        // become visible, or neither does.
        upsert_material(&tx, &record)?;
    }

    let changed = tx
        .execute(
            r#"
            UPDATE mineral_review_revisions
            SET status = ?1, operator_note = ?2, reviewed_at = CURRENT_TIMESTAMP
            WHERE id = ?3 AND status = 'pending'
            "#,
            params![decision.as_str(), operator_note, review_id],
        )
        .context("failed to record mineral review decision")?;
    if changed != 1 {
        bail!("mineral review {review_id} changed during the decision");
    }

    let outcome = load_stored_mineral_review(&tx, review_id)?
        .context("mineral review disappeared after its decision")?
        .into_outcome(true)?;
    tx.commit()
        .context("failed to commit mineral review decision")?;
    Ok(outcome)
}

impl StoredMineralReview {
    fn into_outcome(self, changed: bool) -> Result<MineralReviewOutcome> {
        Ok(MineralReviewOutcome {
            review_id: self.review_id,
            revision: usize::try_from(self.revision).context("invalid mineral review revision")?,
            mineral_slug: self.mineral_slug,
            status: self.status,
            operator_note: self.operator_note,
            submitted_at: self.submitted_at,
            reviewed_at: self.reviewed_at,
            changed,
        })
    }
}

fn upsert_material(tx: &Transaction<'_>, record: &MaterialImport) -> Result<()> {
    let identifiers_json = serde_json::to_string(&record.identifiers)?;
    let synonyms_json = serde_json::to_string(&record.synonyms)?;
    let properties_json = serde_json::to_string(&record.properties)?;
    let safety_json = serde_json::to_string(&record.safety)?;
    let search_text = build_search_text(record);

    tx.execute(
        r#"
        INSERT INTO materials (
            public_id, slug, record_type, canonical_name, formula, description, mineral_family,
            cas_number, identifiers_json, synonyms_json, properties_json, safety_json,
            search_text, verification_status, data_quality_score, source_kind, license_spdx,
            publication_status
        ) VALUES (
            'mat_' || lower(hex(randomblob(16))),
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
            'registry_import', ?15, 'published'
        )
        ON CONFLICT(slug) DO UPDATE SET
            record_type = excluded.record_type,
            canonical_name = excluded.canonical_name,
            formula = excluded.formula,
            description = excluded.description,
            mineral_family = excluded.mineral_family,
            cas_number = excluded.cas_number,
            identifiers_json = excluded.identifiers_json,
            synonyms_json = excluded.synonyms_json,
            properties_json = excluded.properties_json,
            safety_json = excluded.safety_json,
            search_text = excluded.search_text,
            verification_status = excluded.verification_status,
            data_quality_score = excluded.data_quality_score,
            source_kind = excluded.source_kind,
            license_spdx = excluded.license_spdx,
            publication_status = 'published',
            withdrawal_note = '',
            withdrawn_at = NULL,
            image_id = NULL,
            updated_at = CURRENT_TIMESTAMP
        "#,
        params![
            record.slug,
            record.record_type,
            record.canonical_name.trim(),
            record.formula.trim(),
            record.description.trim(),
            record.mineral_family.trim(),
            record.cas_number.as_deref(),
            identifiers_json,
            synonyms_json,
            properties_json,
            safety_json,
            search_text,
            record.verification_status,
            record.data_quality_score,
            record.license_spdx.trim(),
        ],
    )
    .with_context(|| format!("failed to upsert material '{}'", record.slug))?;

    let material_id: i64 = tx
        .query_row(
            "SELECT id FROM materials WHERE slug = ?1",
            params![record.slug],
            |row| row.get(0),
        )
        .context("failed to resolve imported material id")?;

    // Offers describe the previously reviewed identity/version. Retire them
    // whenever a new mineral revision is published so a slug cannot carry
    // stale commercial listings into materially different content.
    tx.execute(
        "UPDATE offers SET active = 0, updated_at = CURRENT_TIMESTAMP WHERE material_id = ?1 AND active = 1",
        params![material_id],
    )
    .context("failed to retire offers from the prior mineral revision")?;

    // An import is a complete evidence snapshot for this material. Source rows
    // remain reusable, while omitted material/source associations are retired.
    tx.execute(
        "DELETE FROM material_evidence WHERE material_id = ?1",
        params![material_id],
    )?;

    tx.execute(
        "DELETE FROM material_aliases WHERE material_id = ?1 AND origin = 'import'",
        params![material_id],
    )?;
    for synonym in &record.synonyms {
        tx.execute(
            r#"
            INSERT OR IGNORE INTO material_aliases(
                material_id, alias, alias_normalized, alias_type, origin
            ) VALUES (?1, ?2, ?3, 'synonym', 'import')
            "#,
            params![material_id, synonym.trim(), normalize_alias(synonym)],
        )?;
    }

    for source in &record.sources {
        let canonical_url = canonicalize_evidence_url(&source.url)?;
        let claim_scope = normalize_claim_scope(&source.claim_scope)?;
        let retrieved_at = normalize_timestamp("source.retrieved_at", &source.retrieved_at)?;
        tx.execute(
            r#"
            INSERT INTO evidence_sources(
                canonical_url, title, publisher, license_spdx, retrieved_at, content_hash
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(canonical_url) DO NOTHING
            "#,
            params![
                canonical_url,
                source.title.trim(),
                source.publisher.trim(),
                source.license_spdx.trim(),
                retrieved_at.as_str(),
                source.content_hash.trim(),
            ],
        )?;
        let source_id: i64 = tx.query_row(
            "SELECT id FROM evidence_sources WHERE canonical_url = ?1",
            params![canonical_url],
            |row| row.get(0),
        )?;
        tx.execute(
            r#"
            INSERT INTO material_evidence(
                material_id, source_id, claim_scope, claim_json, confidence, review_status,
                source_title, source_publisher, source_license_spdx,
                source_retrieved_at, source_content_hash
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9,
                ?10, ?11
            )
            ON CONFLICT(material_id, source_id, claim_scope) DO UPDATE SET
                claim_json = excluded.claim_json,
                confidence = excluded.confidence,
                review_status = excluded.review_status,
                source_title = excluded.source_title,
                source_publisher = excluded.source_publisher,
                source_license_spdx = excluded.source_license_spdx,
                source_retrieved_at = excluded.source_retrieved_at,
                source_content_hash = excluded.source_content_hash,
                updated_at = CURRENT_TIMESTAMP
            "#,
            params![
                material_id,
                source_id,
                claim_scope,
                serde_json::to_string(&source.claim)?,
                source.confidence,
                source.review_status,
                source.title.trim(),
                source.publisher.trim(),
                source.license_spdx.trim(),
                retrieved_at.as_str(),
                source.content_hash.trim(),
            ],
        )?;
    }
    Ok(())
}

fn load_evidence(conn: &Connection, material_id: i64) -> Result<Vec<EvidenceSummary>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
            COALESCE(me.source_title, es.title),
            COALESCE(me.source_publisher, es.publisher),
            es.canonical_url,
            COALESCE(me.source_license_spdx, es.license_spdx),
            me.claim_scope, me.claim_json, me.confidence, me.review_status,
            COALESCE(me.source_retrieved_at, es.retrieved_at),
            COALESCE(me.source_content_hash, es.content_hash),
            me.source_attribution_party, me.source_work_title,
            me.source_work_url, me.source_license_url,
            me.source_changes_notice, me.source_no_endorsement_notice,
            me.source_derived_output_license_spdx
        FROM material_evidence me
        JOIN evidence_sources es ON es.id = me.source_id
        WHERE me.material_id = ?1
        ORDER BY
            CASE me.review_status
                WHEN 'verified' THEN 0 WHEN 'reviewed' THEN 1
                WHEN 'unreviewed' THEN 2 ELSE 3 END,
            me.confidence DESC,
            COALESCE(me.source_publisher, es.publisher) COLLATE NOCASE ASC
        "#,
    )?;
    let stored_rows = stmt
        .query_map(params![material_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, f64>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, Option<String>>(12)?,
                row.get::<_, Option<String>>(13)?,
                row.get::<_, Option<String>>(14)?,
                row.get::<_, Option<String>>(15)?,
                row.get::<_, Option<String>>(16)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    stored_rows
        .into_iter()
        .map(
            |(
                title,
                publisher,
                canonical_url,
                license_spdx,
                claim_scope,
                claim_json,
                confidence,
                review_status,
                retrieved_at,
                content_hash,
                attribution_party,
                work_title,
                work_url,
                license_url,
                changes_notice,
                no_endorsement_notice,
                derived_output_license_spdx,
            )| {
                let claim = serde_json::from_str::<Value>(&claim_json)
                    .context("stored evidence claim is not valid JSON")?;
                let attribution_values = [
                    attribution_party.as_deref(),
                    work_title.as_deref(),
                    work_url.as_deref(),
                    license_url.as_deref(),
                    changes_notice.as_deref(),
                    no_endorsement_notice.as_deref(),
                    derived_output_license_spdx.as_deref(),
                ];
                let attribution = if attribution_values.iter().all(Option::is_none) {
                    None
                } else if attribution_values
                    .iter()
                    .all(|value| value.is_some_and(|value| !value.trim().is_empty()))
                {
                    Some(EvidenceAttributionSummary {
                        attribution_party: attribution_party.unwrap_or_default(),
                        work_title: work_title.unwrap_or_default(),
                        work_url: work_url.unwrap_or_default(),
                        license_url: license_url.unwrap_or_default(),
                        changes_notice: changes_notice.unwrap_or_default(),
                        no_endorsement_notice: no_endorsement_notice.unwrap_or_default(),
                        derived_output_license_spdx: derived_output_license_spdx
                            .unwrap_or_default(),
                    })
                } else {
                    bail!("stored evidence attribution snapshot is incomplete");
                };
                Ok(EvidenceSummary {
                    title,
                    publisher,
                    canonical_url,
                    license_spdx,
                    claim_label: claim_label_from_scope(&claim_scope),
                    claim_summary: claim_summary(&claim),
                    claim_scope,
                    claim,
                    confidence,
                    confidence_percent: confidence_percent(confidence),
                    review_status,
                    retrieved_at,
                    content_hash,
                    attribution,
                })
            },
        )
        .collect()
}

fn load_offers(conn: &Connection, material_id: i64) -> Result<Vec<ProviderOffer>> {
    let sql = format!(
        r#"
        SELECT
            p.name, p.slug, p.verification_status, p.trust_score,
            o.title, o.product_url, o.currency_code, o.price_minor,
            o.currency_exponent, o.pricing_basis, o.minimum_order_quantity,
            o.minimum_order_unit, o.stock_status, o.purity_text, o.grade,
            o.origin_country_code, o.verification_status, o.last_checked_at
        FROM offers o
        JOIN providers p ON p.id = o.provider_id
        WHERE o.material_id = ?1 AND {ACTIVE_OFFER_PREDICATE}
        ORDER BY
            CASE o.verification_status
                WHEN 'verified' THEN 0 WHEN 'observed' THEN 1
                WHEN 'provider_claim' THEN 2 ELSE 3 END,
            CASE p.verification_status
                WHEN 'verified' THEN 0 WHEN 'reviewed' THEN 1 ELSE 2 END,
            CASE o.stock_status
                WHEN 'in_stock' THEN 0 WHEN 'limited' THEN 1
                WHEN 'made_to_order' THEN 2 WHEN 'quote_required' THEN 3
                WHEN 'unknown' THEN 4 ELSE 5 END,
            p.trust_score DESC,
            o.last_checked_at DESC
        "#
    );
    let mut stmt = conn.prepare(&sql)?;

    let rows = stmt
        .query_map(params![material_id], |row| {
            let currency_code: String = row.get(6)?;
            let price_minor: Option<i64> = row.get(7)?;
            let exponent: i64 = row.get(8)?;
            let pricing_basis: String = row.get(9)?;
            let minimum_quantity: Option<f64> = row.get(10)?;
            let minimum_unit: String = row.get(11)?;
            Ok(ProviderOffer {
                provider_name: row.get(0)?,
                provider_slug: row.get(1)?,
                provider_verification_status: row.get(2)?,
                provider_trust_score: row.get(3)?,
                title: row.get(4)?,
                product_url: row.get(5)?,
                price_display: format_price(price_minor, exponent, &currency_code),
                pricing_basis_display: pricing_basis_display(&pricing_basis),
                pricing_basis,
                minimum_order_display: format_quantity(minimum_quantity, &minimum_unit),
                stock_status: row.get(12)?,
                purity_text: row.get(13)?,
                grade: row.get(14)?,
                origin_country_code: row.get(15)?,
                verification_status: row.get(16)?,
                last_checked_at: row.get(17)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn validate_evidence(source: &EvidenceImport) -> Result<()> {
    canonicalize_evidence_url(&source.url)?;
    validate_text("source.title", &source.title, 1, 500)?;
    validate_text("source.publisher", &source.publisher, 0, 240)?;
    validate_text("source.license_spdx", &source.license_spdx, 1, 120)?;
    let claim_scope = normalize_claim_scope(&source.claim_scope)?;
    validate_text("source.content_hash", &source.content_hash, 0, 256)?;
    ensure_json_object("source.claim", &source.claim)?;
    if claim_scope.contains('.')
        && source
            .claim
            .get("value")
            .is_none_or(serde_json::Value::is_null)
    {
        bail!("source.claim.value is required for granular claim scopes");
    }
    if claim_scope.contains('.') {
        for field in ["unit", "source_locator", "note"] {
            if source
                .claim
                .get(field)
                .is_some_and(|value| !value.is_null() && !value.is_string())
            {
                bail!("source.claim.{field} must be a string when provided");
            }
        }
        if source
            .claim
            .get("conditions")
            .is_some_and(|value| !value.is_null() && !value.is_object())
        {
            bail!("source.claim.conditions must be an object when provided");
        }
    }
    if serde_json::to_vec(&source.claim)?.len() > 100_000 {
        bail!("source.claim exceeds 100000 encoded bytes");
    }
    validate_text("source.retrieved_at", &source.retrieved_at, 1, 64)?;
    normalize_timestamp("source.retrieved_at", &source.retrieved_at)?;
    if !(0.0..=1.0).contains(&source.confidence) {
        bail!("source confidence must be between 0 and 1");
    }
    if !matches!(
        source.review_status.as_str(),
        "unreviewed" | "reviewed" | "verified" | "disputed"
    ) {
        bail!(
            "unsupported source review_status '{}'",
            source.review_status
        );
    }
    Ok(())
}

fn normalize_claim_scope(value: &str) -> Result<String> {
    validate_text("source.claim_scope", value, 1, 120)?;
    let normalized = value.trim().to_ascii_lowercase();
    if matches!(
        normalized.as_str(),
        "identity" | "identifiers" | "properties" | "safety"
    ) {
        return Ok(normalized);
    }

    let Some((section, key)) = normalized.split_once('.') else {
        bail!("source.claim_scope must be a legacy broad scope or a canonical granular scope");
    };
    if key.contains('.') || key.is_empty() || !key.chars().all(is_claim_scope_key_character) {
        bail!("source.claim_scope contains an invalid granular key");
    }
    match section {
        "identity"
            if matches!(
                key,
                "canonical_name" | "formula" | "mineral_family" | "description"
            ) => {}
        "identifiers" | "properties" | "safety" => {}
        "identity" => bail!("source.claim_scope contains an unsupported identity field"),
        _ => bail!("source.claim_scope contains an unsupported section"),
    }
    Ok(format!("{section}.{key}"))
}

fn is_claim_scope_key_character(character: char) -> bool {
    character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
}

fn validate_provider_import(provider: &ProviderImport) -> Result<()> {
    if !is_valid_registry_slug(&provider.slug) {
        bail!("invalid provider slug '{}'", provider.slug);
    }
    validate_text("provider.name", &provider.name, 1, 240)?;
    validate_http_url("provider.website_url", &provider.website_url)?;
    validate_text("provider.network_kind", &provider.network_kind, 1, 80)?;
    if !provider.country_code.is_empty()
        && (provider.country_code.len() != 2
            || !provider
                .country_code
                .chars()
                .all(|ch| ch.is_ascii_alphabetic()))
    {
        bail!("provider.country_code must be a two-letter country code");
    }
    if !matches!(
        provider.verification_status.as_str(),
        "unverified" | "reviewed" | "verified" | "suspended"
    ) {
        bail!(
            "unsupported provider verification_status '{}'",
            provider.verification_status
        );
    }
    if !(0.0..=1.0).contains(&provider.trust_score) {
        bail!("provider.trust_score must be between 0 and 1");
    }
    if provider.offers.len() > 10_000 {
        bail!("provider import exceeds the 10000-offer batch limit");
    }
    for offer in &provider.offers {
        validate_offer_import(offer)?;
    }
    Ok(())
}

fn validate_offer_import(offer: &OfferImport) -> Result<()> {
    if !is_valid_registry_slug(&offer.material_slug) {
        bail!("invalid offer mineral_slug '{}'", offer.material_slug);
    }
    validate_text("offer.external_id", &offer.external_id, 1, 240)?;
    validate_text("offer.title", &offer.title, 1, 500)?;
    validate_text("offer.pricing_basis", &offer.pricing_basis, 1, 80)?;
    validate_text("offer.minimum_order_unit", &offer.minimum_order_unit, 0, 40)?;
    validate_text(
        "offer.available_quantity_unit",
        &offer.available_quantity_unit,
        0,
        40,
    )?;
    validate_text("offer.purity_text", &offer.purity_text, 0, 500)?;
    validate_text("offer.grade", &offer.grade, 0, 240)?;
    validate_http_url("offer.product_url", &offer.product_url)?;
    ensure_json_object("offer.provider_claims", &offer.provider_claims)?;
    if serde_json::to_vec(&offer.provider_claims)?.len() > 100_000 {
        bail!("offer.provider_claims exceeds 100000 encoded bytes");
    }
    if offer.price_minor.is_some() == offer.currency_code.trim().is_empty() {
        bail!("offer price_minor and currency_code must be provided together");
    }
    if offer.price_minor.is_some_and(|value| value < 0) {
        bail!("offer price_minor cannot be negative");
    }
    if !offer.currency_code.is_empty()
        && (offer.currency_code.len() != 3
            || !offer
                .currency_code
                .chars()
                .all(|ch| ch.is_ascii_alphabetic()))
    {
        bail!("offer currency_code must be a three-letter ISO currency code");
    }
    if !offer.origin_country_code.is_empty()
        && (offer.origin_country_code.len() != 2
            || !offer
                .origin_country_code
                .chars()
                .all(|ch| ch.is_ascii_alphabetic()))
    {
        bail!("offer.origin_country_code must be a two-letter country code");
    }
    if !(0..=6).contains(&offer.currency_exponent) {
        bail!("offer currency_exponent must be between 0 and 6");
    }
    for (label, quantity) in [
        ("minimum_order_quantity", offer.minimum_order_quantity),
        ("available_quantity", offer.available_quantity),
    ] {
        if quantity.is_some_and(|value| !value.is_finite() || value <= 0.0) {
            bail!("offer {label} must be finite and greater than zero");
        }
    }
    if !matches!(
        offer.stock_status.as_str(),
        "in_stock" | "limited" | "made_to_order" | "quote_required" | "out_of_stock" | "unknown"
    ) {
        bail!("unsupported offer stock_status '{}'", offer.stock_status);
    }
    if !matches!(
        offer.verification_status.as_str(),
        "provider_claim" | "observed" | "verified" | "disputed"
    ) {
        bail!(
            "unsupported offer verification_status '{}'",
            offer.verification_status
        );
    }
    if offer.verification_status == "verified" && offer.evidence_url.is_none() {
        bail!("verified offers require evidence_url");
    }
    if let Some(url) = &offer.evidence_url {
        canonicalize_evidence_url(url)?;
    }
    if !offer.last_checked_at.trim().is_empty() {
        normalize_timestamp("offer.last_checked_at", &offer.last_checked_at)?;
    }
    if let Some(expires_at) = offer.expires_at.as_deref() {
        normalize_timestamp("offer.expires_at", expires_at)?;
    }
    Ok(())
}

fn canonicalize_evidence_url(value: &str) -> Result<String> {
    validate_text("evidence URL", value, 1, 2_048)?;
    let mut url =
        Url::parse(value.trim()).with_context(|| format!("invalid evidence URL '{value}'"))?;
    if !matches!(url.scheme(), "http" | "https") {
        bail!("evidence URL must use http or https: '{value}'");
    }
    if url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() {
        bail!("evidence URL must have a host and cannot contain credentials");
    }
    url.set_fragment(None);
    let default_port = match url.scheme() {
        "http" => Some(80),
        "https" => Some(443),
        _ => None,
    };
    if url.port() == default_port {
        url.set_port(None)
            .map_err(|_| anyhow::anyhow!("invalid evidence URL port"))?;
    }
    Ok(url.into())
}

fn normalize_timestamp(label: &str, value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        bail!("{label} cannot be empty");
    }
    if let Ok(timestamp) = DateTime::parse_from_rfc3339(value) {
        return Ok(timestamp
            .with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::Secs, true));
    }
    if let Ok(timestamp) = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f") {
        return Ok(timestamp
            .and_utc()
            .to_rfc3339_opts(SecondsFormat::Secs, true));
    }
    bail!("{label} must be RFC 3339 or SQLite format YYYY-MM-DD HH:MM:SS")
}

fn validate_mineral_manifest(manifest: &MineralDatasetManifest) -> Result<()> {
    if manifest.schema_version != MINERAL_INGESTION_SCHEMA_VERSION {
        bail!(
            "manifest schema_version must be {}",
            MINERAL_INGESTION_SCHEMA_VERSION
        );
    }
    validate_dataset_key("manifest.dataset.key", &manifest.dataset.key)?;
    validate_text("manifest.dataset.title", &manifest.dataset.title, 1, 240)?;
    validate_dataset_key("manifest.source.key", &manifest.source.key)?;
    validate_http_url("manifest.source.url", &manifest.source.url)?;
    validate_text(
        "manifest.source.license_spdx",
        &manifest.source.license_spdx,
        1,
        120,
    )?;
    validate_explicit_spdx_license(
        "manifest.source.license_spdx",
        &manifest.source.license_spdx,
    )?;
    let attribution = required_manifest_attribution(manifest)?;
    validate_text(
        "manifest.source.attribution.attribution_party",
        &attribution.attribution_party,
        1,
        500,
    )?;
    validate_text(
        "manifest.source.attribution.work_title",
        &attribution.work_title,
        1,
        500,
    )?;
    validate_http_url(
        "manifest.source.attribution.work_url",
        &attribution.work_url,
    )?;
    validate_canonical_license_url(
        "manifest.source.attribution.license_url",
        &attribution.license_url,
    )?;
    validate_text(
        "manifest.source.attribution.changes_notice",
        &attribution.changes_notice,
        10,
        2_000,
    )?;
    validate_text(
        "manifest.source.attribution.no_endorsement_notice",
        &attribution.no_endorsement_notice,
        10,
        1_000,
    )?;
    validate_explicit_spdx_license(
        "manifest.source.attribution.derived_output_license_spdx",
        &attribution.derived_output_license_spdx,
    )?;
    for (label, value) in [
        (
            "manifest.source.attribution.attribution_party",
            attribution.attribution_party.as_str(),
        ),
        (
            "manifest.source.attribution.work_title",
            attribution.work_title.as_str(),
        ),
        (
            "manifest.source.attribution.work_url",
            attribution.work_url.as_str(),
        ),
        (
            "manifest.source.attribution.changes_notice",
            attribution.changes_notice.as_str(),
        ),
        (
            "manifest.source.attribution.no_endorsement_notice",
            attribution.no_endorsement_notice.as_str(),
        ),
    ] {
        if value.trim() != value {
            bail!("{label} cannot have surrounding whitespace");
        }
    }
    validate_attribution_license_compatibility(manifest, attribution)?;
    validate_text(
        "manifest.release.version",
        &manifest.release.version,
        1,
        120,
    )?;
    normalize_date_or_timestamp(
        "manifest.release.released_at",
        &manifest.release.released_at,
    )?;
    normalize_timestamp(
        "manifest.retrieval.retrieved_at",
        &manifest.retrieval.retrieved_at,
    )?;
    validate_http_url("manifest.artifact.url", &manifest.artifact.url)?;
    validate_sha256("manifest.artifact.sha256", &manifest.artifact.sha256)?;
    validate_text("manifest.parser.name", &manifest.parser.name, 1, 120)?;
    validate_text("manifest.parser.version", &manifest.parser.version, 1, 120)?;
    validate_text(
        "manifest.parser.code_revision",
        &manifest.parser.code_revision,
        1,
        240,
    )?;
    validate_sha256(
        "manifest.parser.configuration_sha256",
        &manifest.parser.configuration_sha256,
    )?;
    if !(1..=MAX_MINERAL_INGESTION_RECORDS).contains(&manifest.expected_record_count) {
        bail!(
            "manifest expected_record_count must be between 1 and {MAX_MINERAL_INGESTION_RECORDS}"
        );
    }
    if !(1..=MAX_MINERAL_INGESTION_CHUNKS).contains(&manifest.expected_chunk_count) {
        bail!("manifest expected_chunk_count must be between 1 and {MAX_MINERAL_INGESTION_CHUNKS}");
    }
    if manifest.expected_chunk_count > manifest.expected_record_count {
        bail!("manifest expected_chunk_count cannot exceed expected_record_count");
    }
    if manifest.expected_record_count
        > manifest.expected_chunk_count * MAX_MINERAL_INGESTION_CHUNK_ITEMS
    {
        bail!(
            "manifest record count cannot fit within {} chunks of at most {} items",
            manifest.expected_chunk_count,
            MAX_MINERAL_INGESTION_CHUNK_ITEMS
        );
    }
    validate_sha256("manifest.records_sha256", &manifest.records_sha256)?;
    if let Some(base_batch_id) = manifest.base_batch_id.as_deref() {
        validate_batch_id(base_batch_id)?;
    }
    for (label, value) in [
        ("manifest.dataset.title", manifest.dataset.title.as_str()),
        ("manifest.source.url", manifest.source.url.as_str()),
        (
            "manifest.source.license_spdx",
            manifest.source.license_spdx.as_str(),
        ),
        (
            "manifest.source.attribution.attribution_party",
            attribution.attribution_party.as_str(),
        ),
        (
            "manifest.source.attribution.work_title",
            attribution.work_title.as_str(),
        ),
        (
            "manifest.source.attribution.work_url",
            attribution.work_url.as_str(),
        ),
        (
            "manifest.source.attribution.license_url",
            attribution.license_url.as_str(),
        ),
        (
            "manifest.source.attribution.changes_notice",
            attribution.changes_notice.as_str(),
        ),
        (
            "manifest.source.attribution.no_endorsement_notice",
            attribution.no_endorsement_notice.as_str(),
        ),
        (
            "manifest.source.attribution.derived_output_license_spdx",
            attribution.derived_output_license_spdx.as_str(),
        ),
        (
            "manifest.release.version",
            manifest.release.version.as_str(),
        ),
        (
            "manifest.release.released_at",
            manifest.release.released_at.as_str(),
        ),
        (
            "manifest.retrieval.retrieved_at",
            manifest.retrieval.retrieved_at.as_str(),
        ),
        ("manifest.artifact.url", manifest.artifact.url.as_str()),
        ("manifest.parser.name", manifest.parser.name.as_str()),
        ("manifest.parser.version", manifest.parser.version.as_str()),
        (
            "manifest.parser.code_revision",
            manifest.parser.code_revision.as_str(),
        ),
    ] {
        validate_no_control_characters(label, value)?;
    }
    Ok(())
}

fn required_manifest_attribution(
    manifest: &MineralDatasetManifest,
) -> Result<&MineralSourceAttribution> {
    if manifest.schema_version != MINERAL_INGESTION_SCHEMA_VERSION {
        bail!(
            "manifest schema_version {} cannot be published; restage as schema version {}",
            manifest.schema_version,
            MINERAL_INGESTION_SCHEMA_VERSION
        );
    }
    manifest.source.attribution.as_ref().context(
        "manifest.source.attribution is required; historical schema-v1 batches must be rejected and restaged",
    )
}

fn validate_explicit_spdx_license(label: &str, value: &str) -> Result<()> {
    validate_text(label, value, 1, 120)?;
    let license = value.trim();
    if license.eq_ignore_ascii_case("NOASSERTION")
        || license.eq_ignore_ascii_case("NONE")
        || license != value
        || !license.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '-' | '.' | '+' | '(' | ')' | ' ')
        })
    {
        bail!("{label} requires an explicit valid SPDX license expression");
    }
    Ok(())
}

fn validate_canonical_license_url(label: &str, value: &str) -> Result<()> {
    validate_http_url(label, value)?;
    let url = Url::parse(value)?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("{label} must be a canonical HTTPS URL without credentials, query, or fragment");
    }
    if value != url.as_str() {
        bail!("{label} must use the URL parser's canonical form");
    }
    Ok(())
}

fn validate_attribution_license_compatibility(
    manifest: &MineralDatasetManifest,
    attribution: &MineralSourceAttribution,
) -> Result<()> {
    let source_license = manifest.source.license_spdx.as_str();
    let derived_license = attribution.derived_output_license_spdx.as_str();
    if source_license.contains("CC-BY-SA-") && derived_license != source_license {
        bail!(
            "share-alike source license {source_license} requires an explicitly compatible derived-output license; this release must use the same SPDX expression"
        );
    }
    let expected_cc_url = match source_license {
        "CC-BY-SA-3.0" => Some("https://creativecommons.org/licenses/by-sa/3.0/"),
        "CC-BY-SA-4.0" => Some("https://creativecommons.org/licenses/by-sa/4.0/"),
        "CC-BY-3.0" => Some("https://creativecommons.org/licenses/by/3.0/"),
        "CC-BY-4.0" => Some("https://creativecommons.org/licenses/by/4.0/"),
        "CC0-1.0" => Some("https://creativecommons.org/publicdomain/zero/1.0/"),
        _ => None,
    };
    if (source_license.starts_with("CC-BY-") || source_license.starts_with("CC-BY-SA-"))
        && expected_cc_url.is_none()
    {
        bail!(
            "manifest.source.license_spdx uses a Creative Commons attribution license whose canonical URL and adaptation compatibility are not supported"
        );
    }
    if let Some(expected) = expected_cc_url {
        if attribution.license_url != expected {
            bail!(
                "manifest.source.attribution.license_url must be the canonical URL {expected} for {source_license}"
            );
        }
    }
    Ok(())
}

fn validate_mineral_chunk(chunk: &MineralIngestionChunk) -> Result<()> {
    if chunk.schema_version != MINERAL_INGESTION_SCHEMA_VERSION {
        bail!(
            "chunk schema_version must be {}",
            MINERAL_INGESTION_SCHEMA_VERSION
        );
    }
    if chunk.items.is_empty() {
        bail!("mineral ingestion chunk contains no items");
    }
    if chunk.items.len() > MAX_MINERAL_INGESTION_CHUNK_ITEMS {
        bail!(
            "mineral ingestion chunk exceeds the {}-item limit",
            MAX_MINERAL_INGESTION_CHUNK_ITEMS
        );
    }
    for (index, item) in chunk.items.iter().enumerate() {
        validate_mineral_ingestion_item(item)
            .with_context(|| format!("chunk item {} ({})", index + 1, item.source_record_id))?;
    }
    Ok(())
}

fn validate_mineral_ingestion_item(item: &MineralIngestionItem) -> Result<()> {
    validate_text("item.source_record_id", &item.source_record_id, 1, 240)?;
    if item.source_record_id.trim() != item.source_record_id {
        bail!("item.source_record_id cannot have surrounding whitespace");
    }
    if let Some(locator) = item.source_locator.as_deref() {
        validate_text("item.source_locator", locator, 1, 500)?;
    }
    if !is_valid_registry_slug(&item.slug) || !item.slug.starts_with("mineral.") {
        bail!("invalid mineral item slug '{}'", item.slug);
    }
    validate_text("item.canonical_name", &item.canonical_name, 1, 240)?;
    validate_text("item.formula", &item.formula, 0, 500)?;
    if !matches!(
        item.nomenclature_status.as_str(),
        "approved"
            | "grandfathered"
            | "renamed"
            | "redefined"
            | "discredited"
            | "questionable"
            | "uncertain"
            | "unknown"
    ) {
        bail!(
            "unsupported item nomenclature_status '{}'",
            item.nomenclature_status
        );
    }
    if item.nomenclature_status == "discredited" && item.is_valid_species {
        bail!("a discredited mineral cannot be marked as a valid species");
    }
    if item.official_identifiers.len() > 16 {
        bail!("item official_identifiers exceeds 16 entries");
    }
    for (key, value) in &item.official_identifiers {
        if !matches!(key.as_str(), "ima_number" | "ima_symbol") {
            bail!("unsupported authority identifier key '{key}'");
        }
        validate_text("item official identifier", value, 1, 240)?;
    }
    validate_mineral_official_facts(item)?;
    if item.synonyms.len() > 100 {
        bail!("item synonyms exceeds 100 entries");
    }
    let mut synonyms = HashSet::new();
    let canonical_name = normalize_identity_text(&item.canonical_name);
    for synonym in &item.synonyms {
        validate_text("item synonym", synonym, 1, 240)?;
        let normalized = normalize_identity_text(synonym);
        if normalized == canonical_name {
            bail!("item synonym duplicates its canonical name");
        }
        if !synonyms.insert(normalized) {
            bail!("item contains duplicate synonyms");
        }
    }
    for (label, value) in [
        ("item.source_record_id", item.source_record_id.as_str()),
        ("item.slug", item.slug.as_str()),
        ("item.canonical_name", item.canonical_name.as_str()),
        ("item.formula", item.formula.as_str()),
        (
            "item.nomenclature_status",
            item.nomenclature_status.as_str(),
        ),
    ] {
        validate_no_control_characters(label, value)?;
    }
    if let Some(locator) = item.source_locator.as_deref() {
        validate_no_control_characters("item.source_locator", locator)?;
    }
    for (key, value) in &item.official_identifiers {
        validate_no_control_characters("item official identifier key", key)?;
        validate_no_control_characters("item official identifier", value)?;
    }
    for synonym in &item.synonyms {
        validate_no_control_characters("item synonym", synonym)?;
    }
    Ok(())
}

fn validate_mineral_official_facts(item: &MineralIngestionItem) -> Result<()> {
    for (label, value, max) in [
        (
            "item.official_facts.discovery_country",
            item.official_facts.discovery_country.as_str(),
            240,
        ),
        (
            "item.official_facts.first_reference",
            item.official_facts.first_reference.as_str(),
            500,
        ),
        (
            "item.official_facts.second_reference",
            item.official_facts.second_reference.as_str(),
            500,
        ),
        (
            "item.official_facts.source_status",
            item.official_facts.source_status.as_str(),
            40,
        ),
    ] {
        validate_text(label, value, 0, max)?;
        if value.trim() != value {
            bail!("{label} cannot have surrounding whitespace");
        }
        validate_no_control_characters(label, value)?;
    }
    if !item.official_facts.source_status.is_empty() {
        let expected = match item.official_facts.source_status.as_str() {
            "A" => ("approved", true),
            "A ?" => ("uncertain", true),
            "G" => ("grandfathered", true),
            "Rd" => ("redefined", true),
            "Rn" => ("renamed", true),
            "Q" => ("questionable", true),
            "D" => ("discredited", false),
            value => bail!("unsupported official source status '{value}'"),
        };
        if (item.nomenclature_status.as_str(), item.is_valid_species) != expected {
            bail!(
                "official source status '{}' is inconsistent with nomenclature status and validity",
                item.official_facts.source_status
            );
        }
    }
    Ok(())
}

fn validate_mineral_ingestion_actor(actor: &str) -> Result<()> {
    validate_text("ingestion actor", actor, 1, 240)?;
    if actor.trim() != actor {
        bail!("ingestion actor cannot have surrounding whitespace");
    }
    validate_no_control_characters("ingestion actor", actor)?;
    Ok(())
}

fn validate_no_control_characters(label: &str, value: &str) -> Result<()> {
    if value.chars().any(char::is_control) {
        bail!("{label} cannot contain control characters");
    }
    Ok(())
}

fn validate_dataset_key(label: &str, value: &str) -> Result<()> {
    validate_text(label, value, 1, 120)?;
    if value.trim() != value
        || !value.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '.' | '_' | '-')
        })
        || !value
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
    {
        bail!("{label} must be a lowercase stable key");
    }
    Ok(())
}

fn validate_sha256(label: &str, value: &str) -> Result<()> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        bail!("{label} must use the sha256:<64 lowercase hex> form");
    };
    if hex.len() != 64
        || !hex
            .chars()
            .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character))
    {
        bail!("{label} must use the sha256:<64 lowercase hex> form");
    }
    Ok(())
}

fn validate_batch_id(value: &str) -> Result<()> {
    let Some(hex) = value.strip_prefix("batch_") else {
        bail!("invalid mineral ingestion batch id");
    };
    if hex.len() != 64
        || !hex
            .chars()
            .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character))
    {
        bail!("invalid mineral ingestion batch id");
    }
    Ok(())
}

fn normalize_date_or_timestamp(label: &str, value: &str) -> Result<String> {
    if let Ok(date) = chrono::NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d") {
        return Ok(date.format("%Y-%m-%d").to_string());
    }
    normalize_timestamp(label, value)
}

fn normalize_identity_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn normalize_formula(value: &str) -> String {
    normalize_owned_text(value)
}

fn normalize_owned_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn canonical_json_bytes<T: ?Sized + Serialize>(value: &T) -> Result<Vec<u8>> {
    let value = serde_json::to_value(value).context("failed to encode canonical JSON value")?;
    let mut output = Vec::new();
    write_canonical_json(&value, &mut output)?;
    Ok(output)
}

fn write_canonical_json(value: &Value, output: &mut Vec<u8>) -> Result<()> {
    match value {
        Value::Null => output.extend_from_slice(b"null"),
        Value::Bool(value) => output.extend_from_slice(if *value { b"true" } else { b"false" }),
        Value::Number(value) => output.extend_from_slice(value.to_string().as_bytes()),
        Value::String(value) => output.extend_from_slice(serde_json::to_string(value)?.as_bytes()),
        Value::Array(values) => {
            output.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    output.push(b',');
                }
                write_canonical_json(value, output)?;
            }
            output.push(b']');
        }
        Value::Object(values) => {
            output.push(b'{');
            let sorted = values.iter().collect::<BTreeMap<_, _>>();
            for (index, (key, value)) in sorted.into_iter().enumerate() {
                if index > 0 {
                    output.push(b',');
                }
                output.extend_from_slice(serde_json::to_string(key)?.as_bytes());
                output.push(b':');
                write_canonical_json(value, output)?;
            }
            output.push(b'}');
        }
    }
    Ok(())
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format_sha256_digest(digest(&SHA256, bytes).as_ref())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)
        .with_context(|| format!("failed to open file for hashing {}", path.display()))?;
    let mut context = DigestContext::new(&SHA256);
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("failed to hash file {}", path.display()))?;
        if read == 0 {
            break;
        }
        context.update(&buffer[..read]);
    }
    Ok(format_sha256_digest(context.finish().as_ref()))
}

fn format_sha256_digest(bytes: &[u8]) -> String {
    let hex = bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("sha256:{hex}")
}

fn mineral_ingestion_problem(
    kind: MineralIngestionProblemKind,
    code: &'static str,
    message: impl Into<String>,
) -> anyhow::Error {
    MineralIngestionProblem {
        kind,
        code,
        message: message.into(),
    }
    .into()
}

fn validate_http_url(label: &str, value: &str) -> Result<()> {
    validate_text(label, value, 1, 2_048)?;
    let url = Url::parse(value.trim()).with_context(|| format!("invalid {label}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        bail!("{label} must use http or https");
    }
    if url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() {
        bail!("{label} must have a host and cannot contain credentials");
    }
    Ok(())
}

fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    if !table
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        bail!("invalid schema table name");
    }
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .context("failed to inspect registry table columns")?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(columns.iter().any(|candidate| candidate == column))
}

fn open_connection(data_root: &Path, write: bool) -> Result<Connection> {
    let db_path = data_root.join(DATABASE_FILE);
    let conn = if write {
        Connection::open(&db_path)
    } else {
        Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
    }
    .with_context(|| format!("failed to open registry database {}", db_path.display()))?;
    conn.busy_timeout(Duration::from_secs(5))
        .context("failed to configure registry busy timeout")?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    if write {
        let durability = sqlite_durability_mode()?;
        conn.execute_batch("PRAGMA journal_mode = WAL;")?;
        match durability {
            "FULL" => conn.execute_batch("PRAGMA synchronous = FULL;")?,
            "NORMAL" => conn.execute_batch("PRAGMA synchronous = NORMAL;")?,
            _ => unreachable!("durability mode was validated"),
        }
    }
    Ok(conn)
}

fn sqlite_durability_mode() -> Result<&'static str> {
    let configured = std::env::var("SQLITE_DURABILITY").unwrap_or_else(|_| {
        if cfg!(debug_assertions) {
            "NORMAL".to_string()
        } else {
            "FULL".to_string()
        }
    });
    match configured.trim().to_ascii_uppercase().as_str() {
        "FULL" => Ok("FULL"),
        "NORMAL" => Ok("NORMAL"),
        value => bail!("unsupported SQLITE_DURABILITY '{value}'; expected FULL or NORMAL"),
    }
}

fn mineral_ingestion_limits() -> Result<MineralIngestionLimits> {
    let batch_max_bytes = ingestion_env_u64(
        "INGESTION_BATCH_MAX_BYTES",
        DEFAULT_INGESTION_BATCH_MAX_BYTES,
        MIN_INGESTION_BATCH_MAX_BYTES,
        MAX_INGESTION_BATCH_MAX_BYTES,
    )?;
    let quarantine_max_bytes = ingestion_env_u64(
        "INGESTION_QUARANTINE_MAX_BYTES",
        DEFAULT_INGESTION_QUARANTINE_MAX_BYTES,
        batch_max_bytes,
        MAX_INGESTION_QUARANTINE_MAX_BYTES,
    )?;
    if quarantine_max_bytes < batch_max_bytes {
        bail!("INGESTION_QUARANTINE_MAX_BYTES cannot be smaller than INGESTION_BATCH_MAX_BYTES");
    }
    let abandoned_hours = ingestion_env_u64(
        "INGESTION_ABANDONED_HOURS",
        DEFAULT_INGESTION_ABANDONED_HOURS,
        1,
        MAX_INGESTION_ABANDONED_HOURS,
    )?;
    Ok(MineralIngestionLimits {
        batch_max_bytes,
        quarantine_max_bytes,
        abandoned_hours,
    })
}

fn ingestion_env_u64(name: &str, default: u64, minimum: u64, maximum: u64) -> Result<u64> {
    let Some(raw) = std::env::var_os(name) else {
        return Ok(default);
    };
    let raw = raw
        .into_string()
        .map_err(|_| anyhow::anyhow!("{name} must be valid UTF-8"))?;
    let value = raw
        .trim()
        .parse::<u64>()
        .with_context(|| format!("{name} must be an integer number of bytes/hours"))?;
    if !(minimum..=maximum).contains(&value) {
        bail!("{name} must be between {minimum} and {maximum}");
    }
    Ok(value)
}

fn count(conn: &Connection, sql: &str) -> Result<usize> {
    Ok(conn.query_row(sql, [], |row| row.get::<_, i64>(0))? as usize)
}

fn make_fts_query(query: &str) -> String {
    query
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .take(12)
        .map(|token| format!("\"{token}\"*"))
        .collect::<Vec<_>>()
        .join(" OR ")
}

fn json_object_to_facts(raw: &str) -> Vec<MaterialFact> {
    let Ok(Value::Object(values)) = serde_json::from_str::<Value>(raw) else {
        return Vec::new();
    };
    let values = values.into_iter().collect::<BTreeMap<_, _>>();
    values
        .into_iter()
        .filter_map(|(name, value)| {
            let rendered = json_value_to_public_text(&value, name.ends_with("_pct"));
            (!rendered.is_empty()).then(|| MaterialFact {
                name: humanize_fact_name(&name),
                key: name,
                value: rendered,
            })
        })
        .collect()
}

fn claim_label_from_scope(scope: &str) -> String {
    let Ok(normalized) = normalize_claim_scope(scope) else {
        return "Scientific claim".to_string();
    };
    match normalized.as_str() {
        "identity" => "Identity".to_string(),
        "identifiers" => "Identifiers".to_string(),
        "properties" => "Properties".to_string(),
        "safety" => "Safety".to_string(),
        "identity.canonical_name" => "Canonical name".to_string(),
        "identity.formula" => "Formula".to_string(),
        "identity.mineral_family" => "Mineral family".to_string(),
        "identity.description" => "Description".to_string(),
        _ => normalized
            .split_once('.')
            .map(|(_, key)| humanize_fact_name(key))
            .filter(|label| !label.is_empty())
            .unwrap_or_else(|| "Scientific claim".to_string()),
    }
}

fn claim_summary(claim: &Value) -> String {
    let Value::Object(fields) = claim else {
        return String::new();
    };

    let rendered = if let Some(value) = fields.get("value") {
        let mut parts = Vec::new();
        let mut assertion = json_value_to_public_text(value, false);
        if let Some(unit) = fields.get("unit").and_then(Value::as_str) {
            let unit = unit.trim();
            if !assertion.is_empty() && !unit.is_empty() {
                assertion.push(' ');
                assertion.push_str(unit);
            }
        }
        if !assertion.is_empty() {
            parts.push(assertion);
        }
        if let Some(conditions) = fields.get("conditions") {
            let conditions = json_value_to_public_text(conditions, false);
            if !conditions.is_empty() {
                parts.push(conditions);
            }
        }
        for field in ["source_locator", "note"] {
            if let Some(value) = fields.get(field).and_then(Value::as_str) {
                let value = value.trim();
                if !value.is_empty() {
                    parts.push(value.to_string());
                }
            }
        }
        parts.join(" · ")
    } else {
        // Broad legacy claims predate the envelope. Render their values as
        // ordinary text rather than exposing their JSON representation.
        json_value_to_public_text(claim, false)
    };
    bounded_public_text(&rendered, MAX_CLAIM_SUMMARY_CHARS)
}

fn confidence_percent(confidence: f64) -> u8 {
    (confidence.clamp(0.0, 1.0) * 100.0).round() as u8
}

fn bounded_public_text(value: &str, max_chars: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }
    let mut truncated = normalized
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    if let Some(word_boundary) = truncated.rfind(' ') {
        truncated.truncate(word_boundary);
    }
    truncated.push('…');
    truncated
}

fn humanize_fact_name(name: &str) -> String {
    match name.to_ascii_lowercase().as_str() {
        "appearance" => "Appearance".to_string(),
        "boiling_point_c" => "Boiling point (°C)".to_string(),
        "cas" | "cas_number" => "CAS Registry Number".to_string(),
        "color" | "colour" => "Color".to_string(),
        "crystal_system" => "Crystal system".to_string(),
        "density_g_cm3" => "Density (g/cm³)".to_string(),
        "disposal" => "Disposal".to_string(),
        "first_aid" => "First aid".to_string(),
        "handling" => "Handling".to_string(),
        "hardness_mohs" => "Hardness (Mohs)".to_string(),
        "hazards" => "Hazards".to_string(),
        "ima_number" => "IMA number".to_string(),
        "ima_symbol" => "IMA symbol".to_string(),
        "luster" | "lustre" => "Luster".to_string(),
        "major_elements_pct" => "Major elements (%)".to_string(),
        "melting_point_c" => "Melting point (°C)".to_string(),
        "molar_mass_g_mol" => "Molar mass (g/mol)".to_string(),
        "ppe" => "Protective equipment".to_string(),
        "storage" => "Storage".to_string(),
        "streak" => "Streak".to_string(),
        _ => {
            let words = name
                .split(|character: char| !character.is_alphanumeric())
                .filter(|word| !word.is_empty())
                .collect::<Vec<_>>();
            let label = words.join(" ");
            let mut characters = label.chars();
            match characters.next() {
                Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
                None => String::new(),
            }
        }
    }
}

fn json_value_to_public_text(value: &Value, percentage_values: bool) -> String {
    match value {
        Value::String(value) => value.trim().to_string(),
        Value::Number(value) => format_json_number(value),
        Value::Bool(value) => {
            if *value {
                "Yes".to_string()
            } else {
                "No".to_string()
            }
        }
        Value::Array(values) => values
            .iter()
            .map(|value| json_value_to_public_text(value, false))
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        Value::Object(values) => values
            .iter()
            .filter_map(|(name, value)| {
                let rendered = json_value_to_public_text(value, false);
                if rendered.is_empty() {
                    return None;
                }
                let label = humanize_fact_name(name);
                if percentage_values && value.is_number() {
                    Some(format!("{label} {rendered}%"))
                } else {
                    Some(format!("{label}: {rendered}"))
                }
            })
            .collect::<Vec<_>>()
            .join(", "),
        Value::Null => String::new(),
    }
}

fn format_json_number(value: &serde_json::Number) -> String {
    if let Some(value) = value.as_i64() {
        return value.to_string();
    }
    if let Some(value) = value.as_u64() {
        return value.to_string();
    }
    let Some(value) = value.as_f64() else {
        return value.to_string();
    };
    let formatted = format!("{value:.4}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn json_value_to_text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Null => String::new(),
        _ => value.to_string(),
    }
}

fn pretty_json(raw: &str) -> String {
    serde_json::from_str::<Value>(raw)
        .and_then(|value| serde_json::to_string_pretty(&value))
        .unwrap_or_else(|_| "{}".to_string())
}

fn format_price(price_minor: Option<i64>, exponent: i64, currency: &str) -> String {
    let Some(price_minor) = price_minor else {
        return "Request quote".to_string();
    };
    let exponent = exponent.clamp(0, 6) as u32;
    let divisor = 10_i64.pow(exponent);
    if exponent == 0 {
        format!("{} {}", currency, price_minor)
    } else {
        let major = price_minor / divisor;
        let minor = price_minor.unsigned_abs() % divisor as u64;
        format!(
            "{} {}.{:0width$}",
            currency,
            major,
            minor,
            width = exponent as usize
        )
    }
}

fn format_quantity(quantity: Option<f64>, unit: &str) -> String {
    quantity
        .map(|value| format!("{value} {unit}"))
        .unwrap_or_else(|| "Not specified".to_string())
}

fn pricing_basis_display(value: &str) -> String {
    let words = value
        .trim()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .take(12)
        .map(|word| word.to_lowercase())
        .collect::<Vec<_>>();
    let normalized = words.join("_");
    match normalized.as_str() {
        "quote" | "quoted" | "request_quote" => String::new(),
        "unit" | "each" | "per_unit" | "per_each" => "unit".to_string(),
        "kg" | "kilogram" | "per_kg" | "per_kilogram" => "kg".to_string(),
        "g" | "gram" | "per_g" | "per_gram" => "g".to_string(),
        "mg" | "milligram" | "per_mg" | "per_milligram" => "mg".to_string(),
        "lb" | "pound" | "per_lb" | "per_pound" => "lb".to_string(),
        "tonne" | "metric_ton" | "per_tonne" | "per_metric_ton" => "t".to_string(),
        "l" | "liter" | "litre" | "per_l" | "per_liter" | "per_litre" => "L".to_string(),
        "ml" | "per_ml" => "mL".to_string(),
        "mol" | "mole" | "per_mol" | "per_mole" => "mol".to_string(),
        "lot" | "per_lot" => "lot".to_string(),
        "pack" | "package" | "per_pack" | "per_package" => "package".to_string(),
        _ if words.is_empty() => String::new(),
        _ if words.first().map(String::as_str) == Some("per") => words[1..].join(" "),
        _ => words.join(" "),
    }
}

pub fn material_description_excerpt(value: &str) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= MAX_DESCRIPTION_EXCERPT_CHARS {
        return normalized;
    }

    let mut excerpt = normalized
        .chars()
        .take(MAX_DESCRIPTION_EXCERPT_CHARS - 1)
        .collect::<String>();
    if let Some(word_boundary) = excerpt.rfind(' ') {
        if excerpt[..word_boundary].chars().count() >= MAX_DESCRIPTION_EXCERPT_CHARS * 3 / 4 {
            excerpt.truncate(word_boundary);
        }
    }
    excerpt.push('…');
    excerpt
}

fn build_search_text(record: &MaterialImport) -> String {
    let mut fields = vec![
        record.canonical_name.clone(),
        record.formula.clone(),
        record.mineral_family.clone(),
        record.cas_number.clone().unwrap_or_default(),
    ];
    fields.extend(record.synonyms.iter().cloned());
    if let Value::Object(identifiers) = &record.identifiers {
        fields.extend(identifiers.values().map(json_value_to_text));
    }
    fields.join(" ")
}

fn normalize_alias(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn ensure_json_object(label: &str, value: &Value) -> Result<()> {
    if !value.is_object() {
        bail!("{label} must be a JSON object");
    }
    Ok(())
}

fn json_depth(value: &Value) -> usize {
    match value {
        Value::Array(values) => 1 + values.iter().map(json_depth).max().unwrap_or(0),
        Value::Object(values) => 1 + values.values().map(json_depth).max().unwrap_or(0),
        _ => 1,
    }
}

fn validate_text(label: &str, value: &str, min: usize, max: usize) -> Result<()> {
    let length = value.trim().chars().count();
    if length < min || length > max {
        bail!("{label} must contain between {min} and {max} characters");
    }
    Ok(())
}

fn is_valid_registry_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value.chars().all(|ch| {
            ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '-' | '_')
        })
}

fn is_valid_cas_number(value: &str) -> bool {
    let parts = value.split('-').collect::<Vec<_>>();
    if parts.len() != 3
        || !(2..=7).contains(&parts[0].len())
        || parts[1].len() != 2
        || parts[2].len() != 1
        || parts
            .iter()
            .any(|part| !part.chars().all(|ch| ch.is_ascii_digit()))
    {
        return false;
    }
    let check_digit = parts[2].parse::<u32>().ok();
    let digits = format!("{}{}", parts[0], parts[1]);
    let sum = digits
        .chars()
        .rev()
        .enumerate()
        .map(|(index, ch)| ch.to_digit(10).unwrap_or(0) * (index as u32 + 1))
        .sum::<u32>();
    check_digit == Some(sum % 10)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prepare_data_root() -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("tempdir");
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        conn.execute_batch(
            r#"
            CREATE TABLE images (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                stored_name TEXT NOT NULL UNIQUE
            );
            CREATE TABLE minerals (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                slug TEXT NOT NULL UNIQUE,
                common_name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                mineral_family TEXT NOT NULL,
                formula TEXT NOT NULL,
                hardness_mohs REAL NOT NULL,
                density_g_cm3 REAL NOT NULL,
                crystal_system TEXT NOT NULL,
                color TEXT NOT NULL,
                streak TEXT NOT NULL,
                luster TEXT NOT NULL,
                major_elements_pct_json TEXT NOT NULL,
                notes TEXT NOT NULL,
                image_id INTEGER
            );
            "#,
        )
        .expect("legacy schema");
        root
    }

    fn source(url: &str) -> EvidenceImport {
        EvidenceImport {
            url: url.to_string(),
            title: "Authoritative source".to_string(),
            publisher: "Example authority".to_string(),
            license_spdx: "CC0-1.0".to_string(),
            claim_scope: "identity".to_string(),
            claim: json!({"name": "Quartz"}),
            confidence: 0.95,
            review_status: "reviewed".to_string(),
            retrieved_at: "2026-08-15T09:00:00Z".to_string(),
            ..EvidenceImport::default()
        }
    }

    fn draft_material(slug: &str, name: &str) -> MaterialImport {
        MaterialImport {
            slug: slug.to_string(),
            canonical_name: name.to_string(),
            verification_status: "draft".to_string(),
            ..MaterialImport::default()
        }
    }

    fn approve_all_pending(data_root: &Path) {
        loop {
            let page = list_pending_mineral_reviews(data_root, 100, 0)
                .expect("list pending mineral reviews");
            if page.items.is_empty() {
                break;
            }
            for item in page.items {
                approve_mineral_review(data_root, item.review_id, "Approved by registry test")
                    .expect("approve mineral review");
            }
        }
    }

    fn pending_review(data_root: &Path, slug: &str) -> PendingMineralReview {
        list_pending_mineral_reviews(data_root, 100, 0)
            .expect("list pending mineral reviews")
            .items
            .into_iter()
            .find(|item| item.record.slug == slug)
            .expect("pending mineral review")
    }

    fn bulk_item(source_record_id: &str, slug: &str, name: &str) -> MineralIngestionItem {
        MineralIngestionItem {
            source_record_id: source_record_id.to_string(),
            source_locator: Some(format!("row:{source_record_id}")),
            slug: slug.to_string(),
            canonical_name: name.to_string(),
            formula: "SiO2".to_string(),
            nomenclature_status: "approved".to_string(),
            is_valid_species: true,
            official_identifiers: BTreeMap::from([(
                "ima_number".to_string(),
                source_record_id.to_string(),
            )]),
            synonyms: Vec::new(),
            official_facts: MineralOfficialFacts::default(),
        }
    }

    fn create_only_item(source_record_id: &str, slug: &str, name: &str) -> MineralIngestionItem {
        let mut item = bulk_item(source_record_id, slug, name);
        item.official_identifiers.clear();
        item
    }

    fn bulk_manifest(
        items: &[MineralIngestionItem],
        policy: MineralIngestionPolicy,
        base_batch_id: Option<String>,
    ) -> MineralDatasetManifest {
        MineralDatasetManifest {
            schema_version: MINERAL_INGESTION_SCHEMA_VERSION,
            dataset: MineralDatasetDescriptor {
                key: "ima.test".to_string(),
                title: "IMA test list".to_string(),
            },
            source: MineralSourceDescriptor {
                key: "ima".to_string(),
                url: "https://example.org/ima".to_string(),
                license_spdx: "CC-BY-4.0".to_string(),
                attribution: Some(MineralSourceAttribution {
                    attribution_party: "Example Mineral Authority".to_string(),
                    work_title: "IMA test list".to_string(),
                    work_url: "https://example.org/ima/list.csv".to_string(),
                    license_url: "https://creativecommons.org/licenses/by/4.0/".to_string(),
                    changes_notice:
                        "Waajacu extracted structured identity fields and normalized formatting."
                            .to_string(),
                    no_endorsement_notice:
                        "Example Mineral Authority does not endorse this adaptation.".to_string(),
                    derived_output_license_spdx: "CC-BY-4.0".to_string(),
                }),
            },
            release: MineralReleaseDescriptor {
                version: format!("test-{}", items[0].canonical_name),
                released_at: "2026-08-15".to_string(),
            },
            retrieval: MineralRetrievalDescriptor {
                retrieved_at: "2026-08-15T10:00:00Z".to_string(),
            },
            artifact: MineralArtifactDescriptor {
                url: "https://example.org/ima/list.csv".to_string(),
                sha256: format!("sha256:{}", "1".repeat(64)),
            },
            parser: MineralParserDescriptor {
                name: "test-parser".to_string(),
                version: "1.0.0".to_string(),
                code_revision: "test-revision".to_string(),
                configuration_sha256: format!("sha256:{}", "2".repeat(64)),
            },
            policy,
            expected_record_count: items.len(),
            expected_chunk_count: 1,
            records_sha256: canonical_mineral_records_hash(items).expect("records hash"),
            snapshot_kind: MineralSnapshotKind::Complete,
            base_batch_id,
        }
    }

    #[test]
    fn bulk_ingestion_chunk_retry_finalize_and_activation_are_idempotent() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        registry_is_ready(root.path()).expect("registry ready");
        let items = vec![bulk_item(
            "IMA-2026-001",
            "mineral.bulk-quartz",
            "Bulk quartz",
        )];
        let manifest = bulk_manifest(&items, MineralIngestionPolicy::ImaIdentityV1, None);
        let batch = create_mineral_ingestion_batch(root.path(), "adapter:test", &manifest)
            .expect("create batch");
        assert_eq!(batch.status, MineralIngestionBatchStatus::Receiving);
        let chunk = MineralIngestionChunk {
            schema_version: MINERAL_INGESTION_SCHEMA_VERSION,
            chunk_index: 0,
            items: items.clone(),
        };
        let chunk_hash = canonical_mineral_chunk_hash(&chunk).expect("chunk hash");
        let first = put_mineral_ingestion_chunk(
            root.path(),
            &batch.batch_id,
            "adapter:test",
            &chunk_hash,
            &chunk,
        )
        .expect("put chunk");
        assert!(first.stored);
        let retry = put_mineral_ingestion_chunk(
            root.path(),
            &batch.batch_id,
            "adapter:test",
            &chunk_hash,
            &chunk,
        )
        .expect("retry chunk");
        assert!(!retry.stored);
        let mut conflicting_chunk = chunk.clone();
        conflicting_chunk.items[0].formula = "SiO2-x".to_string();
        let conflicting_hash =
            canonical_mineral_chunk_hash(&conflicting_chunk).expect("conflicting hash");
        let conflict = put_mineral_ingestion_chunk(
            root.path(),
            &batch.batch_id,
            "adapter:test",
            &conflicting_hash,
            &conflicting_chunk,
        )
        .expect_err("same chunk index cannot change");
        let problem = conflict
            .downcast_ref::<MineralIngestionProblem>()
            .expect("typed conflict");
        assert_eq!(problem.kind, MineralIngestionProblemKind::Conflict);
        assert_eq!(problem.code, "chunk_replay_conflict");

        let finalized =
            finalize_mineral_ingestion_batch(root.path(), &batch.batch_id, "reviewer:test")
                .expect("finalize");
        assert_eq!(finalized.status, MineralIngestionBatchStatus::Ready);
        assert_eq!(finalized.review_samples.len(), 1);
        let request = MineralBatchDecisionRequest {
            manifest_hash: finalized.manifest_hash.clone(),
            report_hash: finalized.report_hash.clone().expect("report hash"),
            base_batch_id: None,
            note: "Approved exact IMA test release".to_string(),
        };
        let approved = approve_mineral_ingestion_batch(
            root.path(),
            &batch.batch_id,
            "reviewer:test",
            &request,
        )
        .expect("approve batch");
        assert!(approved.changed);
        assert_eq!(approved.applied_create_count, 1);
        assert!(approved.backup_path.is_some());
        let retry = approve_mineral_ingestion_batch(
            root.path(),
            &batch.batch_id,
            "reviewer:test",
            &request,
        )
        .expect("retry approval");
        assert!(!retry.changed);
        let terminal = get_mineral_ingestion_batch(root.path(), &batch.batch_id)
            .expect("terminal batch detail")
            .expect("terminal batch");
        assert_eq!(terminal.received_chunk_count, 1);
        assert_eq!(terminal.received_record_count, 1);
        assert!(terminal.review_samples.is_empty());
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        let compacted: (i64, i64, i64) = conn
            .query_row(
                r#"
                SELECT
                    (SELECT COUNT(*) FROM mineral_ingestion_chunks WHERE batch_id = ?1),
                    compacted_payload_bytes,
                    (SELECT COUNT(*) FROM mineral_ingestion_events
                     WHERE batch_id = ?1 AND event_type = 'terminal_payload_compacted')
                FROM mineral_ingestion_batches WHERE batch_id = ?1
                "#,
                params![batch.batch_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("terminal compaction state");
        assert_eq!(compacted.0, 0);
        assert!(compacted.1 > 0);
        assert_eq!(compacted.2, 1);
        drop(conn);
        let detail = get_material_detail(root.path(), "mineral.bulk-quartz")
            .expect("detail")
            .expect("published mineral");
        assert!(detail.public_id.starts_with("mat_"));
        assert_eq!(detail.nomenclature_status, "approved");
        assert!(detail.is_valid_species);
        assert!(detail.registry_authoritative);
        assert_eq!(detail.evidence.len(), 1);
        assert_eq!(detail.license_spdx, "CC-BY-4.0");
        let attribution = detail.evidence[0]
            .attribution
            .as_ref()
            .expect("published attribution snapshot");
        assert_eq!(attribution.attribution_party, "Example Mineral Authority");
        assert_eq!(attribution.work_title, "IMA test list");
        assert_eq!(
            attribution.license_url,
            "https://creativecommons.org/licenses/by/4.0/"
        );
        assert_eq!(attribution.derived_output_license_spdx, "CC-BY-4.0");
        assert!(attribution.changes_notice.contains("normalized"));
        assert!(attribution
            .no_endorsement_notice
            .contains("does not endorse"));
        let serialized = serde_json::to_value(&detail).expect("serialize public detail");
        assert_eq!(
            serialized["evidence"][0]["attribution"]["attribution_party"],
            "Example Mineral Authority"
        );
        let listed = search_materials(root.path(), "Bulk quartz", Some("mineral"), 10)
            .expect("list attributed mineral");
        assert_eq!(listed.len(), 1);
        assert_eq!(
            listed[0].attribution_path.as_deref(),
            Some("/minerals/mineral.bulk-quartz#attribution")
        );
        assert_eq!(
            listed[0].attribution_license_spdx.as_deref(),
            Some("CC-BY-4.0")
        );
        let listed_json = serde_json::to_value(&listed[0]).expect("serialize attributed result");
        assert_eq!(
            listed_json["attribution_path"],
            "/minerals/mineral.bulk-quartz#attribution"
        );
        assert_eq!(listed_json["attribution_license_spdx"], "CC-BY-4.0");

        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        conn.execute(
            "UPDATE evidence_sources SET title = 'Mutated title', publisher = 'Mutated publisher', license_spdx = 'CC0-1.0'",
            [],
        )
        .expect("mutate global source row");
        drop(conn);
        let reloaded = get_material_detail(root.path(), "mineral.bulk-quartz")
            .expect("reloaded detail")
            .expect("reloaded mineral");
        let reloaded_attribution = reloaded.evidence[0]
            .attribution
            .as_ref()
            .expect("immutable attribution snapshot");
        assert_eq!(reloaded.evidence[0].publisher, "Example Mineral Authority");
        assert_eq!(reloaded_attribution.work_title, "IMA test list");
        assert_eq!(reloaded.evidence[0].license_spdx, "CC-BY-4.0");
    }

    #[test]
    fn official_facts_are_reviewed_dataset_updates_and_part_of_the_frozen_target() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");

        let first_items = vec![bulk_item(
            "IMA-2026-FACT",
            "mineral.official-facts",
            "Official facts mineral",
        )];
        let first_manifest =
            bulk_manifest(&first_items, MineralIngestionPolicy::ImaIdentityV1, None);
        let first = stage_finalize_bulk(root.path(), &first_manifest, first_items);
        approve_finalized_bulk(root.path(), &first);

        let enriched_items = vec![MineralIngestionItem {
            official_facts: MineralOfficialFacts {
                discovery_country: "Testland".to_string(),
                first_reference: "Journal of Test Mineralogy 12 (2026), 34".to_string(),
                second_reference: "Mineral Reviews 8 (2026), 90".to_string(),
                source_status: "A".to_string(),
            },
            ..bulk_item(
                "IMA-2026-FACT",
                "mineral.official-facts",
                "Official facts mineral",
            )
        }];
        let mut enriched_manifest = bulk_manifest(
            &enriched_items,
            MineralIngestionPolicy::ImaIdentityV1,
            Some(first.batch_id.clone()),
        );
        enriched_manifest.release.version = "facts-release".to_string();
        let enriched = stage_finalize_bulk(root.path(), &enriched_manifest, enriched_items.clone());
        let summary = enriched.report_summary.as_ref().expect("facts summary");
        assert_eq!(summary.update_count, 1);
        assert_eq!(summary.identity_critical_warning_count, 0);
        assert!(get_material_detail(root.path(), "mineral.official-facts")
            .expect("pre-approval detail")
            .expect("pre-approval mineral")
            .official_facts
            .is_empty());

        approve_finalized_bulk(root.path(), &enriched);
        let detail = get_material_detail(root.path(), "mineral.official-facts")
            .expect("detail")
            .expect("published facts mineral");
        assert_eq!(detail.official_facts, enriched_items[0].official_facts);
        assert_eq!(detail.nomenclature_status, "approved");
        assert_eq!(
            search_materials(root.path(), "Testland", Some("mineral"), 10)
                .expect("country search")
                .len(),
            1
        );

        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        let stored: (i64, i64) = conn
            .query_row(
                r#"
                SELECT COUNT(*), COUNT(DISTINCT source_release_id)
                FROM mineral_dataset_facts
                WHERE material_id = (SELECT id FROM materials WHERE slug = 'mineral.official-facts')
                "#,
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("stored official facts");
        assert_eq!(stored, (4, 1));
        drop(conn);

        let changed_items = vec![MineralIngestionItem {
            official_facts: MineralOfficialFacts {
                discovery_country: "Revised Testland".to_string(),
                ..enriched_items[0].official_facts.clone()
            },
            ..enriched_items[0].clone()
        }];
        let mut changed_manifest = bulk_manifest(
            &changed_items,
            MineralIngestionPolicy::ImaIdentityV1,
            Some(enriched.batch_id.clone()),
        );
        changed_manifest.release.version = "facts-release-2".to_string();
        let changed = stage_finalize_bulk(root.path(), &changed_manifest, changed_items);

        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        conn.execute(
            r#"
            UPDATE mineral_dataset_facts SET fact_value = 'Concurrent edit'
            WHERE fact_key = 'discovery_country'
              AND material_id = (SELECT id FROM materials WHERE slug = 'mineral.official-facts')
            "#,
            [],
        )
        .expect("simulate concurrent fact change");
        drop(conn);
        let error = approve_mineral_ingestion_batch(
            root.path(),
            &changed.batch_id,
            "reviewer:test",
            &MineralBatchDecisionRequest {
                manifest_hash: changed.manifest_hash.clone(),
                report_hash: changed.report_hash.clone().expect("report hash"),
                base_batch_id: changed.manifest.base_batch_id.clone(),
                note: "Must fail stale target".to_string(),
            },
        )
        .expect_err("changed official fact must invalidate frozen report");
        assert_eq!(
            error
                .downcast_ref::<MineralIngestionProblem>()
                .expect("typed stale report")
                .code,
            "stale_report"
        );
    }

    #[test]
    fn bulk_boundary_validation_is_strict_typed_and_rejects_controls_and_unknowns() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let items = vec![bulk_item(
            "IMA-2026-002",
            "mineral.strict",
            "Strict mineral",
        )];
        let mut manifest = bulk_manifest(&items, MineralIngestionPolicy::ImaIdentityV1, None);
        manifest.dataset.title = "Strict\nmineral list".to_string();
        let error = create_mineral_ingestion_batch(root.path(), "adapter:test", &manifest)
            .expect_err("control character must fail");
        let problem = error
            .downcast_ref::<MineralIngestionProblem>()
            .expect("typed invalid manifest");
        assert_eq!(problem.kind, MineralIngestionProblemKind::Invalid);
        assert_eq!(problem.code, "invalid_manifest");

        let unknown_manifest = serde_json::json!({
            "schema_version": 1,
            "chunk_index": 0,
            "items": [],
            "unexpected": true
        });
        assert!(serde_json::from_value::<MineralIngestionChunk>(unknown_manifest).is_err());
        let unknown_item = serde_json::json!({
            "source_record_id": "x",
            "source_locator": null,
            "slug": "mineral.x",
            "canonical_name": "X",
            "formula": "X",
            "nomenclature_status": "approved",
            "is_valid_species": true,
            "official_identifiers": {},
            "synonyms": [],
            "typo": "not accepted"
        });
        assert!(serde_json::from_value::<MineralIngestionItem>(unknown_item).is_err());
        let historical_item = serde_json::json!({
            "source_record_id": "historical",
            "source_locator": null,
            "slug": "mineral.historical",
            "canonical_name": "Historical",
            "formula": "X",
            "nomenclature_status": "approved",
            "is_valid_species": true,
            "official_identifiers": {},
            "synonyms": []
        });
        let historical_item: MineralIngestionItem =
            serde_json::from_value(historical_item).expect("historical item remains readable");
        assert!(historical_item.official_facts.is_empty());
        assert!(serde_json::to_value(historical_item)
            .expect("serialize historical item")
            .get("official_facts")
            .is_none());

        let mut inconsistent = bulk_item(
            "IMA-2026-STATUS",
            "mineral.inconsistent-status",
            "Inconsistent status",
        );
        inconsistent.official_facts.source_status = "D".to_string();
        assert!(validate_mineral_ingestion_item(&inconsistent)
            .expect_err("raw source status must agree with normalized status")
            .to_string()
            .contains("inconsistent"));

        let mut invalid_rights = bulk_manifest(
            &[bulk_item("IMA-2026-003", "mineral.rights", "Rights")],
            MineralIngestionPolicy::ImaIdentityV1,
            None,
        );
        invalid_rights.source.license_spdx = "NOASSERTION".to_string();
        let error = create_mineral_ingestion_batch(root.path(), "adapter:test", &invalid_rights)
            .expect_err("explicit rights required");
        assert_eq!(
            error
                .downcast_ref::<MineralIngestionProblem>()
                .expect("typed rights error")
                .code,
            "invalid_manifest"
        );

        let create_only_items = vec![bulk_item(
            "IMA-SQUAT-1",
            "mineral.create-only-squat",
            "Create-only squat",
        )];
        let create_only_manifest = bulk_manifest(
            &create_only_items,
            MineralIngestionPolicy::CreateOnlyV1,
            None,
        );
        let create_only =
            create_mineral_ingestion_batch(root.path(), "adapter:test", &create_only_manifest)
                .expect("create-only batch envelope");
        let chunk = MineralIngestionChunk {
            schema_version: MINERAL_INGESTION_SCHEMA_VERSION,
            chunk_index: 0,
            items: create_only_items,
        };
        let hash = canonical_mineral_chunk_hash(&chunk).expect("chunk hash");
        let error = put_mineral_ingestion_chunk(
            root.path(),
            &create_only.batch_id,
            "adapter:test",
            &hash,
            &chunk,
        )
        .expect_err("create-only cannot claim IMA identifiers");
        assert_eq!(
            error
                .downcast_ref::<MineralIngestionProblem>()
                .expect("typed policy error")
                .code,
            "policy_forbids_authority_identifiers"
        );
    }

    #[test]
    fn manifest_v2_attribution_is_hashed_complete_and_license_compatible() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let items = vec![bulk_item(
            "IMA-ATTRIBUTION-1",
            "mineral.attribution",
            "Attribution mineral",
        )];
        let manifest = bulk_manifest(&items, MineralIngestionPolicy::ImaIdentityV1, None);
        let original_hash = canonical_mineral_manifest_hash(&manifest).expect("manifest hash");

        let mut changed = manifest.clone();
        changed
            .source
            .attribution
            .as_mut()
            .expect("attribution")
            .changes_notice
            .push_str(" Formula typography was preserved.");
        assert_ne!(
            canonical_mineral_manifest_hash(&changed).expect("changed hash"),
            original_hash
        );

        let mut missing = manifest.clone();
        missing.source.attribution = None;
        let error = create_mineral_ingestion_batch(root.path(), "adapter:test", &missing)
            .expect_err("v2 attribution is required");
        assert_eq!(
            error
                .downcast_ref::<MineralIngestionProblem>()
                .expect("typed missing attribution")
                .code,
            "invalid_manifest"
        );

        let mut noncanonical = manifest.clone();
        noncanonical
            .source
            .attribution
            .as_mut()
            .expect("attribution")
            .license_url = "http://creativecommons.org/licenses/by/4.0/".to_string();
        assert!(
            create_mineral_ingestion_batch(root.path(), "adapter:test", &noncanonical)
                .expect_err("license URL must fail closed")
                .to_string()
                .contains("canonical HTTPS")
        );

        let mut mismatched_url = manifest.clone();
        mismatched_url
            .source
            .attribution
            .as_mut()
            .expect("attribution")
            .license_url = "https://creativecommons.org/licenses/by-sa/4.0/".to_string();
        assert!(
            create_mineral_ingestion_batch(root.path(), "adapter:test", &mismatched_url)
                .expect_err("license URL must match the source SPDX identifier")
                .to_string()
                .contains("must be the canonical URL")
        );

        let mut absent_derived_license = manifest.clone();
        absent_derived_license
            .source
            .attribution
            .as_mut()
            .expect("attribution")
            .derived_output_license_spdx = "noassertion".to_string();
        assert!(create_mineral_ingestion_batch(
            root.path(),
            "adapter:test",
            &absent_derived_license
        )
        .expect_err("derived-data license must be explicit")
        .to_string()
        .contains("requires an explicit valid SPDX license expression"));

        let mut share_alike = manifest;
        share_alike.source.license_spdx = "CC-BY-SA-3.0".to_string();
        let attribution = share_alike
            .source
            .attribution
            .as_mut()
            .expect("attribution");
        attribution.license_url = "https://creativecommons.org/licenses/by-sa/3.0/".to_string();
        attribution.derived_output_license_spdx = "CC-BY-4.0".to_string();
        assert!(
            create_mineral_ingestion_batch(root.path(), "adapter:test", &share_alike)
                .expect_err("share-alike mismatch must fail closed")
                .to_string()
                .contains("requires an explicitly compatible derived-output license")
        );
    }

    #[test]
    fn historical_v1_manifest_remains_readable_but_is_rejected_for_restage() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let items = vec![bulk_item(
            "IMA-HISTORICAL-1",
            "mineral.historical-v1",
            "Historical v1",
        )];
        let mut manifest = bulk_manifest(&items, MineralIngestionPolicy::ImaIdentityV1, None);
        manifest.schema_version = 1;
        manifest.source.attribution = None;
        let manifest_hash = canonical_mineral_manifest_hash(&manifest).expect("v1 hash");
        let batch_id = format!("batch_{}", &manifest_hash[7..]);
        let manifest_json =
            String::from_utf8(canonical_json_bytes(&manifest).expect("v1 canonical JSON"))
                .expect("UTF-8 manifest");
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        conn.execute(
            r#"
            INSERT INTO mineral_ingestion_batches(
                batch_id, manifest_hash, manifest_json, dataset_key, source_key,
                release_version, artifact_sha256, parser_name, parser_version,
                parser_code_revision, parser_configuration_sha256, policy,
                snapshot_kind, expected_record_count, expected_chunk_count,
                expected_records_sha256, base_batch_id, status
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                      'ima_identity_v1', 'complete', 1, 1, ?12, NULL, 'receiving')
            "#,
            params![
                batch_id,
                manifest_hash,
                manifest_json,
                manifest.dataset.key,
                manifest.source.key,
                manifest.release.version,
                manifest.artifact.sha256,
                manifest.parser.name,
                manifest.parser.version,
                manifest.parser.code_revision,
                manifest.parser.configuration_sha256,
                manifest.records_sha256,
            ],
        )
        .expect("insert historical staged batch");
        drop(conn);

        let before = get_mineral_ingestion_batch(root.path(), &batch_id)
            .expect("read historical staged batch")
            .expect("historical staged batch");
        assert_eq!(before.manifest.schema_version, 1);
        assert!(before.manifest.source.attribution.is_none());
        let error = finalize_mineral_ingestion_batch(root.path(), &batch_id, "reviewer:test")
            .expect_err("v1 finalization must fail");
        assert_eq!(
            error
                .downcast_ref::<MineralIngestionProblem>()
                .expect("typed v1 refusal")
                .code,
            "manifest_requires_restage"
        );

        let historical_report_hash = format!("sha256:{}", "9".repeat(64));
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        conn.execute(
            "UPDATE mineral_ingestion_batches SET status = 'ready', report_hash = ?1 WHERE batch_id = ?2",
            params![historical_report_hash, batch_id],
        )
        .expect("simulate a pre-migration finalized v1 batch");
        drop(conn);
        let repeated_finalize_error =
            finalize_mineral_ingestion_batch(root.path(), &batch_id, "reviewer:test")
                .expect_err("a pre-migration finalized v1 batch must still be restaged");
        assert_eq!(
            repeated_finalize_error
                .downcast_ref::<MineralIngestionProblem>()
                .expect("typed repeated v1 refusal")
                .code,
            "manifest_requires_restage"
        );
        let approval_error = approve_mineral_ingestion_batch(
            root.path(),
            &batch_id,
            "reviewer:test",
            &MineralBatchDecisionRequest {
                manifest_hash: manifest_hash.clone(),
                report_hash: historical_report_hash,
                base_batch_id: None,
                note: "Must not approve legacy attribution".to_string(),
            },
        )
        .expect_err("v1 approval must fail before report activation");
        assert_eq!(
            approval_error
                .downcast_ref::<MineralIngestionProblem>()
                .expect("typed v1 approval refusal")
                .code,
            "manifest_requires_restage"
        );

        init_registry_database(root.path()).expect("v2 migration restage gate");
        let terminal = get_mineral_ingestion_batch(root.path(), &batch_id)
            .expect("read historical terminal batch")
            .expect("historical terminal batch");
        assert_eq!(terminal.status, MineralIngestionBatchStatus::Rejected);
        assert_eq!(terminal.manifest_hash, manifest_hash);
        assert_eq!(terminal.manifest.schema_version, 1);
        assert!(terminal.manifest.source.attribution.is_none());
        assert_eq!(
            terminal.decision_actor.as_deref(),
            Some("system:attribution-v2-migration")
        );
        assert_eq!(
            terminal.decision_note,
            "schema_v1_requires_attributed_restage"
        );
        registry_is_ready(root.path()).expect("attribution migration readiness");
    }

    fn stage_finalize_bulk(
        data_root: &Path,
        manifest: &MineralDatasetManifest,
        items: Vec<MineralIngestionItem>,
    ) -> MineralIngestionBatchDetail {
        let batch = create_mineral_ingestion_batch(data_root, "adapter:test", manifest)
            .expect("create bulk batch");
        for (chunk_index, records) in items.chunks(MAX_MINERAL_INGESTION_CHUNK_ITEMS).enumerate() {
            let chunk = MineralIngestionChunk {
                schema_version: MINERAL_INGESTION_SCHEMA_VERSION,
                chunk_index,
                items: records.to_vec(),
            };
            let hash = canonical_mineral_chunk_hash(&chunk).expect("chunk hash");
            put_mineral_ingestion_chunk(data_root, &batch.batch_id, "adapter:test", &hash, &chunk)
                .expect("put bulk chunk");
        }
        finalize_mineral_ingestion_batch(data_root, &batch.batch_id, "reviewer:test")
            .expect("finalize bulk batch")
    }

    fn approve_finalized_bulk(
        data_root: &Path,
        batch: &MineralIngestionBatchDetail,
    ) -> MineralBatchDecisionOutcome {
        approve_mineral_ingestion_batch(
            data_root,
            &batch.batch_id,
            "reviewer:test",
            &MineralBatchDecisionRequest {
                manifest_hash: batch.manifest_hash.clone(),
                report_hash: batch.report_hash.clone().expect("report hash"),
                base_batch_id: batch.manifest.base_batch_id.clone(),
                note: "Approved exact test release".to_string(),
            },
        )
        .expect("approve finalized bulk batch")
    }

    #[test]
    fn ima_updates_preserve_enrichment_history_and_pending_reviews_but_retire_critical_offers() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let curated = MaterialImport {
            slug: "mineral.bulk-cobalt".to_string(),
            canonical_name: "Bulk cobalt".to_string(),
            formula: "Co".to_string(),
            description: "Curated description that the identity source does not own.".to_string(),
            mineral_family: "Elements".to_string(),
            properties: json!({"hardness_mohs": 5.0}),
            safety: json!({"handling": "curated precautions"}),
            synonyms: vec!["Curator alias".to_string()],
            verification_status: "sourced".to_string(),
            sources: vec![source("https://curator.example/cobalt")],
            ..MaterialImport::default()
        };
        import_material_batch(root.path(), "curator", &[curated]).expect("curated import");
        approve_all_pending(root.path());
        import_provider(
            root.path(),
            &ProviderImport {
                slug: "provider.bulk".to_string(),
                name: "Bulk provider".to_string(),
                website_url: "https://provider.example".to_string(),
                offers: vec![OfferImport {
                    material_slug: "mineral.bulk-cobalt".to_string(),
                    external_id: "bulk-cobalt".to_string(),
                    title: "Bulk cobalt specimen".to_string(),
                    product_url: "https://provider.example/cobalt".to_string(),
                    ..OfferImport::default()
                }],
                ..ProviderImport::default()
            },
        )
        .expect("provider import");

        let first_items = vec![MineralIngestionItem {
            formula: "Co".to_string(),
            synonyms: vec!["Official alias".to_string()],
            ..bulk_item("IMA-2026-CO", "mineral.bulk-cobalt", "Bulk cobalt")
        }];
        let first_manifest =
            bulk_manifest(&first_items, MineralIngestionPolicy::ImaIdentityV1, None);
        let first = stage_finalize_bulk(root.path(), &first_manifest, first_items);
        assert_eq!(
            first.report_summary.as_ref().expect("summary").adopt_count,
            1
        );
        approve_finalized_bulk(root.path(), &first);
        let adopted = get_material_detail(root.path(), "mineral.bulk-cobalt")
            .expect("detail")
            .expect("adopted mineral");
        assert_eq!(
            adopted.description,
            "Curated description that the identity source does not own."
        );
        assert_eq!(adopted.properties[0].value, "5");
        assert_eq!(adopted.safety[0].value, "curated precautions");
        assert_eq!(
            adopted.offers.len(),
            1,
            "ordinary adoption preserves offers"
        );
        assert_eq!(
            adopted.evidence.len(),
            2,
            "curator and dataset evidence coexist"
        );

        let pending = MaterialImport {
            slug: "mineral.bulk-cobalt".to_string(),
            canonical_name: "A pending curator correction".to_string(),
            verification_status: "draft".to_string(),
            ..MaterialImport::default()
        };
        import_material_batch(root.path(), "curator-pending", &[pending])
            .expect("pending individual review");

        let second_items = vec![MineralIngestionItem {
            source_record_id: "IMA-2026-CO".to_string(),
            source_locator: Some("row:changed".to_string()),
            slug: "mineral.proposed-new-route".to_string(),
            canonical_name: "Bulk carbon monoxide".to_string(),
            formula: "CO".to_string(),
            nomenclature_status: "discredited".to_string(),
            is_valid_species: false,
            official_identifiers: BTreeMap::from([(
                "ima_number".to_string(),
                "IMA-2026-CO".to_string(),
            )]),
            synonyms: vec!["Former official alias".to_string()],
            official_facts: MineralOfficialFacts::default(),
        }];
        let mut second_manifest = bulk_manifest(
            &second_items,
            MineralIngestionPolicy::ImaIdentityV1,
            Some(first.batch_id.clone()),
        );
        second_manifest.release.version = "second-release".to_string();
        let second = stage_finalize_bulk(root.path(), &second_manifest, second_items);
        let summary = second.report_summary.as_ref().expect("second summary");
        assert_eq!(summary.update_count, 1);
        assert_eq!(summary.identity_critical_warning_count, 1);
        assert!(second.anomaly_samples[0].critical_formula_change);
        assert!(second.anomaly_samples[0].critical_validity_change);
        let outcome = approve_finalized_bulk(root.path(), &second);
        assert_eq!(outcome.retired_offer_count, 1);

        assert!(
            get_material_detail(root.path(), "mineral.bulk-cobalt")
                .expect("detail")
                .is_none(),
            "discredited minerals are hidden from the public detail API"
        );
        assert!(
            get_material_detail(root.path(), "mineral.proposed-new-route")
                .expect("new route lookup")
                .is_none()
        );
        assert!(
            search_materials(root.path(), "Bulk carbon monoxide", Some("mineral"), 10)
                .expect("invalid species search")
                .is_empty()
        );
        assert_eq!(
            pending_review(root.path(), "mineral.bulk-cobalt")
                .record
                .canonical_name,
            "A pending curator correction"
        );
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        let preserved: (String, String, String, i64, i64) = conn
            .query_row(
                r#"
                SELECT canonical_name, formula, description,
                       (SELECT COUNT(*) FROM offers o WHERE o.material_id = m.id AND o.active = 1),
                       (SELECT COUNT(*) FROM material_evidence me WHERE me.material_id = m.id)
                FROM materials m WHERE slug = 'mineral.bulk-cobalt'
                "#,
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .expect("inspect hidden historical material");
        assert_eq!(preserved.0, "Bulk carbon monoxide");
        assert_eq!(preserved.1, "CO");
        assert_eq!(
            preserved.2,
            "Curated description that the identity source does not own."
        );
        assert_eq!(preserved.3, 0);
        assert_eq!(preserved.4, 2);
        let former_name: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM material_aliases WHERE material_id = (SELECT id FROM materials WHERE slug = 'mineral.bulk-cobalt') AND alias = 'Bulk cobalt' AND origin = 'bulk_history'",
                [],
                |row| row.get(0),
            )
            .expect("former name count");
        assert_eq!(former_name, 1);
    }

    #[test]
    fn approval_rejects_a_frozen_report_after_curator_target_drift() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let initial_items = vec![bulk_item(
            "IMA-STALE-1",
            "mineral.stale-target",
            "Stale target",
        )];
        let initial_manifest =
            bulk_manifest(&initial_items, MineralIngestionPolicy::ImaIdentityV1, None);
        let initial = stage_finalize_bulk(root.path(), &initial_manifest, initial_items);
        approve_finalized_bulk(root.path(), &initial);

        let mut changed_items = vec![bulk_item(
            "IMA-STALE-1",
            "mineral.stale-target",
            "Renamed target",
        )];
        changed_items[0].formula = "Si2O4".to_string();
        let mut changed_manifest = bulk_manifest(
            &changed_items,
            MineralIngestionPolicy::ImaIdentityV1,
            Some(initial.batch_id.clone()),
        );
        changed_manifest.release.version = "stale-second".to_string();
        let finalized = stage_finalize_bulk(root.path(), &changed_manifest, changed_items);
        assert_eq!(finalized.status, MineralIngestionBatchStatus::Ready);

        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        conn.execute(
            r#"
            UPDATE materials
            SET description = 'curator changed this after finalization',
                cas_number = '14808-60-7'
            WHERE slug = 'mineral.stale-target'
            "#,
            [],
        )
        .expect("curator drift");
        drop(conn);
        let error = approve_mineral_ingestion_batch(
            root.path(),
            &finalized.batch_id,
            "reviewer:test",
            &MineralBatchDecisionRequest {
                manifest_hash: finalized.manifest_hash.clone(),
                report_hash: finalized.report_hash.clone().expect("report hash"),
                base_batch_id: finalized.manifest.base_batch_id.clone(),
                note: "now stale".to_string(),
            },
        )
        .expect_err("stale target must block activation");
        let problem = error
            .downcast_ref::<MineralIngestionProblem>()
            .expect("typed stale report");
        assert_eq!(problem.kind, MineralIngestionProblemKind::Conflict);
        assert_eq!(problem.code, "stale_report");
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        let state: (String, String, i64) = conn
            .query_row(
                r#"
                SELECT status,
                       (SELECT canonical_name FROM materials WHERE slug = 'mineral.stale-target'),
                       (SELECT COUNT(*) FROM mineral_ingestion_backups WHERE batch_id = ?1)
                FROM mineral_ingestion_batches WHERE batch_id = ?1
                "#,
                params![finalized.batch_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("stale batch state");
        assert_eq!(state, ("ready".to_string(), "Stale target".to_string(), 0));
    }

    #[test]
    fn critical_approval_is_stale_when_an_offer_arrives_after_finalization() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let initial_items = vec![bulk_item(
            "IMA-OFFER-RACE",
            "mineral.offer-race",
            "Offer race",
        )];
        let initial_manifest =
            bulk_manifest(&initial_items, MineralIngestionPolicy::ImaIdentityV1, None);
        let initial = stage_finalize_bulk(root.path(), &initial_manifest, initial_items);
        approve_finalized_bulk(root.path(), &initial);

        let mut changed_items = vec![bulk_item(
            "IMA-OFFER-RACE",
            "mineral.offer-race",
            "Offer race",
        )];
        changed_items[0].formula = "CO".to_string();
        let mut changed_manifest = bulk_manifest(
            &changed_items,
            MineralIngestionPolicy::ImaIdentityV1,
            Some(initial.batch_id.clone()),
        );
        changed_manifest.release.version = "offer-race-second".to_string();
        let finalized = stage_finalize_bulk(root.path(), &changed_manifest, changed_items);
        assert!(finalized.anomaly_samples[0].critical_formula_change);
        import_provider(
            root.path(),
            &ProviderImport {
                slug: "provider.offer-race".to_string(),
                name: "Offer race provider".to_string(),
                website_url: "https://provider.example".to_string(),
                offers: vec![OfferImport {
                    material_slug: "mineral.offer-race".to_string(),
                    external_id: "late-offer".to_string(),
                    title: "Late offer".to_string(),
                    product_url: "https://provider.example/late".to_string(),
                    ..OfferImport::default()
                }],
                ..ProviderImport::default()
            },
        )
        .expect("late provider offer");
        let error = approve_mineral_ingestion_batch(
            root.path(),
            &finalized.batch_id,
            "reviewer:test",
            &MineralBatchDecisionRequest {
                manifest_hash: finalized.manifest_hash.clone(),
                report_hash: finalized.report_hash.clone().expect("report hash"),
                base_batch_id: finalized.manifest.base_batch_id.clone(),
                note: "must be restaged".to_string(),
            },
        )
        .expect_err("late offer changes retirement scope");
        assert_eq!(
            error
                .downcast_ref::<MineralIngestionProblem>()
                .expect("typed stale report")
                .code,
            "stale_report"
        );
        assert_eq!(
            offers_for_material(root.path(), "mineral.offer-race")
                .expect("offers")
                .expect("public mineral")
                .len(),
            1
        );
    }

    #[test]
    fn approval_is_stale_when_dataset_owned_side_state_drifts() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let mut initial_items = vec![bulk_item("IMA-SIDE-RACE", "mineral.side-race", "Side race")];
        initial_items[0].synonyms = vec!["Official side alias".to_string()];
        let initial_manifest =
            bulk_manifest(&initial_items, MineralIngestionPolicy::ImaIdentityV1, None);
        let initial = stage_finalize_bulk(root.path(), &initial_manifest, initial_items.clone());
        approve_finalized_bulk(root.path(), &initial);

        initial_items[0].canonical_name = "Side race renamed".to_string();
        let mut second_manifest = bulk_manifest(
            &initial_items,
            MineralIngestionPolicy::ImaIdentityV1,
            Some(initial.batch_id.clone()),
        );
        second_manifest.release.version = "side-race-second".to_string();
        let finalized = stage_finalize_bulk(root.path(), &second_manifest, initial_items);
        assert_eq!(
            finalized
                .report_summary
                .as_ref()
                .expect("summary")
                .update_count,
            1
        );
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        conn.execute(
            r#"
            UPDATE material_aliases
            SET source_release_id = 'externally-mutated-release'
            WHERE dataset_key = 'ima.test' AND origin = 'bulk_dataset'
            "#,
            [],
        )
        .expect("mutate owned alias provenance");
        drop(conn);
        let error = approve_mineral_ingestion_batch(
            root.path(),
            &finalized.batch_id,
            "reviewer:test",
            &MineralBatchDecisionRequest {
                manifest_hash: finalized.manifest_hash.clone(),
                report_hash: finalized.report_hash.clone().expect("report hash"),
                base_batch_id: finalized.manifest.base_batch_id.clone(),
                note: "must be restaged".to_string(),
            },
        )
        .expect_err("owned side-state drift must block activation");
        assert_eq!(
            error
                .downcast_ref::<MineralIngestionProblem>()
                .expect("typed stale report")
                .code,
            "stale_report"
        );
    }

    #[test]
    fn unchanged_release_refreshes_only_dataset_provenance() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let mut items = vec![bulk_item(
            "IMA-UNCHANGED-1",
            "mineral.unchanged",
            "Unchanged mineral",
        )];
        items[0].synonyms = vec!["Official unchanged synonym".to_string()];
        let manifest = bulk_manifest(&items, MineralIngestionPolicy::ImaIdentityV1, None);
        let first = stage_finalize_bulk(root.path(), &manifest, items.clone());
        approve_finalized_bulk(root.path(), &first);
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        conn.execute(
            r#"
            UPDATE materials
            SET description = 'curator-owned enrichment',
                properties_json = '{"hardness_mohs":7}',
                updated_at = '2001-01-01 00:00:00'
            WHERE slug = 'mineral.unchanged'
            "#,
            [],
        )
        .expect("curator enrichment");
        conn.execute(
            r#"
            INSERT INTO material_aliases(
                material_id, alias, alias_normalized, alias_type, origin
            ) SELECT id, 'Curator synonym', 'curator synonym', 'synonym', 'curator'
              FROM materials WHERE slug = 'mineral.unchanged'
            "#,
            [],
        )
        .expect("curator alias");
        drop(conn);

        items[0].source_locator = Some("row:new-release".to_string());
        let mut second_manifest = bulk_manifest(
            &items,
            MineralIngestionPolicy::ImaIdentityV1,
            Some(first.batch_id.clone()),
        );
        second_manifest.release.version = "unchanged-second".to_string();
        second_manifest.artifact.sha256 = format!("sha256:{}", "3".repeat(64));
        let second = stage_finalize_bulk(root.path(), &second_manifest, items);
        assert_eq!(
            second
                .report_summary
                .as_ref()
                .expect("summary")
                .unchanged_count,
            1
        );
        approve_finalized_bulk(root.path(), &second);

        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        let material: (String, String, String) = conn
            .query_row(
                "SELECT description, properties_json, updated_at FROM materials WHERE slug = 'mineral.unchanged'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("material enrichment");
        assert_eq!(material.0, "curator-owned enrichment");
        assert_eq!(material.1, r#"{"hardness_mohs":7}"#);
        assert_eq!(material.2, "2001-01-01 00:00:00");
        for sql in [
            "SELECT source_release_id FROM mineral_dataset_identifiers WHERE dataset_key = 'ima.test'",
            "SELECT source_release_id FROM material_aliases WHERE origin = 'bulk_dataset' AND dataset_key = 'ima.test'",
            "SELECT source_release_id FROM material_evidence WHERE dataset_key = 'ima.test'",
        ] {
            let release: String = conn.query_row(sql, [], |row| row.get(0)).expect("release id");
            assert_eq!(release, second.batch_id);
        }
        let curator_aliases: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM material_aliases WHERE alias = 'Curator synonym' AND origin = 'curator'",
                [],
                |row| row.get(0),
            )
            .expect("curator alias count");
        assert_eq!(curator_aliases, 1);
    }

    #[test]
    fn ima_identity_policy_is_bound_to_one_reviewed_dataset_and_source() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let items = vec![bulk_item(
            "IMA-AUTHORITY-1",
            "mineral.authority-one",
            "Authority one",
        )];
        let manifest = bulk_manifest(&items, MineralIngestionPolicy::ImaIdentityV1, None);
        let first = stage_finalize_bulk(root.path(), &manifest, items);

        let rogue_items = vec![bulk_item(
            "IMA-AUTHORITY-2",
            "mineral.authority-two",
            "Authority two",
        )];
        let mut rogue_manifest =
            bulk_manifest(&rogue_items, MineralIngestionPolicy::ImaIdentityV1, None);
        rogue_manifest.dataset.key = "rogue.ima".to_string();
        rogue_manifest.dataset.title = "Unreviewed mirror".to_string();
        rogue_manifest.source.key = "rogue".to_string();
        rogue_manifest.release.version = "rogue-release".to_string();
        let rogue = stage_finalize_bulk(root.path(), &rogue_manifest, rogue_items);
        assert_eq!(rogue.status, MineralIngestionBatchStatus::Ready);
        approve_finalized_bulk(root.path(), &first);
        let error = approve_mineral_ingestion_batch(
            root.path(),
            &rogue.batch_id,
            "reviewer:test",
            &MineralBatchDecisionRequest {
                manifest_hash: rogue.manifest_hash.clone(),
                report_hash: rogue.report_hash.clone().expect("rogue report hash"),
                base_batch_id: None,
                note: "attempted takeover".to_string(),
            },
        )
        .expect_err("second source cannot take over IMA identity policy");
        let problem = error
            .downcast_ref::<MineralIngestionProblem>()
            .expect("typed authority conflict");
        assert_eq!(problem.kind, MineralIngestionProblemKind::Conflict);
        assert_eq!(problem.code, "authority_binding_conflict");
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        let binding: (String, String, String) = conn
            .query_row(
                "SELECT dataset_key, source_key, bound_batch_id FROM mineral_ingestion_authorities WHERE policy = 'ima_identity_v1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("authority binding");
        assert_eq!(
            binding,
            ("ima.test".to_string(), "ima".to_string(), first.batch_id)
        );
        let rogue_state: (String, i64) = conn
            .query_row(
                r#"
                SELECT status,
                       (SELECT COUNT(*) FROM mineral_ingestion_backups WHERE batch_id = ?1)
                FROM mineral_ingestion_batches WHERE batch_id = ?1
                "#,
                params![rogue.batch_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("rogue batch state");
        assert_eq!(rogue_state, ("ready".to_string(), 0));
    }

    #[test]
    fn quarantine_limits_are_exact_and_abandoned_payloads_are_reclaimed() {
        let limits = MineralIngestionLimits {
            batch_max_bytes: 100,
            quarantine_max_bytes: 150,
            abandoned_hours: 24,
        };
        enforce_quarantine_limits(40, 90, 60, limits).expect("exact limits accepted");
        let error =
            enforce_quarantine_limits(41, 90, 60, limits).expect_err("batch overflow rejected");
        assert_eq!(
            error
                .downcast_ref::<MineralIngestionProblem>()
                .expect("typed batch quota")
                .code,
            "batch_quota_exceeded"
        );
        let error =
            enforce_quarantine_limits(40, 91, 60, limits).expect_err("global overflow rejected");
        assert_eq!(
            error
                .downcast_ref::<MineralIngestionProblem>()
                .expect("typed global quota")
                .code,
            "quarantine_quota_exceeded"
        );

        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let old_items = vec![create_only_item(
            "IMA-EXPIRE-1",
            "mineral.expired-batch",
            "Expired batch",
        )];
        let old_manifest = bulk_manifest(&old_items, MineralIngestionPolicy::CreateOnlyV1, None);
        let old = create_mineral_ingestion_batch(root.path(), "adapter:test", &old_manifest)
            .expect("old batch");
        let chunk = MineralIngestionChunk {
            schema_version: MINERAL_INGESTION_SCHEMA_VERSION,
            chunk_index: 0,
            items: old_items,
        };
        let hash = canonical_mineral_chunk_hash(&chunk).expect("chunk hash");
        put_mineral_ingestion_chunk(root.path(), &old.batch_id, "adapter:test", &hash, &chunk)
            .expect("old chunk");
        let recent_items = vec![create_only_item(
            "IMA-RECENT-1",
            "mineral.recent-batch",
            "Recent batch",
        )];
        let recent_manifest =
            bulk_manifest(&recent_items, MineralIngestionPolicy::CreateOnlyV1, None);
        let recent = create_mineral_ingestion_batch(root.path(), "adapter:test", &recent_manifest)
            .expect("recent batch");

        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        conn.execute_batch("DROP TRIGGER mineral_ingestion_chunks_immutable_update;")
            .expect("temporarily remove update guard");
        conn.execute(
            "UPDATE mineral_ingestion_batches SET created_at = '2000-01-01 00:00:00' WHERE batch_id = ?1",
            params![old.batch_id],
        )
        .expect("age old batch");
        conn.execute(
            "UPDATE mineral_ingestion_chunks SET created_at = '2000-01-01 00:00:00' WHERE batch_id = ?1",
            params![old.batch_id],
        )
        .expect("age old chunk");
        drop(conn);
        init_registry_database(root.path()).expect("restore immutable guards");
        assert_eq!(
            expire_abandoned_mineral_ingestion_batches(root.path(), "operator:test", 24)
                .expect("expire abandoned batch"),
            1
        );
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        let expired: (String, String, i64, i64) = conn
            .query_row(
                r#"
                SELECT status, decision_note,
                       (SELECT COUNT(*) FROM mineral_ingestion_chunks WHERE batch_id = ?1),
                       (SELECT COUNT(*) FROM mineral_ingestion_items WHERE batch_id = ?1)
                FROM mineral_ingestion_batches WHERE batch_id = ?1
                "#,
                params![old.batch_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("expired tombstone");
        assert_eq!(
            expired,
            (
                "rejected".to_string(),
                "expired_abandoned_batch".to_string(),
                0,
                0
            )
        );
        let recent_status: String = conn
            .query_row(
                "SELECT status FROM mineral_ingestion_batches WHERE batch_id = ?1",
                params![recent.batch_id],
                |row| row.get(0),
            )
            .expect("recent status");
        assert_eq!(recent_status, "receiving");
    }

    #[test]
    fn registry_upgrade_repairs_legacy_insert_public_id_trigger() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        conn.execute_batch(
            r#"
            DROP TRIGGER minerals_registry_insert;
            CREATE TRIGGER minerals_registry_insert AFTER INSERT ON minerals BEGIN
                INSERT OR IGNORE INTO materials(
                    source_mineral_id, slug, record_type, canonical_name
                ) VALUES (new.id, new.slug, 'mineral', new.common_name);
            END;
            "#,
        )
        .expect("install legacy trigger");
        drop(conn);
        init_registry_database(root.path()).expect("repair trigger migration");
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        conn.execute(
            r#"
            INSERT INTO minerals(
                slug, common_name, description, mineral_family, formula,
                hardness_mohs, density_g_cm3, crystal_system, color, streak,
                luster, major_elements_pct_json, notes
            ) VALUES (
                'mineral.future-legacy', 'Future legacy', '', '', 'X',
                1.0, 1.0, 'unknown', '', '', '', '{}', ''
            )
            "#,
            [],
        )
        .expect("future legacy insert");
        let public_id: String = conn
            .query_row(
                "SELECT public_id FROM materials WHERE slug = 'mineral.future-legacy'",
                [],
                |row| row.get(0),
            )
            .expect("future public id");
        assert!(public_id.starts_with("mat_"));
        assert_eq!(public_id.len(), 36);
        drop(conn);
        registry_is_ready(root.path()).expect("registry ready after trigger repair");
    }

    #[test]
    fn preactivation_backup_keeps_the_writer_reservation() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let backups = root.path().join("backups");
        fs::create_dir_all(&backups).expect("backup directory");
        let mut writer = open_connection(root.path(), true).expect("writer");
        let tx = writer
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("writer reservation");
        let source = open_connection(root.path(), false).expect("backup source");
        let backup = create_pre_activation_backup(
            &source,
            &backups,
            "batch_12345678901234567890",
            &format!("sha256:{}", "a".repeat(64)),
        )
        .expect("backup under writer reservation");
        let contender = Connection::open(root.path().join(DATABASE_FILE)).expect("contender");
        contender
            .busy_timeout(Duration::from_millis(20))
            .expect("short busy timeout");
        let error = contender
            .execute(
                "INSERT INTO providers(slug, name, website_url) VALUES ('blocked', 'Blocked', 'https://example.org')",
                [],
            )
            .expect_err("unrelated writer must remain blocked");
        match error {
            rusqlite::Error::SqliteFailure(details, _) => assert!(matches!(
                details.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            )),
            other => panic!("unexpected lock error: {other}"),
        }
        drop(backup);
        drop(source);
        tx.rollback().expect("rollback writer reservation");
    }

    #[test]
    fn create_only_replay_and_withdrawn_route_are_reported_as_conflicts() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let items = vec![create_only_item(
            "IMA-CREATE-1",
            "mineral.create-only",
            "Create only mineral",
        )];
        let first_manifest = bulk_manifest(&items, MineralIngestionPolicy::CreateOnlyV1, None);
        let first = stage_finalize_bulk(root.path(), &first_manifest, items.clone());
        assert_eq!(first.status, MineralIngestionBatchStatus::Ready);
        approve_finalized_bulk(root.path(), &first);
        assert_eq!(
            get_material_detail(root.path(), "mineral.create-only")
                .expect("detail")
                .expect("created mineral")
                .verification_status,
            "draft"
        );

        let mut replay_manifest = bulk_manifest(
            &items,
            MineralIngestionPolicy::CreateOnlyV1,
            Some(first.batch_id.clone()),
        );
        replay_manifest.release.version = "create-only-replay".to_string();
        let replay = stage_finalize_bulk(root.path(), &replay_manifest, items);
        assert_eq!(replay.status, MineralIngestionBatchStatus::NeedsAttention);
        assert_eq!(
            replay
                .report_summary
                .as_ref()
                .expect("replay summary")
                .conflict_count,
            1
        );
        assert_eq!(
            replay.anomaly_samples[0].code,
            "create_only_existing_identity"
        );
        let reject = MineralBatchDecisionRequest {
            manifest_hash: replay.manifest_hash.clone(),
            report_hash: replay.report_hash.clone().expect("report hash"),
            base_batch_id: replay.manifest.base_batch_id.clone(),
            note: "Rejected create-only replay".to_string(),
        };
        let rejected =
            reject_mineral_ingestion_batch(root.path(), &replay.batch_id, "reviewer:test", &reject)
                .expect("reject replay");
        assert!(rejected.changed);
        assert!(
            !reject_mineral_ingestion_batch(
                root.path(),
                &replay.batch_id,
                "reviewer:test",
                &reject,
            )
            .expect("retry rejection")
            .changed
        );

        let withdrawn = MaterialImport {
            slug: "mineral.withdrawn-collision".to_string(),
            canonical_name: "Withdrawn collision".to_string(),
            verification_status: "draft".to_string(),
            ..MaterialImport::default()
        };
        import_material_batch(root.path(), "curator", &[withdrawn]).expect("curator import");
        approve_all_pending(root.path());
        withdraw_mineral(
            root.path(),
            "mineral.withdrawn-collision",
            "Withdrawn for conflict test",
        )
        .expect("withdraw mineral");
        let candidate = vec![bulk_item(
            "IMA-WITHDRAWN",
            "mineral.withdrawn-collision",
            "Withdrawn collision",
        )];
        let mut manifest = bulk_manifest(
            &candidate,
            MineralIngestionPolicy::ImaIdentityV1,
            Some(first.batch_id.clone()),
        );
        manifest.release.version = "withdrawn-collision".to_string();
        let conflict = stage_finalize_bulk(root.path(), &manifest, candidate);
        assert_eq!(conflict.status, MineralIngestionBatchStatus::NeedsAttention);
        assert_eq!(
            conflict.anomaly_samples[0].code,
            "unmapped_identity_collision"
        );
    }

    #[test]
    fn complete_snapshot_missing_rows_never_withdraw_public_minerals() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let initial_items = vec![
            bulk_item("IMA-MISSING-1", "mineral.missing-one", "Missing one"),
            bulk_item("IMA-MISSING-2", "mineral.missing-two", "Missing two"),
        ];
        let mut first_manifest =
            bulk_manifest(&initial_items, MineralIngestionPolicy::ImaIdentityV1, None);
        first_manifest.expected_chunk_count = 1;
        let first = stage_finalize_bulk(root.path(), &first_manifest, initial_items.clone());
        approve_finalized_bulk(root.path(), &first);

        let retained = vec![initial_items[0].clone()];
        let mut second_manifest = bulk_manifest(
            &retained,
            MineralIngestionPolicy::ImaIdentityV1,
            Some(first.batch_id.clone()),
        );
        second_manifest.release.version = "missing-second".to_string();
        let second = stage_finalize_bulk(root.path(), &second_manifest, retained);
        assert_eq!(second.status, MineralIngestionBatchStatus::Ready);
        assert_eq!(
            second
                .report_summary
                .as_ref()
                .expect("summary")
                .missing_count,
            1
        );
        assert!(second
            .anomaly_samples
            .iter()
            .any(|sample| sample.code == "missing_from_complete_snapshot"));
        approve_finalized_bulk(root.path(), &second);
        assert!(get_material_detail(root.path(), "mineral.missing-two")
            .expect("detail")
            .is_some());
    }

    #[test]
    fn readiness_does_not_contend_with_a_live_writer_and_invalid_decision_is_typed() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let mut writer = open_connection(root.path(), true).expect("writer connection");
        let tx = writer
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("held writer transaction");
        registry_is_ready(root.path()).expect("steady-state readiness during writer");
        tx.rollback().expect("release writer");
        registry_accepts_writes(root.path()).expect("startup write acceptance");

        let invalid = MineralBatchDecisionRequest {
            manifest_hash: "not-a-hash".to_string(),
            report_hash: format!("sha256:{}", "1".repeat(64)),
            base_batch_id: None,
            note: "Invalid decision".to_string(),
        };
        let error = reject_mineral_ingestion_batch(
            root.path(),
            &format!("batch_{}", "1".repeat(64)),
            "reviewer:test",
            &invalid,
        )
        .expect_err("invalid decision hash");
        let problem = error
            .downcast_ref::<MineralIngestionProblem>()
            .expect("typed invalid decision");
        assert_eq!(problem.kind, MineralIngestionProblemKind::Invalid);
        assert_eq!(problem.code, "invalid_decision");
    }

    #[test]
    fn concurrent_bulk_approvals_apply_once_and_leave_one_durable_backup() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let items = vec![bulk_item(
            "IMA-CONCURRENT",
            "mineral.bulk-concurrent",
            "Bulk concurrent",
        )];
        let manifest = bulk_manifest(&items, MineralIngestionPolicy::ImaIdentityV1, None);
        let batch = stage_finalize_bulk(root.path(), &manifest, items);
        let request = MineralBatchDecisionRequest {
            manifest_hash: batch.manifest_hash.clone(),
            report_hash: batch.report_hash.clone().expect("report hash"),
            base_batch_id: None,
            note: "Concurrent exact approval".to_string(),
        };
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let mut handles = Vec::new();
        for actor in ["reviewer:one", "reviewer:two"] {
            let data_root = root.path().to_path_buf();
            let batch_id = batch.batch_id.clone();
            let request = request.clone();
            let barrier = barrier.clone();
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                approve_mineral_ingestion_batch(&data_root, &batch_id, actor, &request)
            }));
        }
        barrier.wait();
        let outcomes = handles
            .into_iter()
            .map(|handle| handle.join().expect("approval thread").expect("approval"))
            .collect::<Vec<_>>();
        assert_eq!(outcomes.iter().filter(|outcome| outcome.changed).count(), 1);
        assert!(outcomes
            .iter()
            .all(|outcome| outcome.status == MineralIngestionBatchStatus::Approved));
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        let backup_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM mineral_ingestion_backups WHERE batch_id = ?1",
                params![batch.batch_id],
                |row| row.get(0),
            )
            .expect("backup count");
        let approval_event_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM mineral_ingestion_events WHERE batch_id = ?1 AND event_type = 'batch_approved'",
                params![batch.batch_id],
                |row| row.get(0),
            )
            .expect("approval event count");
        assert_eq!(backup_count, 1);
        assert_eq!(approval_event_count, 1);
    }

    #[test]
    fn failed_bulk_activation_rolls_back_every_material_and_removes_orphan_backup() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let items = vec![
            bulk_item("IMA-ROLLBACK-1", "mineral.rollback-one", "Rollback one"),
            bulk_item("IMA-ROLLBACK-2", "mineral.rollback-two", "Rollback two"),
        ];
        let manifest = bulk_manifest(&items, MineralIngestionPolicy::ImaIdentityV1, None);
        let batch = stage_finalize_bulk(root.path(), &manifest, items);
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        conn.execute_batch(
            r#"
            CREATE TRIGGER fail_second_bulk_material
            BEFORE INSERT ON materials
            WHEN new.slug = 'mineral.rollback-two'
            BEGIN
                SELECT RAISE(ABORT, 'injected activation failure');
            END;
            "#,
        )
        .expect("failure trigger");
        drop(conn);
        let request = MineralBatchDecisionRequest {
            manifest_hash: batch.manifest_hash.clone(),
            report_hash: batch.report_hash.clone().expect("report hash"),
            base_batch_id: None,
            note: "Approval with injected failure".to_string(),
        };
        approve_mineral_ingestion_batch(root.path(), &batch.batch_id, "reviewer:test", &request)
            .expect_err("activation failure must roll back");
        assert!(get_material_detail(root.path(), "mineral.rollback-one")
            .expect("first detail")
            .is_none());
        assert!(get_material_detail(root.path(), "mineral.rollback-two")
            .expect("second detail")
            .is_none());
        assert_eq!(
            get_mineral_ingestion_batch(root.path(), &batch.batch_id)
                .expect("batch detail")
                .expect("batch")
                .status,
            MineralIngestionBatchStatus::Ready
        );
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        let recorded_backups: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM mineral_ingestion_backups WHERE batch_id = ?1",
                params![batch.batch_id],
                |row| row.get(0),
            )
            .expect("backup count");
        assert_eq!(recorded_backups, 0);
        let backup_files = fs::read_dir(root.path().join("backups"))
            .expect("backup directory")
            .collect::<std::io::Result<Vec<_>>>()
            .expect("backup entries");
        assert!(backup_files.is_empty());
    }

    #[test]
    #[ignore = "explicit 6,226-record release rehearsal"]
    fn bulk_ingestion_rehearses_full_ima_scale() {
        let started = std::time::Instant::now();
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let items = (0..6_226)
            .map(|index| {
                let mut item = bulk_item(
                    &format!("IMA-SCALE-{index:05}"),
                    &format!("mineral.scale-{index:05}"),
                    &format!("Scale mineral {index:05}"),
                );
                item.formula = format!("X{index}");
                item
            })
            .collect::<Vec<_>>();
        let mut manifest = bulk_manifest(&items, MineralIngestionPolicy::ImaIdentityV1, None);
        manifest.release.version = "ima-scale-6226".to_string();
        manifest.expected_chunk_count = items.len().div_ceil(MAX_MINERAL_INGESTION_CHUNK_ITEMS);
        let finalized = stage_finalize_bulk(root.path(), &manifest, items);
        assert_eq!(finalized.status, MineralIngestionBatchStatus::Ready);
        assert_eq!(
            finalized
                .report_summary
                .as_ref()
                .expect("summary")
                .create_count,
            6_226
        );
        assert_eq!(finalized.review_samples.len(), 25);
        let outcome = approve_finalized_bulk(root.path(), &finalized);
        assert_eq!(outcome.applied_create_count, 6_226);
        assert_eq!(
            registry_stats(root.path()).expect("stats").mineral_count,
            6_226
        );
        let search =
            search_materials(root.path(), "X4242", Some("mineral"), 10).expect("scale search");
        assert_eq!(search.len(), 1);
        assert_eq!(search[0].slug, "mineral.scale-04242");
        eprintln!("6,226-record ingestion rehearsal: {:?}", started.elapsed());
    }

    #[test]
    fn review_migration_preserves_existing_live_minerals_as_published() {
        let root = prepare_data_root();
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        conn.execute(
            r#"
            INSERT INTO minerals(
                slug, common_name, description, mineral_family, formula,
                hardness_mohs, density_g_cm3, crystal_system, color, streak,
                luster, major_elements_pct_json, notes
            ) VALUES (
                'mineral.legacy', 'Legacy mineral', 'Existing public record',
                'Silicates', 'SiO2', 7.0, 2.65, 'trigonal', 'clear', 'white',
                'vitreous', '{}', ''
            )
            "#,
            [],
        )
        .expect("legacy mineral");
        drop(conn);
        init_registry_database(root.path()).expect("initial registry init");

        // Recreate the exact pre-review-workflow shape, then run initialization
        // again as an upgrade rather than as a fresh database.
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        conn.execute_batch(
            r#"
            DROP INDEX idx_materials_publication;
            DROP INDEX idx_materials_public_browse;
            ALTER TABLE materials DROP COLUMN publication_status;
            ALTER TABLE materials DROP COLUMN withdrawal_note;
            ALTER TABLE materials DROP COLUMN withdrawn_at;
            DROP TABLE mineral_review_revisions;
            DELETE FROM schema_migrations WHERE name = 'mineral_review_workflow_v1';
            DELETE FROM schema_migrations WHERE name = 'mineral_withdrawal_v1';
            "#,
        )
        .expect("simulate pre-review registry");
        drop(conn);

        init_registry_database(root.path()).expect("review workflow migration");
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        assert!(
            table_has_column(&conn, "materials", "publication_status").expect("publication column")
        );
        assert!(table_has_column(&conn, "materials", "withdrawal_note")
            .expect("withdrawal note column"));
        assert!(table_has_column(&conn, "materials", "withdrawn_at").expect("withdrawn-at column"));
        let state: String = conn
            .query_row(
                "SELECT publication_status FROM materials WHERE slug = 'mineral.legacy'",
                [],
                |row| row.get(0),
            )
            .expect("migrated publication state");
        assert_eq!(state, "published");
        drop(conn);
        assert!(get_material_detail(root.path(), "mineral.legacy")
            .expect("legacy detail")
            .is_some());
    }

    #[test]
    fn evidence_snapshot_migration_falls_back_to_legacy_source_metadata() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let mut evidence = source("https://authority.example/legacy-snapshot");
        evidence.title = "Legacy evidence title".to_string();
        evidence.publisher = "Legacy publisher".to_string();
        evidence.license_spdx = "CC-BY-4.0".to_string();
        evidence.retrieved_at = "2024-04-05T06:07:08Z".to_string();
        evidence.content_hash = "sha256:legacy".to_string();
        let record = MaterialImport {
            slug: "mineral.legacy-evidence".to_string(),
            canonical_name: "Legacy evidence mineral".to_string(),
            verification_status: "sourced".to_string(),
            sources: vec![evidence],
            ..MaterialImport::default()
        };
        import_material_batch(root.path(), "legacy evidence", &[record]).expect("import");
        approve_all_pending(root.path());

        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        conn.execute_batch(
            r#"
            ALTER TABLE material_evidence DROP COLUMN source_title;
            ALTER TABLE material_evidence DROP COLUMN source_publisher;
            ALTER TABLE material_evidence DROP COLUMN source_license_spdx;
            ALTER TABLE material_evidence DROP COLUMN source_retrieved_at;
            ALTER TABLE material_evidence DROP COLUMN source_content_hash;
            ALTER TABLE material_evidence DROP COLUMN source_attribution_party;
            ALTER TABLE material_evidence DROP COLUMN source_work_title;
            ALTER TABLE material_evidence DROP COLUMN source_work_url;
            ALTER TABLE material_evidence DROP COLUMN source_license_url;
            ALTER TABLE material_evidence DROP COLUMN source_changes_notice;
            ALTER TABLE material_evidence DROP COLUMN source_no_endorsement_notice;
            ALTER TABLE material_evidence DROP COLUMN source_derived_output_license_spdx;
            DELETE FROM schema_migrations WHERE name = 'material_evidence_snapshots_v1';
            DELETE FROM schema_migrations WHERE name = 'material_evidence_attribution_snapshots_v2';
            "#,
        )
        .expect("simulate pre-snapshot evidence schema");
        drop(conn);

        init_registry_database(root.path()).expect("evidence snapshot migration");
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        for column in [
            "source_title",
            "source_publisher",
            "source_license_spdx",
            "source_retrieved_at",
            "source_content_hash",
            "source_attribution_party",
            "source_work_title",
            "source_work_url",
            "source_license_url",
            "source_changes_notice",
            "source_no_endorsement_notice",
            "source_derived_output_license_spdx",
        ] {
            assert!(table_has_column(&conn, "material_evidence", column).expect("snapshot column"));
        }
        drop(conn);
        let detail = get_material_detail(root.path(), "mineral.legacy-evidence")
            .expect("detail")
            .expect("published mineral");
        let evidence = &detail.evidence[0];
        assert_eq!(evidence.title, "Legacy evidence title");
        assert_eq!(evidence.publisher, "Legacy publisher");
        assert_eq!(evidence.license_spdx, "CC-BY-4.0");
        assert_eq!(evidence.retrieved_at, "2024-04-05T06:07:08Z");
        assert_eq!(evidence.content_hash, "sha256:legacy");
        assert!(evidence.attribution.is_none());
    }

    #[test]
    fn canonical_url_collision_preserves_each_minerals_evidence_snapshot() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let mut first_source = source("https://authority.example/shared#first");
        first_source.title = "First extraction".to_string();
        first_source.publisher = "First publisher".to_string();
        first_source.license_spdx = "CC-BY-4.0".to_string();
        first_source.retrieved_at = "2024-01-02T03:04:05Z".to_string();
        first_source.content_hash = "sha256:first".to_string();
        let mut second_source = source("https://AUTHORITY.example:443/shared#second");
        second_source.title = "Second extraction".to_string();
        second_source.publisher = "Second publisher".to_string();
        second_source.license_spdx = "CC0-1.0".to_string();
        second_source.retrieved_at = "2025-02-03T04:05:06Z".to_string();
        second_source.content_hash = "sha256:second".to_string();
        let records = [
            MaterialImport {
                slug: "mineral.first-provenance".to_string(),
                canonical_name: "First provenance".to_string(),
                verification_status: "sourced".to_string(),
                sources: vec![first_source],
                ..MaterialImport::default()
            },
            MaterialImport {
                slug: "mineral.second-provenance".to_string(),
                canonical_name: "Second provenance".to_string(),
                verification_status: "sourced".to_string(),
                sources: vec![second_source],
                ..MaterialImport::default()
            },
        ];
        import_material_batch(root.path(), "provenance collision", &records).expect("import");
        approve_all_pending(root.path());

        let first = get_material_detail(root.path(), "mineral.first-provenance")
            .expect("first detail")
            .expect("first mineral");
        let second = get_material_detail(root.path(), "mineral.second-provenance")
            .expect("second detail")
            .expect("second mineral");
        assert_eq!(first.evidence[0].title, "First extraction");
        assert_eq!(first.evidence[0].publisher, "First publisher");
        assert_eq!(first.evidence[0].content_hash, "sha256:first");
        assert_eq!(second.evidence[0].title, "Second extraction");
        assert_eq!(second.evidence[0].publisher, "Second publisher");
        assert_eq!(second.evidence[0].content_hash, "sha256:second");

        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        let global: (String, String, String) = conn
            .query_row(
                "SELECT title, publisher, content_hash FROM evidence_sources WHERE canonical_url = 'https://authority.example/shared'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("global source metadata");
        assert_eq!(global.0, "First extraction");
        assert_eq!(global.1, "First publisher");
        assert_eq!(global.2, "sha256:first");
    }

    #[test]
    fn imported_mineral_is_in_review_queue_and_invisible_publicly() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let record = MaterialImport {
            slug: "mineral.pending".to_string(),
            canonical_name: "Pending mineral".to_string(),
            description: "Awaiting operator review".to_string(),
            ..MaterialImport::default()
        };
        import_material_batch(root.path(), "pending test", &[record]).expect("mineral import");

        let review = pending_review(root.path(), "mineral.pending");
        assert_eq!(review.revision, 1);
        assert_eq!(review.source_label, "pending test");
        assert_eq!(review.record.canonical_name, "Pending mineral");
        assert!(
            search_materials(root.path(), "Pending mineral", Some("mineral"), 10)
                .expect("public search")
                .is_empty()
        );
        assert!(get_material_detail(root.path(), "mineral.pending")
            .expect("public detail")
            .is_none());
        assert!(offers_for_material(root.path(), "mineral.pending")
            .expect("public offers")
            .is_none());
        assert_eq!(registry_stats(root.path()).expect("stats").mineral_count, 0);
    }

    #[test]
    fn approval_atomically_publishes_the_reviewed_revision() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let record = MaterialImport {
            slug: "mineral.approved".to_string(),
            canonical_name: "Approved mineral".to_string(),
            formula: "Ap2".to_string(),
            verification_status: "sourced".to_string(),
            sources: vec![source("https://authority.example/approved")],
            ..MaterialImport::default()
        };
        import_material_batch(root.path(), "approval test", &[record]).expect("mineral import");
        let review = pending_review(root.path(), "mineral.approved");

        let outcome = approve_mineral_review(
            root.path(),
            review.review_id,
            "Identity and source checked.",
        )
        .expect("approve review");
        assert!(outcome.changed);
        assert_eq!(outcome.status, MineralReviewStatus::Approved);
        assert_eq!(outcome.operator_note, "Identity and source checked.");
        assert!(outcome.reviewed_at.is_some());
        let detail = get_material_detail(root.path(), "mineral.approved")
            .expect("public detail")
            .expect("published mineral");
        assert_eq!(detail.formula, "Ap2");
        assert_eq!(detail.evidence.len(), 1);
        assert!(list_pending_mineral_reviews(root.path(), 10, 0)
            .expect("review queue")
            .items
            .is_empty());
    }

    #[test]
    fn rejected_update_does_not_replace_the_published_revision() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let first = MaterialImport {
            slug: "mineral.stable".to_string(),
            canonical_name: "Stable mineral".to_string(),
            description: "Published description".to_string(),
            ..MaterialImport::default()
        };
        import_material_batch(root.path(), "first", &[first]).expect("first import");
        approve_all_pending(root.path());

        let replacement = MaterialImport {
            slug: "mineral.stable".to_string(),
            canonical_name: "Untrusted replacement".to_string(),
            description: "Must not be public".to_string(),
            ..MaterialImport::default()
        };
        import_material_batch(root.path(), "replacement", &[replacement])
            .expect("replacement import");
        let before = get_material_detail(root.path(), "mineral.stable")
            .expect("detail")
            .expect("published mineral");
        assert_eq!(before.canonical_name, "Stable mineral");

        let review = pending_review(root.path(), "mineral.stable");
        let outcome = reject_mineral_review(
            root.path(),
            review.review_id,
            "Name conflicts with the reviewed identity.",
        )
        .expect("reject review");
        assert!(outcome.changed);
        assert_eq!(outcome.status, MineralReviewStatus::Rejected);
        let after = get_material_detail(root.path(), "mineral.stable")
            .expect("detail")
            .expect("published mineral");
        assert_eq!(after.canonical_name, "Stable mineral");
        assert_eq!(after.description, "Published description");
    }

    #[test]
    fn detail_snapshot_stays_consistent_while_approval_replaces_public_data() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let mut old_source = source("https://authority.example/versioned");
        old_source.title = "Old evidence".to_string();
        old_source.retrieved_at = "2024-01-01T00:00:00Z".to_string();
        let first = MaterialImport {
            slug: "mineral.snapshot".to_string(),
            canonical_name: "Old public name".to_string(),
            verification_status: "sourced".to_string(),
            sources: vec![old_source],
            ..MaterialImport::default()
        };
        import_material_batch(root.path(), "first snapshot", &[first]).expect("first import");
        approve_all_pending(root.path());
        import_provider(
            root.path(),
            &ProviderImport {
                slug: "provider.snapshot".to_string(),
                name: "Snapshot provider".to_string(),
                website_url: "https://provider.example".to_string(),
                offers: vec![OfferImport {
                    material_slug: "mineral.snapshot".to_string(),
                    external_id: "old-offer".to_string(),
                    title: "Old-version specimen".to_string(),
                    product_url: "https://provider.example/old-offer".to_string(),
                    expires_at: Some("2999-01-01T00:00:00Z".to_string()),
                    ..OfferImport::default()
                }],
                ..ProviderImport::default()
            },
        )
        .expect("provider import");

        let mut new_source = source("https://authority.example/versioned#new");
        new_source.title = "New evidence".to_string();
        new_source.retrieved_at = "2025-01-01T00:00:00Z".to_string();
        let replacement = MaterialImport {
            slug: "mineral.snapshot".to_string(),
            canonical_name: "New public name".to_string(),
            verification_status: "sourced".to_string(),
            sources: vec![new_source],
            ..MaterialImport::default()
        };
        import_material_batch(root.path(), "second snapshot", &[replacement])
            .expect("replacement import");
        let review_id = pending_review(root.path(), "mineral.snapshot").review_id;

        let mut read_conn = open_connection(root.path(), false).expect("read connection");
        let read_tx = read_conn.transaction().expect("read transaction");
        let observed_name: String = read_tx
            .query_row(
                "SELECT canonical_name FROM materials WHERE slug = 'mineral.snapshot'",
                [],
                |row| row.get(0),
            )
            .expect("establish read snapshot");
        assert_eq!(observed_name, "Old public name");

        let data_root = root.path().to_path_buf();
        std::thread::spawn(move || {
            approve_mineral_review(&data_root, review_id, "Approve replacement")
        })
        .join()
        .expect("approval worker")
        .expect("replacement approval");

        let during = load_material_detail(&read_tx, "mineral.snapshot")
            .expect("snapshot detail")
            .expect("snapshot mineral");
        assert_eq!(during.canonical_name, "Old public name");
        assert_eq!(during.evidence[0].title, "Old evidence");
        assert_eq!(during.offers.len(), 1);
        read_tx.commit().expect("finish read snapshot");

        let after = get_material_detail(root.path(), "mineral.snapshot")
            .expect("current detail")
            .expect("current mineral");
        assert_eq!(after.canonical_name, "New public name");
        assert_eq!(after.evidence[0].title, "New evidence");
        assert!(after.offers.is_empty());
        assert!(offers_for_material(root.path(), "mineral.snapshot")
            .expect("current offers")
            .expect("published mineral")
            .is_empty());
    }

    #[test]
    fn registry_statistics_use_one_snapshot_during_concurrent_withdrawal() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let record = MaterialImport {
            slug: "mineral.stats-snapshot".to_string(),
            canonical_name: "Statistics snapshot".to_string(),
            verification_status: "sourced".to_string(),
            sources: vec![source("https://authority.example/stats-snapshot")],
            ..MaterialImport::default()
        };
        import_material_batch(root.path(), "statistics snapshot", &[record]).expect("import");
        approve_all_pending(root.path());
        import_provider(
            root.path(),
            &ProviderImport {
                slug: "provider.stats-snapshot".to_string(),
                name: "Statistics provider".to_string(),
                website_url: "https://stats-provider.example".to_string(),
                offers: vec![OfferImport {
                    material_slug: "mineral.stats-snapshot".to_string(),
                    external_id: "stats-offer".to_string(),
                    title: "Statistics specimen".to_string(),
                    product_url: "https://stats-provider.example/specimen".to_string(),
                    ..OfferImport::default()
                }],
                ..ProviderImport::default()
            },
        )
        .expect("provider import");

        let mut read_conn = open_connection(root.path(), false).expect("read connection");
        let read_tx = read_conn.transaction().expect("read transaction");
        let published: i64 = read_tx
            .query_row(
                "SELECT COUNT(*) FROM materials WHERE publication_status = 'published'",
                [],
                |row| row.get(0),
            )
            .expect("establish statistics snapshot");
        assert_eq!(published, 1);
        let data_root = root.path().to_path_buf();
        std::thread::spawn(move || {
            withdraw_mineral(&data_root, "mineral.stats-snapshot", "Snapshot test")
        })
        .join()
        .expect("withdrawal worker")
        .expect("withdrawal");

        let during = load_registry_stats(&read_tx).expect("snapshot statistics");
        assert_eq!(during.mineral_count, 1);
        assert_eq!(during.evidence_count, 1);
        assert_eq!(during.active_offer_count, 1);
        read_tx.commit().expect("finish statistics snapshot");
        let after = registry_stats(root.path()).expect("current statistics");
        assert_eq!(after.mineral_count, 0);
        assert_eq!(after.evidence_count, 0);
        assert_eq!(after.active_offer_count, 0);
    }

    #[test]
    fn search_marks_registry_authority_without_serializing_internal_flag() {
        let root = prepare_data_root();
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        conn.execute_batch(
            r#"
            CREATE TABLE catalog (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                slug TEXT NOT NULL UNIQUE,
                folder_name TEXT NOT NULL,
                source_mineral_id INTEGER NOT NULL,
                image_id INTEGER
            );
            INSERT INTO images(stored_name) VALUES ('legacy-authority.jpg');
            "#,
        )
        .expect("legacy catalog schema");
        conn.execute(
            r#"
            INSERT INTO minerals(
                slug, common_name, description, mineral_family, formula,
                hardness_mohs, density_g_cm3, crystal_system, color, streak,
                luster, major_elements_pct_json, notes, image_id
            ) VALUES (
                'mineral.authority', 'Legacy authority', 'Legacy projection',
                'Silicates', 'SiO2', 7.0, 2.65, 'trigonal', 'clear', 'white',
                'vitreous', '{}', '', (SELECT id FROM images WHERE stored_name = 'legacy-authority.jpg')
            )
            "#,
            [],
        )
        .expect("legacy mineral");
        conn.execute(
            r#"
            INSERT INTO catalog(slug, folder_name, source_mineral_id, image_id)
            SELECT slug, 'legacy-authority-folder', id, image_id
            FROM minerals WHERE slug = 'mineral.authority'
            "#,
            [],
        )
        .expect("legacy catalog record");
        drop(conn);
        init_registry_database(root.path()).expect("registry init");

        let legacy = search_materials(root.path(), "Legacy authority", Some("mineral"), 10)
            .expect("legacy search")
            .remove(0);
        assert!(!legacy.registry_authoritative);
        assert!(!serde_json::to_value(&legacy)
            .expect("serialize search item")
            .as_object()
            .expect("search object")
            .contains_key("registry_authoritative"));
        assert!(published_legacy_mineral_slugs(root.path())
            .expect("legacy slugs")
            .contains("mineral.authority"));
        assert!(
            registered_image_is_public(root.path(), "legacy-authority.jpg")
                .expect("legacy image publication")
        );
        assert!(
            legacy_report_folder_is_public(root.path(), "legacy-authority-folder")
                .expect("legacy report publication")
        );

        let approved = MaterialImport {
            slug: "mineral.authority".to_string(),
            canonical_name: "Registry authority".to_string(),
            ..MaterialImport::default()
        };
        import_material_batch(root.path(), "authority replacement", &[approved]).expect("import");
        approve_all_pending(root.path());
        let registry = search_materials(root.path(), "Registry authority", Some("mineral"), 10)
            .expect("registry search")
            .remove(0);
        assert!(registry.registry_authoritative);
        assert!(get_material_detail(root.path(), "mineral.authority")
            .expect("registry detail")
            .expect("registry mineral")
            .legacy_profile_path
            .is_none());
        let unfiltered_registry = search_materials(root.path(), "", Some("mineral"), 10)
            .expect("unfiltered registry search")
            .into_iter()
            .find(|item| item.slug == "mineral.authority")
            .expect("unfiltered registry item");
        assert!(unfiltered_registry.registry_authoritative);
        assert!(!published_legacy_mineral_slugs(root.path())
            .expect("legacy slugs")
            .contains("mineral.authority"));
        assert!(
            !registered_image_is_public(root.path(), "legacy-authority.jpg")
                .expect("registry-shadowed image")
        );
        assert!(
            !legacy_report_folder_is_public(root.path(), "legacy-authority-folder")
                .expect("registry-shadowed report")
        );
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        let image_id: Option<i64> = conn
            .query_row(
                "SELECT image_id FROM materials WHERE slug = 'mineral.authority'",
                [],
                |row| row.get(0),
            )
            .expect("registry material image");
        assert!(image_id.is_none());
    }

    #[test]
    fn image_detach_migration_clears_pre_fix_registry_image_links() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        import_material_batch(
            root.path(),
            "pre-fix registry image",
            &[draft_material(
                "mineral.pre-fix-image",
                "Pre-fix image mineral",
            )],
        )
        .expect("mineral import");
        approve_all_pending(root.path());
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        conn.execute(
            "INSERT INTO images(stored_name) VALUES ('inherited-pre-fix.jpg')",
            [],
        )
        .expect("legacy image");
        conn.execute(
            r#"
            UPDATE materials
            SET image_id = (SELECT id FROM images WHERE stored_name = 'inherited-pre-fix.jpg')
            WHERE slug = 'mineral.pre-fix-image'
            "#,
            [],
        )
        .expect("seed pre-fix image link");
        conn.execute(
            "DELETE FROM schema_migrations WHERE name = ?1",
            params![REGISTRY_IMAGE_DETACH_MIGRATION],
        )
        .expect("rewind image detach migration");
        let before: Option<i64> = conn
            .query_row(
                "SELECT image_id FROM materials WHERE slug = 'mineral.pre-fix-image'",
                [],
                |row| row.get(0),
            )
            .expect("pre-fix image link");
        assert!(before.is_some());
        drop(conn);
        assert!(
            !registered_image_is_public(root.path(), "inherited-pre-fix.jpg")
                .expect("registry direct image must stay private")
        );

        init_registry_database(root.path()).expect("image detach migration");
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        let after: Option<i64> = conn
            .query_row(
                "SELECT image_id FROM materials WHERE slug = 'mineral.pre-fix-image'",
                [],
                |row| row.get(0),
            )
            .expect("migrated image link");
        assert!(after.is_none());
        let applied: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE name = ?1",
                params![REGISTRY_IMAGE_DETACH_MIGRATION],
                |row| row.get(0),
            )
            .expect("image detach marker");
        assert_eq!(applied, 1);
    }

    #[test]
    fn withdrawal_hides_imported_and_legacy_minerals_and_retires_offers() {
        let root = prepare_data_root();
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        conn.execute_batch(
            r#"
            CREATE TABLE catalog (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                slug TEXT NOT NULL UNIQUE,
                folder_name TEXT NOT NULL,
                source_mineral_id INTEGER NOT NULL,
                image_id INTEGER
            );
            INSERT INTO images(stored_name) VALUES ('withdraw-legacy.jpg');
            "#,
        )
        .expect("legacy catalog schema");
        conn.execute(
            r#"
            INSERT INTO minerals(
                slug, common_name, description, mineral_family, formula,
                hardness_mohs, density_g_cm3, crystal_system, color, streak,
                luster, major_elements_pct_json, notes, image_id
            ) VALUES (
                'mineral.withdraw-legacy', 'Withdraw legacy', 'Legacy record',
                'Silicates', 'SiO2', 7.0, 2.65, 'trigonal', 'clear', 'white',
                'vitreous', '{}', '', (SELECT id FROM images WHERE stored_name = 'withdraw-legacy.jpg')
            )
            "#,
            [],
        )
        .expect("legacy mineral");
        conn.execute(
            r#"
            INSERT INTO catalog(slug, folder_name, source_mineral_id, image_id)
            SELECT slug, 'withdraw-legacy-folder', id, image_id
            FROM minerals WHERE slug = 'mineral.withdraw-legacy'
            "#,
            [],
        )
        .expect("legacy catalog record");
        drop(conn);
        init_registry_database(root.path()).expect("registry init");
        import_material_batch(
            root.path(),
            "withdraw imported",
            &[draft_material(
                "mineral.withdraw-imported",
                "Withdraw imported",
            )],
        )
        .expect("registry mineral import");
        approve_all_pending(root.path());
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        conn.execute(
            "INSERT INTO images(stored_name) VALUES ('registry-media.jpg')",
            [],
        )
        .expect("registry media image");
        conn.execute(
            r#"
            INSERT INTO material_media(material_id, image_id)
            SELECT m.id, i.id
            FROM materials m, images i
            WHERE m.slug = 'mineral.withdraw-imported'
              AND i.stored_name = 'registry-media.jpg'
            "#,
            [],
        )
        .expect("registry media association");
        drop(conn);
        import_provider(
            root.path(),
            &ProviderImport {
                slug: "provider.withdraw".to_string(),
                name: "Withdrawal provider".to_string(),
                website_url: "https://withdraw.example".to_string(),
                offers: vec![
                    OfferImport {
                        material_slug: "mineral.withdraw-legacy".to_string(),
                        external_id: "legacy".to_string(),
                        title: "Legacy specimen".to_string(),
                        product_url: "https://withdraw.example/legacy".to_string(),
                        ..OfferImport::default()
                    },
                    OfferImport {
                        material_slug: "mineral.withdraw-imported".to_string(),
                        external_id: "imported".to_string(),
                        title: "Imported specimen".to_string(),
                        product_url: "https://withdraw.example/imported".to_string(),
                        ..OfferImport::default()
                    },
                ],
                ..ProviderImport::default()
            },
        )
        .expect("provider import");
        assert_eq!(registry_stats(root.path()).expect("stats").mineral_count, 2);
        assert!(
            registered_image_is_public(root.path(), "withdraw-legacy.jpg")
                .expect("public legacy image")
        );
        assert!(
            legacy_report_folder_is_public(root.path(), "withdraw-legacy-folder")
                .expect("public legacy report")
        );
        assert!(
            registered_image_is_public(root.path(), "registry-media.jpg")
                .expect("public registry media")
        );

        assert!(withdraw_mineral(
            root.path(),
            "mineral.withdraw-legacy",
            "Legacy record withdrawn."
        )
        .expect("withdraw legacy"));
        assert!(withdraw_mineral(
            root.path(),
            "mineral.withdraw-imported",
            "Registry record withdrawn."
        )
        .expect("withdraw imported"));
        assert!(!withdraw_mineral(
            root.path(),
            "mineral.withdraw-imported",
            "Repeated withdrawal must not rewrite the audit note."
        )
        .expect("repeat withdrawal"));

        for slug in ["mineral.withdraw-legacy", "mineral.withdraw-imported"] {
            assert!(search_materials(root.path(), slug, Some("mineral"), 10)
                .expect("search")
                .is_empty());
            assert!(get_material_detail(root.path(), slug)
                .expect("detail")
                .is_none());
            assert!(offers_for_material(root.path(), slug)
                .expect("offers")
                .is_none());
        }
        let stats = registry_stats(root.path()).expect("stats");
        assert_eq!(stats.mineral_count, 0);
        assert_eq!(stats.active_offer_count, 0);
        assert!(
            !registered_image_is_public(root.path(), "withdraw-legacy.jpg")
                .expect("withdrawn legacy image")
        );
        assert!(
            !legacy_report_folder_is_public(root.path(), "withdraw-legacy-folder")
                .expect("withdrawn legacy report")
        );
        assert!(
            !registered_image_is_public(root.path(), "registry-media.jpg")
                .expect("withdrawn registry media")
        );
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        let audit: (String, Option<String>) = conn
            .query_row(
                "SELECT withdrawal_note, withdrawn_at FROM materials WHERE slug = 'mineral.withdraw-imported'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("withdrawal audit");
        assert_eq!(audit.0, "Registry record withdrawn.");
        assert!(audit.1.is_some());
        let active_offers: i64 = conn
            .query_row("SELECT COUNT(*) FROM offers WHERE active = 1", [], |row| {
                row.get(0)
            })
            .expect("active offers");
        assert_eq!(active_offers, 0);
        drop(conn);

        let compound = MaterialImport {
            slug: "compound.not-a-mineral".to_string(),
            record_type: "compound".to_string(),
            canonical_name: "Compatibility compound".to_string(),
            ..MaterialImport::default()
        };
        import_material_batch(root.path(), "compound compatibility", &[compound])
            .expect("compound import");
        assert!(withdraw_mineral(root.path(), "compound.not-a-mineral", "Not allowed").is_err());
        assert!(withdraw_mineral(root.path(), "mineral.unknown", "Not found").is_err());
    }

    #[test]
    fn withdrawal_supersedes_pending_revision_and_later_approval_republishes_cleanly() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        import_material_batch(
            root.path(),
            "initial",
            &[draft_material("mineral.republish", "Initial name")],
        )
        .expect("initial import");
        approve_all_pending(root.path());
        import_material_batch(
            root.path(),
            "pending before withdrawal",
            &[draft_material(
                "mineral.republish",
                "Superseded pending name",
            )],
        )
        .expect("pending import");
        let superseded_id = pending_review(root.path(), "mineral.republish").review_id;
        assert!(
            withdraw_mineral(root.path(), "mineral.republish", "Identity disputed")
                .expect("withdraw mineral")
        );
        assert!(list_pending_mineral_reviews(root.path(), 10, 0)
            .expect("review queue")
            .items
            .is_empty());
        assert!(approve_mineral_review(root.path(), superseded_id, "Stale approval").is_err());

        import_material_batch(
            root.path(),
            "reviewed republication",
            &[draft_material("mineral.republish", "Restored identity")],
        )
        .expect("republication import");
        let review_id = pending_review(root.path(), "mineral.republish").review_id;
        approve_mineral_review(root.path(), review_id, "Identity re-established")
            .expect("approve republication");
        let detail = get_material_detail(root.path(), "mineral.republish")
            .expect("detail")
            .expect("republished mineral");
        assert_eq!(detail.canonical_name, "Restored identity");
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        let state: (String, String, Option<String>) = conn
            .query_row(
                "SELECT publication_status, withdrawal_note, withdrawn_at FROM materials WHERE slug = 'mineral.republish'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("republication state");
        assert_eq!(state.0, "published");
        assert!(state.1.is_empty());
        assert!(state.2.is_none());
    }

    #[test]
    fn review_decisions_are_idempotent_and_concurrent_safe() {
        use std::sync::{Arc, Barrier};

        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        import_material_batch(
            root.path(),
            "concurrency test",
            &[draft_material("mineral.concurrent", "Concurrent mineral")],
        )
        .expect("mineral import");
        let review_id = pending_review(root.path(), "mineral.concurrent").review_id;
        let data_root = root.path().to_path_buf();
        let barrier = Arc::new(Barrier::new(3));
        let handles = (0..2)
            .map(|_| {
                let data_root = data_root.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    approve_mineral_review(&data_root, review_id, "Concurrent approval")
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let outcomes = handles
            .into_iter()
            .map(|handle| handle.join().expect("review worker").expect("approval"))
            .collect::<Vec<_>>();

        assert_eq!(outcomes.iter().filter(|outcome| outcome.changed).count(), 1);
        assert!(outcomes
            .iter()
            .all(|outcome| outcome.status == MineralReviewStatus::Approved));
        assert!(reject_mineral_review(root.path(), review_id, "Conflicting decision").is_err());
        assert!(get_material_detail(root.path(), "mineral.concurrent")
            .expect("detail")
            .is_some());
    }

    #[test]
    fn validates_cas_checksum() {
        assert!(is_valid_cas_number("7647-14-5"));
        assert!(!is_valid_cas_number("7647-14-4"));
        assert!(!is_valid_cas_number("not-a-cas"));
    }

    #[test]
    fn new_evidence_requires_an_explicit_valid_retrieval_time() {
        let mut missing = source("https://authority.example/missing-retrieval");
        missing.retrieved_at.clear();
        assert!(validate_evidence(&missing).is_err());

        let mut invalid = source("https://authority.example/invalid-retrieval");
        invalid.retrieved_at = "sometime last week".to_string();
        assert!(validate_evidence(&invalid).is_err());
    }

    #[test]
    fn granular_claim_scopes_are_normalized_and_require_values() {
        let granular = EvidenceImport {
            url: "https://authority.example/hardness".to_string(),
            title: "Hardness reference".to_string(),
            claim_scope: " Properties.Hardness_Mohs ".to_string(),
            claim: json!({
                "value": {"min": 6.0, "max": 6.5},
                "unit": "Mohs",
                "conditions": {"specimen": "natural"},
                "source_locator": "table 2",
                "note": "reported range"
            }),
            retrieved_at: "2026-08-15T09:00:00Z".to_string(),
            ..EvidenceImport::default()
        };
        validate_evidence(&granular).expect("granular claim");
        assert_eq!(
            normalize_claim_scope(&granular.claim_scope).expect("normalized scope"),
            "properties.hardness_mohs"
        );

        let missing_value = EvidenceImport {
            claim: json!({"unit": "Mohs"}),
            ..granular.clone()
        };
        assert!(validate_evidence(&missing_value).is_err());

        let unsupported_identity = EvidenceImport {
            claim_scope: "identity.unrecognized".to_string(),
            ..granular.clone()
        };
        assert!(validate_evidence(&unsupported_identity).is_err());

        let invalid_key = EvidenceImport {
            claim_scope: "properties.hardness-mohs".to_string(),
            ..granular.clone()
        };
        assert!(validate_evidence(&invalid_key).is_err());

        validate_evidence(&source("https://authority.example/legacy"))
            .expect("broad legacy scope remains compatible");
    }

    #[test]
    fn duplicate_canonical_source_and_normalized_scope_is_rejected() {
        let first = EvidenceImport {
            url: "https://EXAMPLE.org:443/reference#first".to_string(),
            title: "First extraction".to_string(),
            claim_scope: "Properties.Hardness_Mohs".to_string(),
            claim: json!({"value": 6.0, "unit": "Mohs"}),
            retrieved_at: "2026-08-15T09:00:00Z".to_string(),
            ..EvidenceImport::default()
        };
        let second = EvidenceImport {
            url: "https://example.org/reference#second".to_string(),
            title: "Second extraction".to_string(),
            claim_scope: "properties.hardness_mohs".to_string(),
            claim: json!({"value": 6.5, "unit": "Mohs"}),
            retrieved_at: "2026-08-15T09:00:00Z".to_string(),
            ..EvidenceImport::default()
        };
        let duplicate = MaterialImport {
            slug: "mineral.duplicate-claim".to_string(),
            canonical_name: "Duplicate claim".to_string(),
            sources: vec![first.clone(), second.clone()],
            ..MaterialImport::default()
        };
        let error = validate_import(&duplicate).expect_err("duplicate evidence claim");
        assert!(error.to_string().contains("duplicate evidence claim"));

        let distinct_scope = MaterialImport {
            slug: "mineral.distinct-claim".to_string(),
            canonical_name: "Distinct claim".to_string(),
            sources: vec![
                first,
                EvidenceImport {
                    claim_scope: "properties.density_g_cm3".to_string(),
                    ..second
                },
            ],
            ..MaterialImport::default()
        };
        validate_import(&distinct_scope).expect("one source may support distinct scopes");
    }

    #[test]
    fn detail_exposes_safe_claim_transparency_and_deduplicates_cas() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let claim = json!({
            "value": {"min": 6.0, "max": 6.5},
            "unit": "Mohs",
            "conditions": {"specimen": "natural"},
            "source_locator": "table 2",
            "note": "reported range"
        });
        let evidence = EvidenceImport {
            url: "https://authority.example/hardness".to_string(),
            title: "Hardness reference".to_string(),
            publisher: "Example authority".to_string(),
            license_spdx: "CC0-1.0".to_string(),
            claim_scope: " Properties.Hardness_Mohs ".to_string(),
            claim: claim.clone(),
            confidence: 0.876,
            review_status: "reviewed".to_string(),
            retrieved_at: "2026-08-15T09:00:00Z".to_string(),
            ..EvidenceImport::default()
        };
        let record = MaterialImport {
            slug: "mineral.claim-detail".to_string(),
            canonical_name: "Claim detail".to_string(),
            cas_number: Some("7647-14-5".to_string()),
            identifiers: json!({"cas": "7647-14-5", "external_id": "example-1"}),
            properties: json!({"hardness_mohs": {"min": 6.0, "max": 6.5}}),
            sources: vec![evidence],
            ..MaterialImport::default()
        };
        import_material_batch(root.path(), "claim transparency", &[record])
            .expect("material import");
        approve_all_pending(root.path());

        let detail = get_material_detail(root.path(), "mineral.claim-detail")
            .expect("detail query")
            .expect("published detail");
        assert_eq!(detail.cas_number.as_deref(), Some("7647-14-5"));
        assert_eq!(detail.identifiers.len(), 1);
        assert_eq!(detail.identifiers[0].key, "external_id");

        let source = &detail.evidence[0];
        assert_eq!(source.claim_scope, "properties.hardness_mohs");
        assert_eq!(source.claim, claim);
        assert_eq!(source.claim_label, "Hardness (Mohs)");
        assert_eq!(source.confidence, 0.876);
        assert_eq!(source.confidence_percent, 88);
        assert!(source.claim_summary.contains("6"));
        assert!(source.claim_summary.contains("6.5"));
        assert!(source.claim_summary.contains("Mohs"));
        assert!(source.claim_summary.contains("natural"));
        assert!(source.claim_summary.contains("table 2"));
        assert!(source.claim_summary.contains("reported range"));
        assert!(!source.claim_summary.contains('{'));
        assert!(!source.claim_summary.contains('}'));
        assert!(!source.claim_summary.contains('"'));
    }

    #[test]
    fn verified_records_need_independent_sources() {
        let record = MaterialImport {
            slug: "compound.sodium-chloride".to_string(),
            canonical_name: "Sodium chloride".to_string(),
            formula: "NaCl".to_string(),
            cas_number: Some("7647-14-5".to_string()),
            verification_status: "verified".to_string(),
            data_quality_score: 0.95,
            sources: vec![source("https://example.org/source-a")],
            ..MaterialImport::default()
        };
        assert!(validate_import(&record).is_err());
    }

    #[test]
    fn review_gates_use_canonical_independently_reviewed_sources() {
        let mut unreviewed = source("https://example.org/source-a");
        unreviewed.review_status = "unreviewed".to_string();
        let reviewed = MaterialImport {
            slug: "compound.review-gate".to_string(),
            canonical_name: "Review gate".to_string(),
            verification_status: "reviewed".to_string(),
            sources: vec![unreviewed],
            ..MaterialImport::default()
        };
        assert!(validate_import(&reviewed).is_err());

        let mut verified_source = source("https://EXAMPLE.org:443/source-a#second-claim");
        verified_source.review_status = "verified".to_string();
        let duplicate_sources = MaterialImport {
            slug: "compound.duplicate-sources".to_string(),
            canonical_name: "Duplicate sources".to_string(),
            verification_status: "verified".to_string(),
            sources: vec![
                source("https://example.org/source-a#first-claim"),
                verified_source,
            ],
            ..MaterialImport::default()
        };
        assert!(validate_import(&duplicate_sources).is_err());

        let mut independent_verified = source("https://authority.example/source-b");
        independent_verified.review_status = "verified".to_string();
        let verified = MaterialImport {
            slug: "compound.independent-sources".to_string(),
            canonical_name: "Independent sources".to_string(),
            verification_status: "verified".to_string(),
            sources: vec![source("https://example.org/source-a"), independent_verified],
            ..MaterialImport::default()
        };
        validate_import(&verified).expect("two independently reviewed sources should pass");
    }

    #[test]
    fn evidence_urls_are_canonicalized_for_storage() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let record = MaterialImport {
            slug: "compound.canonical-source".to_string(),
            canonical_name: "Canonical source".to_string(),
            verification_status: "sourced".to_string(),
            sources: vec![source("https://EXAMPLE.org:443/source#claim")],
            ..MaterialImport::default()
        };
        import_material_batch(root.path(), "test", &[record]).expect("material import");
        approve_all_pending(root.path());
        let detail = get_material_detail(root.path(), "compound.canonical-source")
            .expect("detail")
            .expect("record");
        assert_eq!(
            detail.evidence[0].canonical_url,
            "https://example.org/source"
        );
    }

    #[test]
    fn imports_and_searches_a_sourced_record() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let record = MaterialImport {
            slug: "compound.sodium-chloride".to_string(),
            canonical_name: "Sodium chloride".to_string(),
            formula: "NaCl".to_string(),
            cas_number: Some("7647-14-5".to_string()),
            identifiers: json!({"cas": "7647-14-5"}),
            properties: json!({"melting_point_c": 801, "appearance": "white solid"}),
            safety: json!({"handling": "standard laboratory precautions"}),
            synonyms: vec!["table salt".to_string()],
            verification_status: "sourced".to_string(),
            data_quality_score: 0.7,
            license_spdx: "CC0-1.0".to_string(),
            sources: vec![source("https://example.org/source-a")],
            ..MaterialImport::default()
        };
        let summary = import_material_batch(root.path(), "test", &[record]).expect("import");
        assert_eq!(summary.imported_count, 1);
        assert_eq!(summary.queued_count, 1);
        assert_eq!(summary.published_count, 0);
        assert_eq!(summary.review_ids.len(), 1);
        approve_all_pending(root.path());
        let results = search_materials(root.path(), "table salt", None, 10).expect("search");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].canonical_name, "Sodium chloride");
        let detail = get_material_detail(root.path(), "compound.sodium-chloride")
            .expect("detail")
            .expect("record");
        assert_eq!(detail.evidence.len(), 1);
        assert_eq!(detail.properties.len(), 2);
        assert_eq!(detail.safety.len(), 1);
        assert!(detail.legacy_profile_path.is_none());
    }

    #[test]
    fn paged_search_reports_total_without_truncating_results() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let records = (0..55)
            .map(|index| MaterialImport {
                slug: format!("material.paged-{index:03}"),
                record_type: if index < 5 { "mineral" } else { "compound" }.to_string(),
                canonical_name: format!("Paged material {index:03}"),
                description: "A paged registry test material".to_string(),
                verification_status: "draft".to_string(),
                ..MaterialImport::default()
            })
            .collect::<Vec<_>>();
        import_material_batch(root.path(), "pagination test", &records).expect("material import");
        approve_all_pending(root.path());

        let first =
            search_materials_page(root.path(), "paged", None, 24, 0).expect("first result page");
        assert_eq!(first.total_count, 55);
        assert_eq!(first.items.len(), 24);
        assert_eq!(first.limit, 24);
        assert_eq!(first.offset, 0);

        let last =
            search_materials_page(root.path(), "paged", None, 24, 48).expect("last result page");
        assert_eq!(last.total_count, 55);
        assert_eq!(last.items.len(), 7);
        assert_eq!(last.offset, 48);

        let compound_last = search_materials_page(root.path(), "paged", Some("compound"), 24, 48)
            .expect("filtered result page");
        assert_eq!(compound_last.total_count, 50);
        assert_eq!(compound_last.items.len(), 2);

        let compatibility =
            search_materials(root.path(), "paged", None, 24).expect("compatibility search");
        assert_eq!(compatibility.len(), 24);
    }

    #[test]
    fn search_description_excerpt_is_unicode_bounded_and_preserves_full_text() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let description = "Ásványi összetétel és biztonságos kezelés. ".repeat(120);
        let record = MaterialImport {
            slug: "compound.long-description".to_string(),
            canonical_name: "Long description".to_string(),
            description: description.clone(),
            verification_status: "draft".to_string(),
            ..MaterialImport::default()
        };
        import_material_batch(root.path(), "excerpt test", &[record]).expect("material import");
        approve_all_pending(root.path());
        let result = search_materials(root.path(), "Long description", None, 10)
            .expect("search")
            .remove(0);
        assert_eq!(result.description, description.trim());
        assert!(result.description_excerpt.chars().count() <= 320);
        assert!(result.description_excerpt.ends_with('…'));
        let normalized = description.split_whitespace().collect::<Vec<_>>().join(" ");
        let excerpt_without_ellipsis = result.description_excerpt.trim_end_matches('…');
        assert!(normalized.starts_with(excerpt_without_ellipsis));
        assert_eq!(
            normalized[excerpt_without_ellipsis.len()..].chars().next(),
            Some(' ')
        );
    }

    #[test]
    fn structured_facts_are_human_readable_and_do_not_emit_raw_json() {
        let facts = json_object_to_facts(
            r#"{
                "crystal_system": "trigonal",
                "density_g_cm3": 2.97000002861023,
                "major_elements_pct": {"Be": 19.9, "O": 48.8, "Si": 31.3}
            }"#,
        );

        assert_eq!(facts[0].name, "Crystal system");
        assert_eq!(facts[1].name, "Density (g/cm³)");
        assert_eq!(facts[1].value, "2.97");
        assert_eq!(facts[2].name, "Major elements (%)");
        assert_eq!(facts[2].value, "Be 19.9%, O 48.8%, Si 31.3%");
        assert!(facts.iter().all(|fact| {
            !fact.name.contains('_') && !fact.value.contains('{') && !fact.value.contains('}')
        }));
    }

    #[test]
    fn provider_claims_remain_separate_from_material_evidence() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let record = MaterialImport {
            slug: "compound.sodium-chloride".to_string(),
            canonical_name: "Sodium chloride".to_string(),
            formula: "NaCl".to_string(),
            verification_status: "draft".to_string(),
            ..MaterialImport::default()
        };
        import_material_batch(root.path(), "test", &[record]).expect("material import");
        approve_all_pending(root.path());
        let provider = ProviderImport {
            slug: "provider.example-labs".to_string(),
            name: "Example Labs".to_string(),
            website_url: "https://example.org".to_string(),
            trust_score: 0.4,
            offers: vec![OfferImport {
                material_slug: "compound.sodium-chloride".to_string(),
                external_id: "nacl-001".to_string(),
                title: "Sodium chloride sample".to_string(),
                product_url: "https://example.org/nacl".to_string(),
                stock_status: "in_stock".to_string(),
                provider_claims: json!({"purity": "provider states 99%"}),
                ..OfferImport::default()
            }],
            ..ProviderImport::default()
        };
        import_provider(root.path(), &provider).expect("provider import");
        let detail = get_material_detail(root.path(), "compound.sodium-chloride")
            .expect("detail")
            .expect("record");
        assert_eq!(detail.offers.len(), 1);
        assert!(detail.evidence.is_empty());
        assert_eq!(detail.offers[0].verification_status, "provider_claim");
        assert_eq!(detail.offers[0].pricing_basis, "quote");
        assert!(detail.offers[0].pricing_basis_display.is_empty());

        let mut empty_snapshot = provider;
        empty_snapshot.offers.clear();
        import_provider(root.path(), &empty_snapshot).expect("empty provider snapshot");
        let detail = get_material_detail(root.path(), "compound.sodium-chloride")
            .expect("detail")
            .expect("record");
        assert!(detail.offers.is_empty());
    }

    #[test]
    fn pricing_basis_has_a_safe_human_readable_display() {
        assert_eq!(pricing_basis_display("per_kg"), "kg");
        assert_eq!(pricing_basis_display("PER-METRIC-TON"), "t");
        assert_eq!(
            pricing_basis_display("per_25kg_drum<script>"),
            "25kg drum script"
        );
    }

    #[test]
    fn suspended_and_expired_provider_offers_are_not_active() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        import_material_batch(
            root.path(),
            "test",
            &[draft_material("compound.test", "Test compound")],
        )
        .expect("material import");
        approve_all_pending(root.path());

        let expired_provider = ProviderImport {
            slug: "provider.expired".to_string(),
            name: "Expired provider".to_string(),
            website_url: "https://expired.example".to_string(),
            offers: vec![OfferImport {
                material_slug: "compound.test".to_string(),
                external_id: "expired".to_string(),
                title: "Expired listing".to_string(),
                product_url: "https://expired.example/listing".to_string(),
                expires_at: Some("2000-01-01T00:00:00-05:00".to_string()),
                ..OfferImport::default()
            }],
            ..ProviderImport::default()
        };
        import_provider(root.path(), &expired_provider).expect("expired provider import");

        let suspended_provider = ProviderImport {
            slug: "provider.suspended".to_string(),
            name: "Suspended provider".to_string(),
            website_url: "https://suspended.example".to_string(),
            verification_status: "suspended".to_string(),
            offers: vec![OfferImport {
                material_slug: "compound.test".to_string(),
                external_id: "suspended".to_string(),
                title: "Suspended listing".to_string(),
                product_url: "https://suspended.example/listing".to_string(),
                expires_at: Some("2999-01-01 00:00:00".to_string()),
                ..OfferImport::default()
            }],
            ..ProviderImport::default()
        };
        import_provider(root.path(), &suspended_provider).expect("suspended provider import");

        let live_provider = ProviderImport {
            slug: "provider.live".to_string(),
            name: "Live provider".to_string(),
            website_url: "https://live.example".to_string(),
            offers: vec![OfferImport {
                material_slug: "compound.test".to_string(),
                external_id: "live".to_string(),
                title: "Live listing".to_string(),
                product_url: "https://live.example/listing".to_string(),
                last_checked_at: "2026-08-14 12:00:00".to_string(),
                expires_at: Some("2999-01-01T00:00:00+02:00".to_string()),
                ..OfferImport::default()
            }],
            ..ProviderImport::default()
        };
        import_provider(root.path(), &live_provider).expect("live provider import");

        let stats = registry_stats(root.path()).expect("stats");
        assert_eq!(stats.active_offer_count, 1);
        assert_eq!(stats.provider_count, 2);
        let search = search_materials(root.path(), "Test compound", None, 10).expect("search");
        assert_eq!(search[0].active_offer_count, 1);
        let detail = get_material_detail(root.path(), "compound.test")
            .expect("detail")
            .expect("record");
        assert_eq!(detail.offers.len(), 1);
        assert_eq!(detail.offers[0].provider_slug, "provider.live");

        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        let suspended_active: i64 = conn
            .query_row(
                "SELECT active FROM providers WHERE slug = 'provider.suspended'",
                [],
                |row| row.get(0),
            )
            .expect("provider active flag");
        assert_eq!(suspended_active, 0);
    }

    #[test]
    fn offer_timestamps_are_strict_and_normalized() {
        assert!(normalize_timestamp("timestamp", "2026-02-30 10:00:00").is_err());
        assert!(normalize_timestamp("timestamp", "next Tuesday").is_err());
        assert_eq!(
            normalize_timestamp("timestamp", "2026-01-02 03:04:05").expect("SQLite timestamp"),
            "2026-01-02T03:04:05Z"
        );
        assert_eq!(
            normalize_timestamp("timestamp", "2026-01-02T03:04:05+02:00")
                .expect("RFC 3339 timestamp"),
            "2026-01-02T01:04:05Z"
        );

        let invalid = OfferImport {
            material_slug: "compound.test".to_string(),
            external_id: "bad-time".to_string(),
            title: "Bad time".to_string(),
            product_url: "https://example.org/bad-time".to_string(),
            last_checked_at: "2026-02-30 10:00:00".to_string(),
            ..OfferImport::default()
        };
        assert!(validate_offer_import(&invalid).is_err());
    }

    #[test]
    fn provider_evidence_collision_preserves_scientific_metadata() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let record = MaterialImport {
            slug: "compound.collision".to_string(),
            canonical_name: "Collision compound".to_string(),
            verification_status: "sourced".to_string(),
            sources: vec![source("https://authority.example/fact#identity")],
            ..MaterialImport::default()
        };
        import_material_batch(root.path(), "test", &[record]).expect("material import");
        approve_all_pending(root.path());
        let provider = ProviderImport {
            slug: "provider.collision".to_string(),
            name: "Commercial provider".to_string(),
            website_url: "https://provider.example".to_string(),
            offers: vec![OfferImport {
                material_slug: "compound.collision".to_string(),
                external_id: "collision".to_string(),
                title: "Commercial listing title".to_string(),
                product_url: "https://provider.example/listing".to_string(),
                evidence_url: Some("https://authority.example:443/fact#offer".to_string()),
                ..OfferImport::default()
            }],
            ..ProviderImport::default()
        };
        import_provider(root.path(), &provider).expect("provider import");

        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        let metadata: (String, String, String) = conn
            .query_row(
                "SELECT title, publisher, license_spdx FROM evidence_sources WHERE canonical_url = 'https://authority.example/fact'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("evidence source");
        assert_eq!(metadata.0, "Authoritative source");
        assert_eq!(metadata.1, "Example authority");
        assert_eq!(metadata.2, "CC0-1.0");
    }

    #[test]
    fn material_reimport_replaces_its_evidence_snapshot() {
        let root = prepare_data_root();
        init_registry_database(root.path()).expect("registry init");
        let first = MaterialImport {
            slug: "compound.snapshot".to_string(),
            canonical_name: "Snapshot compound".to_string(),
            verification_status: "sourced".to_string(),
            sources: vec![source("https://example.org/old-source")],
            ..MaterialImport::default()
        };
        import_material_batch(root.path(), "first", &[first]).expect("first import");
        approve_all_pending(root.path());
        let second = MaterialImport {
            slug: "compound.snapshot".to_string(),
            canonical_name: "Snapshot compound".to_string(),
            verification_status: "sourced".to_string(),
            sources: vec![source("https://example.org/new-source")],
            ..MaterialImport::default()
        };
        import_material_batch(root.path(), "second", &[second]).expect("second import");
        approve_all_pending(root.path());

        let detail = get_material_detail(root.path(), "compound.snapshot")
            .expect("detail")
            .expect("record");
        assert_eq!(detail.evidence.len(), 1);
        assert_eq!(
            detail.evidence[0].canonical_url,
            "https://example.org/new-source"
        );
        let conn = Connection::open(root.path().join(DATABASE_FILE)).expect("database");
        let retained_sources: i64 = conn
            .query_row("SELECT COUNT(*) FROM evidence_sources", [], |row| {
                row.get(0)
            })
            .expect("source count");
        assert_eq!(retained_sources, 2);
    }

    #[test]
    fn provider_offer_contract_uses_mineral_slug_with_legacy_alias() {
        let current: OfferImport = serde_json::from_value(json!({
            "mineral_slug": "mineral.quartz"
        }))
        .expect("current provider offer");
        assert_eq!(current.material_slug, "mineral.quartz");

        let legacy: OfferImport = serde_json::from_value(json!({
            "material_slug": "mineral.quartz"
        }))
        .expect("legacy provider offer");
        assert_eq!(legacy.material_slug, "mineral.quartz");

        let serialized = serde_json::to_value(current).expect("serialize provider offer");
        assert_eq!(serialized["mineral_slug"], "mineral.quartz");
        assert!(serialized.get("material_slug").is_none());
    }
}
