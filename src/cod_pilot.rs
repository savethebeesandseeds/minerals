//! Preparation, verification, and bounded metadata discovery for the private COD
//! crystallography pilot.
//!
//! `prepare` and `verify` are offline. Network access exists only behind the
//! explicit, sequential `fetch` command, whose responses are written to the
//! private content-addressed pilot store under frozen limits. The crate has no
//! dependency on the mutable mineral registry, never writes a database, and opens
//! its sole database input—the validated public catalog—read-only.

pub mod cod_selection;

use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use anyhow::{bail, ensure, Context, Result};
use chrono::{DateTime, Duration as ChronoDuration, SecondsFormat, Utc};
use minerals_public_catalog::{
    validate_public_catalog_release, PublicCatalogManifest, PUBLIC_CATALOG_FORMAT,
    PUBLIC_CATALOG_SCHEMA_VERSION,
};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    redirect::Policy,
    Client, Response,
};
use ring::digest::{Context as DigestContext, SHA256};
use rusqlite::{Connection, OpenFlags};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use tokio::time::{sleep, Instant};
use unicode_normalization::UnicodeNormalization;

pub const POPULATION_SNAPSHOT_FILE: &str = "population-snapshot.json";
pub const BASELINE_SELECTION_FILE: &str = "baseline-selection.json";
pub const QUERY_PLAN_FILE: &str = "cod-query-plan.json";
pub const PREPARATION_MANIFEST_FILE: &str = "preparation-manifest.json";
pub const DISCOVERY_INDEX_FILE: &str = "metadata-discovery-execution-index.json";

const PILOT_CONTRACT_PATH: &str = "schemas/pilots/cod-crystallography-pilot-v1.json";
const PUBLIC_CATALOG_PATH: &str = "public-catalog";
const PILOT_ID: &str = "cod-crystallography-v1";
const EXPECTED_POPULATION_COUNT: u64 = 6_226;
const EXPECTED_RELEASE_ID: &str =
    "sha256:684c2830a145bfa0f0b0ecaf4f80bac64ddb51ffcb4a75035304840a868bccc4";
const EXPECTED_DATABASE_SHA256: &str =
    "sha256:04f38354e7aec37d91466b88b6201997deae40ad53d5d76136f3efc8c45bca0c";
const EXPECTED_DATABASE_BYTES: u64 = 13_967_360;
const BASELINE_SALT: &str = "questionnaire-v1-baseline";
const BASELINE_DEVELOPMENT_COUNT: usize = 45;
const BASELINE_COUNT: usize = 60;
const EXPECTED_TOP_60_ID_LINES_SHA256: &str =
    "sha256:4f6dc5557611fc4f083c324d4c43b3b4ddbb4fe1016ea0dfcd791a1f9593ff74";
const EXPECTED_ALL_RANKED_ID_LINES_SHA256: &str =
    "sha256:84a3f9124a431baa1b01008d738377a089d3218fb0b6a37b0e248b939082ca60";
const DISCOVERY_ENDPOINT: &str = "https://www.crystallography.net/cod/result";
const REQUEST_COUNT: usize = 1_000;
const USER_AGENT: &str = "Waajacu-COD-Crystallography-Pilot/1 (+https://waajacu.org/)";
const PILOT_ROOT_RELATIVE: &str = "data/pilots/cod-crystallography-v1";
const OBJECTS_RELATIVE: &str = "objects/sha256";
const FETCH_LOCK_NAME: &str = ".metadata-discovery-fetch.lock";
const ATTEMPT_START_INTERVAL_MS: u64 = 12_000;
const CONNECT_TIMEOUT_MS: u64 = 15_000;
const ATTEMPT_TIMEOUT_MS: u64 = 180_000;
const MAX_ATTEMPTS: usize = 4;
const MAX_TRANSPORT_RECOVERIES: usize = 8;
const TRANSPORT_RECOVERY_WAIT_MS: i64 = 300_000;
const RETRY_BACKOFF_MS: [u64; 4] = [0, 12_000, 24_000, 48_000];
const RETRY_AFTER_CAP_MS: u64 = 300_000;
const MAX_RESPONSE_HEADER_BYTES: usize = 65_536;
const MAX_SUCCESS_RESPONSE_BYTES: usize = 134_217_728;
const MAX_ERROR_RESPONSE_BYTES: usize = 1_048_576;
const MAX_TOTAL_RAW_RESPONSE_BYTES: u64 = 8_589_934_592;
const MAX_JSON_DEPTH: usize = 64;
const MAX_ROWS_PER_SHARD: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparationManifest {
    format: String,
    schema_version: u32,
    pilot_id: String,
    execution_boundary: ExecutionBoundary,
    inputs: PreparationInputs,
    artifacts: PreparedArtifactSet,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecutionBoundary {
    network_access: String,
    mutable_registry_access: String,
    database_writes: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreparationInputs {
    pilot_contract: InputArtifact,
    public_catalog: PublicCatalogBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InputArtifact {
    path: String,
    sha256: String,
    bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicCatalogBinding {
    path: String,
    format: String,
    schema_version: u32,
    release_id: String,
    mineral_count: u64,
    database_path: String,
    database_sha256: String,
    database_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreparedArtifactSet {
    population_snapshot: ArtifactBinding,
    baseline_selection: ArtifactBinding,
    query_plan: ArtifactBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactBinding {
    path: String,
    sha256: String,
    bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PopulationSnapshot {
    format: String,
    schema_version: u32,
    pilot_id: String,
    source: PopulationSource,
    count: u64,
    entries: Vec<PopulationEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PopulationSource {
    catalog_format: String,
    catalog_schema_version: u32,
    release_id: String,
    database_sha256: String,
    database_bytes: u64,
    table: String,
    id_column: String,
    canonical_name_column: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PopulationEntry {
    material_public_id: String,
    canonical_name_snapshot: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BaselineSelection {
    format: String,
    schema_version: u32,
    pilot_id: String,
    population: BaselinePopulationBinding,
    selection_policy: BaselineSelectionPolicy,
    counts: BaselineCounts,
    entries: Vec<BaselineEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BaselinePopulationBinding {
    release_id: String,
    mineral_count: u64,
    snapshot_path: String,
    snapshot_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BaselineSelectionPolicy {
    version: String,
    salt: String,
    hash_input: String,
    ranking: String,
    development_ranks: String,
    holdout_ranks: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BaselineCounts {
    total: u32,
    development: u32,
    holdout: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BaselineEntry {
    position: u32,
    material_public_id: String,
    canonical_name_snapshot: String,
    selection_kind: String,
    split: String,
    selection_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct QueryPlan {
    format: String,
    schema_version: u32,
    pilot_id: String,
    plan_revision: u32,
    status: String,
    endpoint: String,
    request_digest_policy: RequestDigestPolicy,
    response_validation: CodResponseValidationPolicy,
    transport: FrozenTransport,
    limits: QueryLimits,
    requests: Vec<PlannedRequest>,
    safety_assertions: SafetyAssertions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CodResponseValidationPolicy {
    profile: String,
    top_level: String,
    text_encoding: String,
    byte_order_mark: String,
    duplicate_object_keys: String,
    depth: DepthValidationPolicy,
    rows: RowValidationPolicy,
    scalar_policy: String,
    file: FileValidationPolicy,
    svnrevision: RevisionValidationPolicy,
    unknown_row_fields: String,
    row_order: String,
    record_key: String,
    duplicate_record_keys: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DepthValidationPolicy {
    root_value_depth: u32,
    maximum: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RowValidationPolicy {
    maximum: u32,
    enforcement: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileValidationPolicy {
    canonical_form: String,
    request_prefix_match: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RevisionValidationPolicy {
    minimum: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestDigestPolicy {
    algorithm: String,
    canonicalization: String,
    input: String,
    prefix: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrozenTransport {
    scheme: String,
    host: String,
    port: u16,
    method: String,
    headers: BTreeMap<String, String>,
    accepted_success_content_types: Vec<String>,
    concurrency: u32,
    minimum_attempt_start_interval_ms: u64,
    connect_timeout_ms: u64,
    attempt_timeout_ms: u64,
    maximum_attempts_per_request: u32,
    attempt_backoff_ms: Vec<u64>,
    retry_after_cap_ms: u64,
    retryable_http_statuses: Vec<u16>,
    redirects: String,
    authentication: String,
    cookies: String,
    request_body: String,
    conditional_requests: String,
    jitter: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct QueryLimits {
    planned_requests: u32,
    maximum_response_header_bytes: u64,
    maximum_success_response_bytes: u64,
    maximum_error_response_bytes: u64,
    maximum_total_raw_response_bytes: u64,
    maximum_json_depth: u32,
    maximum_rows_per_shard: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SafetyAssertions {
    ordinals_are_contiguous_zero_through_999: bool,
    ddd_prefix_is_zero_padded_ordinal: bool,
    requests_are_in_ordinal_order: bool,
    every_url_matches_the_frozen_template: bool,
    request_digests_match_the_frozen_policy: bool,
    prefixes_form_a_complete_nonoverlapping_partition: bool,
    raw_response_bytes_are_immutable: bool,
    candidate_discovery_precedes_normalization: bool,
    normalized_output_is_forbidden: bool,
    catalog_and_registry_database_writes_are_forbidden: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlannedRequest {
    ordinal: u32,
    ddd_prefix: String,
    method: String,
    url: String,
    request_sha256: String,
}

#[derive(Debug)]
struct PilotBinding {
    file_sha256: String,
    file_bytes: u64,
    population_release_id: String,
    population_count: u64,
}

#[derive(Debug)]
struct PreparedArtifacts {
    population_bytes: Vec<u8>,
    baseline_bytes: Vec<u8>,
    query_plan_bytes: Vec<u8>,
    manifest: PreparationManifest,
    manifest_bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryExecutionIndex {
    format: String,
    schema_version: u32,
    pilot_id: String,
    index_revision: u64,
    status: String,
    started_at: String,
    updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    completed_at: Option<String>,
    plan: ExecutionPlanBinding,
    counts: ExecutionCounts,
    successful_receipts: Vec<SuccessfulReceipt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    halted_request: Option<HaltedRequest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    transport_recoveries: Vec<TransportRecovery>,
    #[serde(skip_serializing_if = "Option::is_none")]
    snapshot_identity: Option<SnapshotIdentity>,
    safety_assertions: ExecutionSafetyAssertions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecutionPlanBinding {
    path: String,
    sha256: String,
    request_count: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecutionCounts {
    planned_requests: u32,
    successful_requests: u32,
    next_ordinal: u32,
    attempts: u32,
    http_response_attempts: u32,
    transport_error_attempts: u32,
    response_limit_attempts: u32,
    raw_body_count: u32,
    raw_body_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SuccessfulReceipt {
    ordinal: u32,
    ddd_prefix: String,
    method: String,
    url: String,
    request_sha256: String,
    attempts: Vec<ExecutionAttempt>,
    successful_attempt_number: u32,
    response: SuccessfulResponse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SuccessfulResponse {
    status_code: u16,
    content_type: String,
    retrieved_at: String,
    raw_body: RawBodyBinding,
    json_validation: JsonValidation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonValidation {
    valid: bool,
    maximum_depth: u32,
    row_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecutionAttempt {
    attempt_number: u32,
    started_at: String,
    finished_at: String,
    elapsed_ms: u64,
    outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    status_code: Option<u16>,
    #[serde(
        default,
        deserialize_with = "deserialize_present_nullable_string",
        skip_serializing_if = "Option::is_none"
    )]
    content_type: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "deserialize_present_nullable_string",
        skip_serializing_if = "Option::is_none"
    )]
    retry_after: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    raw_body: Option<RawBodyBinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bytes_received: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_kind: Option<String>,
    retry_disposition: String,
}

/// Preserve the distinction between an absent field and an explicitly present
/// JSON `null`. Serde's default `Option<Option<T>>` deserializer collapses both
/// cases to `None`, but the execution contract requires nullable HTTP headers
/// to be present on every completed response and absent on transport failures.
fn deserialize_present_nullable_string<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<Option<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBodyBinding {
    path: String,
    sha256: String,
    bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HaltedRequest {
    ordinal: u32,
    ddd_prefix: String,
    method: String,
    url: String,
    request_sha256: String,
    reason: String,
    attempts: Vec<ExecutionAttempt>,
}

/// An explicit exception to the v1 per-request attempt limit, retained in v2
/// indices together with the exact index that exhausted its attempt budget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransportRecovery {
    reviewed_at: String,
    reviewer: String,
    reason: String,
    previous_index: RawBodyBinding,
    halted_request: HaltedRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotIdentity {
    algorithm: String,
    input: String,
    request_count: u32,
    sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecutionSafetyAssertions {
    plan_sha256_verified_before_execution: bool,
    successful_receipts_are_contiguous_from_ordinal_zero: bool,
    every_receipt_matches_its_plan_request: bool,
    attempt_numbers_are_contiguous_from_one: bool,
    successful_response_matches_the_final_attempt: bool,
    all_complete_http_response_bodies_are_content_addressed: bool,
    cas_paths_match_body_hashes: bool,
    counts_match_receipts_attempts_and_raw_bodies: bool,
    request_start_spacing_and_retry_policy_were_enforced: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reviewed_transport_recovery_policy_was_enforced: Option<bool>,
    raw_response_bytes_are_immutable: bool,
    candidate_discovery_precedes_normalization: bool,
    normalized_output_was_not_produced: bool,
    catalog_and_registry_databases_were_not_written: bool,
    complete_snapshot_identity_requires_all_1000_receipts: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResponseValidation {
    maximum_depth: usize,
    row_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ResponseValidationError {
    InvalidJson(String),
    JsonDepthLimit(usize),
    RowLimit(usize),
    InvalidSemantics(String),
}

impl std::fmt::Display for ResponseValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidJson(message) => write!(formatter, "invalid JSON: {message}"),
            Self::JsonDepthLimit(depth) => write!(formatter, "JSON depth {depth} exceeds limit"),
            Self::RowLimit(rows) => write!(formatter, "row count {rows} exceeds limit"),
            Self::InvalidSemantics(message) => {
                write!(formatter, "invalid COD row semantics: {message}")
            }
        }
    }
}

impl std::error::Error for ResponseValidationError {}

#[derive(Debug)]
struct RankedPopulationEntry {
    digest: [u8; 32],
    entry: PopulationEntry,
}

#[derive(Debug)]
struct ExecutionContext {
    repo_root: PathBuf,
    pilot_root: PathBuf,
    plan: QueryPlan,
    plan_binding: ExecutionPlanBinding,
}

#[derive(Debug)]
struct FetchLock {
    path: PathBuf,
}

impl FetchLock {
    fn acquire(pilot_root: &Path) -> Result<Self> {
        let path = pilot_root.join(FETCH_LOCK_NAME);
        match fs::create_dir(&path) {
            Ok(()) => Ok(Self { path }),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => bail!(
                "COD metadata fetch lock already exists (a concurrent or interrupted fetch must be reviewed): {}",
                path.display()
            ),
            Err(error) => Err(error)
                .with_context(|| format!("failed to acquire fetch lock: {}", path.display())),
        }
    }
}

impl Drop for FetchLock {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.path);
    }
}

#[derive(Debug)]
enum AttemptDecision {
    Success {
        attempt: ExecutionAttempt,
        response: SuccessfulResponse,
    },
    Retry(ExecutionAttempt),
    Halt {
        attempt: ExecutionAttempt,
        reason: String,
    },
}

#[derive(Debug)]
enum BoundedBody {
    Complete(Vec<u8>),
    LimitExceeded(u64),
}

/// Prepare all deterministic, offline pilot inputs in a new output directory.
///
/// The output directory must not already exist. No network access is performed,
/// the private registry is never opened, and the public catalog is opened only
/// in SQLite read-only/query-only mode.
pub fn prepare(repo_root: &Path, output: &Path) -> Result<PreparationManifest> {
    let prepared = build_prepared_artifacts(repo_root)?;
    create_new_output_directory(output)?;

    write_new_file(output, POPULATION_SNAPSHOT_FILE, &prepared.population_bytes)?;
    write_new_file(output, BASELINE_SELECTION_FILE, &prepared.baseline_bytes)?;
    write_new_file(output, QUERY_PLAN_FILE, &prepared.query_plan_bytes)?;
    // The manifest is the final commit point for a complete preparation.
    write_new_file(output, PREPARATION_MANIFEST_FILE, &prepared.manifest_bytes)?;
    Ok(prepared.manifest)
}

/// Verify an offline preparation byte-for-byte against the committed contract
/// and catalog, while also re-running every semantic invariant.
pub fn verify(repo_root: &Path, input: &Path) -> Result<PreparationManifest> {
    let input = require_real_directory(input, "prepared artifact directory")?;
    validate_prepared_directory_entries(&input)?;
    let expected = build_prepared_artifacts(repo_root)?;

    verify_canonical_file(
        &input.join(POPULATION_SNAPSHOT_FILE),
        &expected.population_bytes,
        &expected.manifest.artifacts.population_snapshot,
    )?;
    verify_canonical_file(
        &input.join(BASELINE_SELECTION_FILE),
        &expected.baseline_bytes,
        &expected.manifest.artifacts.baseline_selection,
    )?;
    verify_canonical_file(
        &input.join(QUERY_PLAN_FILE),
        &expected.query_plan_bytes,
        &expected.manifest.artifacts.query_plan,
    )?;

    let manifest_path = input.join(PREPARATION_MANIFEST_FILE);
    let actual_manifest_bytes = read_regular_file(&manifest_path, "preparation manifest")?;
    ensure_canonical_json_file(&actual_manifest_bytes, "preparation manifest")?;
    let actual_manifest: PreparationManifest = serde_json::from_slice(&actual_manifest_bytes)
        .context("failed to parse preparation manifest")?;
    if actual_manifest != expected.manifest || actual_manifest_bytes != expected.manifest_bytes {
        bail!("preparation manifest does not match the committed pilot inputs");
    }
    Ok(actual_manifest)
}

/// Verify an existing metadata-discovery execution index and every referenced
/// content-addressed response body without performing network access.
pub fn verify_execution(
    repo_root: &Path,
    prepared: &Path,
    pilot_root: &Path,
) -> Result<DiscoveryExecutionIndex> {
    let context = load_execution_context(repo_root, prepared, pilot_root)?;
    let index_path = context.pilot_root.join(DISCOVERY_INDEX_FILE);
    let index = load_execution_index(&index_path)?
        .context("metadata discovery has no execution index to verify")?;
    validate_execution_index(&context, &index)?;
    Ok(index)
}

/// Offline, reviewed recovery for exhausted connection failures only. This
/// preserves the prior index in CAS before granting one additional four-attempt
/// cycle. It neither retrieves data nor edits the frozen query/preparation.
pub fn recover_transport(
    repo_root: &Path,
    prepared: &Path,
    pilot_root: &Path,
    reviewer: &str,
    reason: &str,
) -> Result<DiscoveryExecutionIndex> {
    validate_review_text(reviewer, 120, "recovery reviewer")?;
    validate_review_text(reason, 2_000, "recovery reason")?;
    let context = load_execution_context(repo_root, prepared, pilot_root)?;
    let _fetch_lock = FetchLock::acquire(&context.pilot_root)?;
    let index_path = context.pilot_root.join(DISCOVERY_INDEX_FILE);
    let index = load_execution_index(&index_path)?.context("no execution index to recover")?;
    let recovered =
        reviewed_transport_recovery(&context, &index, reviewer, reason, timestamp_now())?;
    persist_execution_index(&context, &index_path, recovered)
}

fn reviewed_transport_recovery(
    context: &ExecutionContext,
    index: &DiscoveryExecutionIndex,
    reviewer: &str,
    reason: &str,
    reviewed_at: String,
) -> Result<DiscoveryExecutionIndex> {
    validate_execution_index(context, index)?;
    validate_review_text(reviewer, 120, "recovery reviewer")?;
    validate_review_text(reason, 2_000, "recovery reason")?;
    ensure!(
        index.transport_recoveries.len() < MAX_TRANSPORT_RECOVERIES,
        "reviewed recovery limit reached"
    );
    let halted = index
        .halted_request
        .as_ref()
        .context("execution has no halted request")?;
    validate_transport_recovery_halt(halted, &reviewed_at)?;
    ensure!(
        index
            .transport_recoveries
            .iter()
            .all(|review| review.halted_request.ordinal != halted.ordinal),
        "this shard has already received its reviewed recovery cycle"
    );
    let previous_index = store_raw_body(context, &canonical_file_bytes(index)?)?;
    let mut recovered = index.clone();
    recovered.schema_version = 2;
    recovered.transport_recoveries.push(TransportRecovery {
        reviewed_at,
        reviewer: reviewer.to_string(),
        reason: reason.to_string(),
        previous_index,
        halted_request: halted.clone(),
    });
    recovered.halted_request = None;
    recovered.safety_assertions = recovered_execution_safety_assertions();
    recovered.counts = recompute_execution_counts(&recovered)?;
    validate_execution_index(context, &recovered)?;
    Ok(recovered)
}

fn validate_review_text(value: &str, maximum: usize, label: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.trim() == value
            && value.chars().count() <= maximum
            && !value.chars().any(char::is_control)
            && value.nfc().eq(value.chars()),
        "{label} must be trimmed NFC text without control characters (1–{maximum} characters)"
    );
    Ok(())
}

fn validate_transport_recovery_halt(halted: &HaltedRequest, reviewed_at: &str) -> Result<()> {
    ensure!(
        halted.reason == "transport_error"
            && halted.attempts.len() == MAX_ATTEMPTS
            && halted.attempts.iter().all(|attempt| {
                attempt.outcome == "transport_error"
                    && matches!(attempt.error_kind.as_deref(), Some("connect" | "timeout"))
                    && attempt.status_code.is_none()
                    && attempt.raw_body.is_none()
                    && attempt.content_type.is_none()
                    && attempt.retry_after.is_none()
                    && attempt.bytes_received.is_none()
            }),
        "recovery requires four completed connection/timeout failures without an HTTP response"
    );
    let finished = parse_timestamp(
        &halted.attempts.last().unwrap().finished_at,
        "failed cycle finished_at",
    )?;
    ensure!(
        parse_timestamp(reviewed_at, "recovery reviewed_at")?
            .signed_duration_since(finished)
            .num_milliseconds()
            >= TRANSPORT_RECOVERY_WAIT_MS,
        "reviewed recovery requires at least a five-minute cooldown"
    );
    Ok(())
}

/// Fetch COD metadata in the exact frozen query-plan order.
///
/// Existing receipts and CAS objects are fully revalidated before any request
/// starts. `max_new_requests` bounds newly processed ordinals for deliberate,
/// resumable runs; omission continues until completion or a blocking response.
pub async fn fetch(
    repo_root: &Path,
    prepared: &Path,
    pilot_root: &Path,
    max_new_requests: Option<u32>,
) -> Result<DiscoveryExecutionIndex> {
    ensure!(
        max_new_requests != Some(0),
        "max_new_requests must be between 1 and {REQUEST_COUNT}"
    );
    let context = load_execution_context(repo_root, prepared, pilot_root)?;
    let _fetch_lock = FetchLock::acquire(&context.pilot_root)?;
    let index_path = context.pilot_root.join(DISCOVERY_INDEX_FILE);
    let mut index = match load_execution_index(&index_path)? {
        Some(index) => {
            validate_execution_index(&context, &index)?;
            index
        }
        None => {
            let index = new_execution_index(context.plan_binding.clone());
            persist_execution_index(&context, &index_path, index)?
        }
    };
    if index.status == "complete" {
        return Ok(index);
    }

    let request_limit = max_new_requests.unwrap_or(REQUEST_COUNT as u32) as usize;
    ensure!(
        request_limit <= REQUEST_COUNT,
        "max_new_requests cannot exceed {REQUEST_COUNT}"
    );
    let mut resumed_attempts = Vec::new();
    if let Some(halted) = index.halted_request.take() {
        ensure!(
            halted.reason == "operator_interrupt" && halted.attempts.len() < MAX_ATTEMPTS,
            "execution is halted at ordinal {} for {}; manual review is required",
            halted.ordinal,
            halted.reason
        );
        resumed_attempts = halted.attempts;
    }

    let client = build_http_client(&context.plan)?;
    let mut newly_processed = 0_usize;
    while index.successful_receipts.len() < REQUEST_COUNT && newly_processed < request_limit {
        let ordinal = index.successful_receipts.len();
        let request = context.plan.requests[ordinal].clone();
        let mut attempts = if !resumed_attempts.is_empty() {
            std::mem::take(&mut resumed_attempts)
        } else {
            Vec::new()
        };
        newly_processed += 1;

        loop {
            let attempt_number = attempts.len() + 1;
            ensure!(
                attempt_number <= MAX_ATTEMPTS,
                "request ordinal {ordinal} exhausted its frozen attempt limit"
            );
            wait_for_attempt_start(&index, &attempts).await?;
            let reservation = reserved_attempt(attempt_number);
            attempts.push(reservation);
            let reservation_reason = if attempt_number < MAX_ATTEMPTS {
                "operator_interrupt"
            } else {
                "transport_error"
            };
            index.halted_request = Some(halted_request(
                &request,
                reservation_reason,
                attempts.clone(),
            ));
            index = persist_execution_index(&context, &index_path, index)?;
            index.halted_request = None;
            attempts
                .pop()
                .expect("the just-persisted reservation exists");
            let decision = perform_attempt(
                &client,
                &context,
                &request,
                attempt_number,
                index.counts.raw_body_bytes,
            )
            .await?;
            match decision {
                AttemptDecision::Success { attempt, response } => {
                    attempts.push(attempt);
                    let successful_attempt_number = attempts.len() as u32;
                    index.successful_receipts.push(SuccessfulReceipt {
                        ordinal: request.ordinal,
                        ddd_prefix: request.ddd_prefix.clone(),
                        method: request.method.clone(),
                        url: request.url.clone(),
                        request_sha256: request.request_sha256.clone(),
                        attempts,
                        successful_attempt_number,
                        response,
                    });
                    index.halted_request = None;
                    if index.successful_receipts.len() == REQUEST_COUNT {
                        let completed_at = timestamp_now();
                        index.status = "complete".to_string();
                        index.completed_at = Some(completed_at);
                        index.snapshot_identity = Some(execution_snapshot_identity(
                            &index.plan.sha256,
                            &index.successful_receipts,
                            &index.transport_recoveries,
                        )?);
                    }
                    index = persist_execution_index(&context, &index_path, index)?;
                    break;
                }
                AttemptDecision::Retry(attempt) => {
                    attempts.push(attempt);
                    index.halted_request = Some(halted_request(
                        &request,
                        "operator_interrupt",
                        attempts.clone(),
                    ));
                    index = persist_execution_index(&context, &index_path, index)?;
                    // The durable index intentionally remains resumable if the
                    // process stops during the following delay or request.
                    index.halted_request = None;
                }
                AttemptDecision::Halt { attempt, reason } => {
                    attempts.push(attempt);
                    index.halted_request = Some(halted_request(&request, &reason, attempts));
                    let _ = persist_execution_index(&context, &index_path, index)?;
                    bail!(
                        "metadata discovery halted at ordinal {}: {} (index preserved at {})",
                        request.ordinal,
                        reason,
                        index_path.display()
                    );
                }
            }
        }
    }
    validate_execution_index(&context, &index)?;
    Ok(index)
}

fn load_execution_context(
    repo_root: &Path,
    prepared: &Path,
    pilot_root: &Path,
) -> Result<ExecutionContext> {
    let repo_root = require_real_directory(repo_root, "repository root")?;
    let prepared = resolve_repo_argument(&repo_root, prepared);
    let prepared_root = require_real_directory(&prepared, "prepared artifact directory")?;
    let pilot_argument = resolve_repo_argument(&repo_root, pilot_root);
    let expected_pilot_root = repo_root.join(PILOT_ROOT_RELATIVE);
    ensure!(
        normalize_location(&pilot_argument)? == normalize_location(&expected_pilot_root)?,
        "--pilot-root must resolve exactly to {PILOT_ROOT_RELATIVE}"
    );
    let pilot_root = require_real_directory(&expected_pilot_root, "COD pilot root")?;
    ensure!(
        prepared_root.starts_with(&pilot_root),
        "prepared artifacts used for fetch must be inside {PILOT_ROOT_RELATIVE}"
    );

    let preparation = verify(&repo_root, &prepared_root)?;
    let plan_path = prepared_root.join(QUERY_PLAN_FILE);
    let plan_bytes = read_regular_file(&plan_path, "prepared query plan")?;
    ensure_canonical_json_file(&plan_bytes, "prepared query plan")?;
    let plan: QueryPlan =
        serde_json::from_slice(&plan_bytes).context("failed to parse prepared COD query plan")?;
    validate_query_plan_runtime(&plan)?;
    let plan_relative = repo_relative_path(&repo_root, &plan_path)?;
    ensure!(
        plan_relative.starts_with(&format!("{PILOT_ROOT_RELATIVE}/"))
            && plan_relative.contains("query-plan")
            && plan_relative.ends_with(".json"),
        "prepared query-plan path is incompatible with the execution-index contract"
    );
    let artifact = &preparation.artifacts.query_plan;
    ensure!(
        artifact.sha256 == sha256_identifier(&plan_bytes)
            && artifact.bytes == plan_bytes.len() as u64,
        "prepared query-plan binding drifted"
    );
    Ok(ExecutionContext {
        repo_root,
        pilot_root,
        plan,
        plan_binding: ExecutionPlanBinding {
            path: plan_relative,
            sha256: artifact.sha256.clone(),
            request_count: REQUEST_COUNT as u32,
        },
    })
}

fn validate_query_plan_runtime(plan: &QueryPlan) -> Result<()> {
    ensure!(
        plan.format == "waajacu-cod-metadata-discovery-query-plan"
            && plan.schema_version == 1
            && plan.pilot_id == PILOT_ID
            && plan.plan_revision == 1
            && plan.status == "frozen_before_retrieval"
            && plan.endpoint == DISCOVERY_ENDPOINT,
        "prepared query plan has incompatible identity fields"
    );
    ensure!(
        plan.response_validation == cod_response_validation_policy(),
        "prepared query plan response-validation profile drifted"
    );
    ensure!(
        plan.transport.scheme == "https"
            && plan.transport.host == "www.crystallography.net"
            && plan.transport.port == 443
            && plan.transport.method == "GET"
            && plan.transport.concurrency == 1
            && plan.transport.minimum_attempt_start_interval_ms == ATTEMPT_START_INTERVAL_MS
            && plan.transport.connect_timeout_ms == CONNECT_TIMEOUT_MS
            && plan.transport.attempt_timeout_ms == ATTEMPT_TIMEOUT_MS
            && plan.transport.maximum_attempts_per_request == MAX_ATTEMPTS as u32
            && plan.transport.attempt_backoff_ms == RETRY_BACKOFF_MS
            && plan.transport.retry_after_cap_ms == RETRY_AFTER_CAP_MS
            && plan.transport.redirects == "forbidden",
        "prepared query plan transport drifted"
    );
    ensure!(
        plan.limits.planned_requests == REQUEST_COUNT as u32
            && plan.limits.maximum_response_header_bytes == MAX_RESPONSE_HEADER_BYTES as u64
            && plan.limits.maximum_success_response_bytes == MAX_SUCCESS_RESPONSE_BYTES as u64
            && plan.limits.maximum_error_response_bytes == MAX_ERROR_RESPONSE_BYTES as u64
            && plan.limits.maximum_total_raw_response_bytes == MAX_TOTAL_RAW_RESPONSE_BYTES
            && plan.limits.maximum_json_depth == MAX_JSON_DEPTH as u32
            && plan.limits.maximum_rows_per_shard == MAX_ROWS_PER_SHARD as u32,
        "prepared query plan limits drifted"
    );
    validate_query_plan_anchors(&plan.requests)
}

fn resolve_repo_argument(repo_root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo_root.join(path)
    }
}

fn normalize_location(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return path
            .canonicalize()
            .with_context(|| format!("failed to canonicalize path: {}", path.display()));
    }
    let name = path
        .file_name()
        .context("path must have a final component")?;
    let parent = path.parent().context("path must have a parent")?;
    Ok(parent
        .canonicalize()
        .with_context(|| format!("failed to canonicalize parent: {}", parent.display()))?
        .join(name))
}

fn repo_relative_path(repo_root: &Path, path: &Path) -> Result<String> {
    let path = path
        .canonicalize()
        .with_context(|| format!("failed to canonicalize path: {}", path.display()))?;
    let relative = path
        .strip_prefix(repo_root)
        .context("path is outside the repository root")?;
    let components = relative
        .components()
        .map(|component| match component {
            Component::Normal(value) => value
                .to_str()
                .context("repository-relative path is not Unicode"),
            _ => bail!("repository-relative path contains an unsafe component"),
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(components.join("/"))
}

fn new_execution_index(plan: ExecutionPlanBinding) -> DiscoveryExecutionIndex {
    let now = timestamp_now();
    DiscoveryExecutionIndex {
        format: "waajacu-cod-metadata-discovery-execution-index".to_string(),
        schema_version: 1,
        pilot_id: PILOT_ID.to_string(),
        index_revision: 0,
        status: "partial".to_string(),
        started_at: now.clone(),
        updated_at: now,
        completed_at: None,
        plan,
        counts: ExecutionCounts {
            planned_requests: REQUEST_COUNT as u32,
            ..ExecutionCounts::default()
        },
        successful_receipts: Vec::new(),
        halted_request: None,
        transport_recoveries: Vec::new(),
        snapshot_identity: None,
        safety_assertions: execution_safety_assertions(),
    }
}

fn execution_safety_assertions() -> ExecutionSafetyAssertions {
    ExecutionSafetyAssertions {
        plan_sha256_verified_before_execution: true,
        successful_receipts_are_contiguous_from_ordinal_zero: true,
        every_receipt_matches_its_plan_request: true,
        attempt_numbers_are_contiguous_from_one: true,
        successful_response_matches_the_final_attempt: true,
        all_complete_http_response_bodies_are_content_addressed: true,
        cas_paths_match_body_hashes: true,
        counts_match_receipts_attempts_and_raw_bodies: true,
        request_start_spacing_and_retry_policy_were_enforced: true,
        reviewed_transport_recovery_policy_was_enforced: None,
        raw_response_bytes_are_immutable: true,
        candidate_discovery_precedes_normalization: true,
        normalized_output_was_not_produced: true,
        catalog_and_registry_databases_were_not_written: true,
        complete_snapshot_identity_requires_all_1000_receipts: true,
    }
}

fn recovered_execution_safety_assertions() -> ExecutionSafetyAssertions {
    let mut assertions = execution_safety_assertions();
    // Recovery is an explicitly recorded exception to the original four
    // attempts per request. Do not claim that original limit still holds.
    assertions.request_start_spacing_and_retry_policy_were_enforced = false;
    assertions.reviewed_transport_recovery_policy_was_enforced = Some(true);
    assertions
}

fn load_execution_index(path: &Path) -> Result<Option<DiscoveryExecutionIndex>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink(),
                "execution index must be a regular non-symlink file: {}",
                path.display()
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect execution index: {}", path.display()))
        }
    }
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read execution index: {}", path.display()))?;
    ensure_canonical_json_file(&bytes, "metadata discovery execution index")?;
    let index = serde_json::from_slice(&bytes)
        .context("failed to parse metadata discovery execution index")?;
    Ok(Some(index))
}

fn persist_execution_index(
    context: &ExecutionContext,
    path: &Path,
    mut index: DiscoveryExecutionIndex,
) -> Result<DiscoveryExecutionIndex> {
    index.counts = recompute_execution_counts(&index)?;
    index.updated_at = timestamp_now();
    index.index_revision = index
        .index_revision
        .checked_add(1)
        .context("execution index revision overflow")?;
    ensure!(
        index.counts.raw_body_bytes <= MAX_TOTAL_RAW_RESPONSE_BYTES,
        "execution index exceeds total raw-body limit"
    );
    ensure!(
        index.plan == context.plan_binding,
        "execution index plan binding drifted"
    );
    let bytes = canonical_file_bytes(&index)?;
    atomic_replace_file(path, &bytes, index.index_revision)?;
    Ok(index)
}

fn atomic_replace_file(path: &Path, bytes: &[u8], revision: u64) -> Result<()> {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .context("managed index path has no Unicode file name")?;
    let nonce = INDEX_TEMP_NONCE.fetch_add(1, Ordering::Relaxed);
    let temporary = path.with_file_name(format!(
        ".{file_name}.tmp.{}.{}.{}",
        std::process::id(),
        revision,
        nonce
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .with_context(|| {
            format!(
                "failed to create index temporary file: {}",
                temporary.display()
            )
        })?;
    file.write_all(bytes).with_context(|| {
        format!(
            "failed to write index temporary file: {}",
            temporary.display()
        )
    })?;
    file.sync_all().with_context(|| {
        format!(
            "failed to synchronize index temporary file: {}",
            temporary.display()
        )
    })?;
    drop(file);
    match replace_file(&temporary, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(error).with_context(|| {
                format!(
                    "failed to publish execution index {} from {}",
                    path.display(),
                    temporary.display()
                )
            })
        }
    }
}

static INDEX_TEMP_NONCE: AtomicU64 = AtomicU64::new(0);

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let moved = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn recompute_execution_counts(index: &DiscoveryExecutionIndex) -> Result<ExecutionCounts> {
    let mut counts = ExecutionCounts {
        planned_requests: REQUEST_COUNT as u32,
        successful_requests: index.successful_receipts.len() as u32,
        next_ordinal: index.successful_receipts.len() as u32,
        ..ExecutionCounts::default()
    };
    for attempt in index
        .successful_receipts
        .iter()
        .flat_map(|receipt| receipt.attempts.iter())
        .chain(
            index
                .halted_request
                .iter()
                .flat_map(|halted| halted.attempts.iter()),
        )
        .chain(
            index
                .transport_recoveries
                .iter()
                .flat_map(|review| review.halted_request.attempts.iter()),
        )
    {
        counts.attempts = counts
            .attempts
            .checked_add(1)
            .context("attempt count overflow")?;
        match attempt.outcome.as_str() {
            "http_response" => counts.http_response_attempts += 1,
            "transport_error" => counts.transport_error_attempts += 1,
            "response_limit_exceeded" => counts.response_limit_attempts += 1,
            other => bail!("unknown attempt outcome {other}"),
        }
        if let Some(raw_body) = &attempt.raw_body {
            counts.raw_body_count += 1;
            counts.raw_body_bytes = counts
                .raw_body_bytes
                .checked_add(raw_body.bytes)
                .context("raw-body byte count overflow")?;
        }
    }
    Ok(counts)
}

fn validate_execution_index(
    context: &ExecutionContext,
    index: &DiscoveryExecutionIndex,
) -> Result<()> {
    ensure!(
        index.format == "waajacu-cod-metadata-discovery-execution-index"
            && matches!(index.schema_version, 1 | 2)
            && index.pilot_id == PILOT_ID
            && index.index_revision >= 1,
        "execution index identity fields are invalid"
    );
    ensure!(
        index.plan == context.plan_binding,
        "execution index plan binding drifted"
    );
    parse_timestamp(&index.started_at, "index started_at")?;
    parse_timestamp(&index.updated_at, "index updated_at")?;
    ensure!(
        (index.schema_version == 1 && index.transport_recoveries.is_empty())
            || (index.schema_version == 2
                && (1..=MAX_TRANSPORT_RECOVERIES).contains(&index.transport_recoveries.len())),
        "execution version does not match its reviewed recovery history"
    );
    ensure!(
        index.safety_assertions
            == if index.schema_version == 1 {
                execution_safety_assertions()
            } else {
                recovered_execution_safety_assertions()
            },
        "execution index safety assertions drifted"
    );
    ensure!(
        index.successful_receipts.len() <= REQUEST_COUNT,
        "execution index has too many successful receipts"
    );

    validate_recovery_archives(context, index)?;
    let mut previous_start: Option<DateTime<Utc>> = None;
    for (ordinal, receipt) in index.successful_receipts.iter().enumerate() {
        validate_recovery_before_ordinal(
            context,
            index,
            ordinal,
            receipt.attempts.first(),
            &mut previous_start,
        )?;
        validate_receipt(context, receipt, ordinal, &mut previous_start)?;
    }
    validate_recovery_before_ordinal(
        context,
        index,
        index.successful_receipts.len(),
        index
            .halted_request
            .as_ref()
            .and_then(|halted| halted.attempts.first()),
        &mut previous_start,
    )?;
    if let Some(halted) = &index.halted_request {
        ensure!(
            index.status == "partial"
                && (halted.ordinal as usize) < REQUEST_COUNT
                && halted.ordinal as usize == index.successful_receipts.len(),
            "halted request is not the next planned ordinal"
        );
        let planned = &context.plan.requests[halted.ordinal as usize];
        ensure!(
            halted.ddd_prefix == planned.ddd_prefix
                && halted.method == planned.method
                && halted.url == planned.url
                && halted.request_sha256 == planned.request_sha256,
            "halted request does not match its planned request"
        );
        ensure!(
            (1..=MAX_ATTEMPTS).contains(&halted.attempts.len()),
            "halted request has an invalid attempt count"
        );
        validate_attempts(context, &halted.attempts, &mut previous_start)?;
        validate_halted_request(halted)?;
    }

    ensure!(
        recompute_execution_counts(index)? == index.counts,
        "execution index counts do not match receipts and attempts"
    );
    if index.status == "complete" {
        ensure!(
            index.successful_receipts.len() == REQUEST_COUNT
                && index.halted_request.is_none()
                && index.completed_at.is_some()
                && index.snapshot_identity.is_some(),
            "complete execution index is incomplete"
        );
        parse_timestamp(index.completed_at.as_deref().unwrap(), "index completed_at")?;
        ensure!(
            index.snapshot_identity.as_ref()
                == Some(&execution_snapshot_identity(
                    &index.plan.sha256,
                    &index.successful_receipts,
                    &index.transport_recoveries,
                )?),
            "complete snapshot identity drifted"
        );
    } else {
        ensure!(
            index.status == "partial"
                && index.completed_at.is_none()
                && index.snapshot_identity.is_none(),
            "partial execution index contains complete-only fields"
        );
    }
    Ok(())
}

fn validate_recovery_archives(
    context: &ExecutionContext,
    index: &DiscoveryExecutionIndex,
) -> Result<()> {
    let mut previous_ordinal = None;
    for (position, review) in index.transport_recoveries.iter().enumerate() {
        let ordinal = review.halted_request.ordinal as usize;
        ensure!(
            ordinal <= index.successful_receipts.len() && ordinal < REQUEST_COUNT,
            "recovery ordinal is outside the saved prefix"
        );
        ensure!(
            previous_ordinal.is_none_or(|previous| ordinal > previous),
            "only one recovery per shard is permitted, in ordinal order"
        );
        previous_ordinal = Some(ordinal);
        validate_review_text(&review.reviewer, 120, "recovery reviewer")?;
        validate_review_text(&review.reason, 2_000, "recovery reason")?;
        validate_transport_recovery_halt(&review.halted_request, &review.reviewed_at)?;
        ensure!(
            review.previous_index.bytes <= 16_777_216,
            "archived execution index exceeds 16MiB"
        );
        let bytes = read_and_validate_raw_body(context, &review.previous_index)?;
        ensure_canonical_json_file(&bytes, "archived execution index")?;
        let previous: DiscoveryExecutionIndex =
            serde_json::from_slice(&bytes).context("invalid archived execution index")?;
        ensure!(
            previous.format == index.format
                && previous.pilot_id == index.pilot_id
                && previous.schema_version == if position == 0 { 1 } else { 2 }
                && previous.index_revision <= index.index_revision
                && previous.status == "partial"
                && previous.completed_at.is_none()
                && previous.snapshot_identity.is_none()
                && previous.started_at == index.started_at
                && previous.plan == index.plan
                && previous.successful_receipts == index.successful_receipts[..ordinal]
                && previous.transport_recoveries == index.transport_recoveries[..position]
                && previous.halted_request.as_ref() == Some(&review.halted_request)
                && previous.counts == recompute_execution_counts(&previous)?
                && previous.safety_assertions
                    == if position == 0 {
                        execution_safety_assertions()
                    } else {
                        recovered_execution_safety_assertions()
                    },
            "archived execution index does not match the retained failure and history"
        );
        ensure!(
            parse_timestamp(&previous.updated_at, "archived index updated_at")?
                <= parse_timestamp(&review.reviewed_at, "recovery reviewed_at")?,
            "review precedes archived index"
        );
    }
    Ok(())
}

fn validate_recovery_before_ordinal(
    context: &ExecutionContext,
    index: &DiscoveryExecutionIndex,
    ordinal: usize,
    next_attempt: Option<&ExecutionAttempt>,
    previous_start: &mut Option<DateTime<Utc>>,
) -> Result<()> {
    if let Some(review) = index
        .transport_recoveries
        .iter()
        .find(|review| review.halted_request.ordinal as usize == ordinal)
    {
        let halted = &review.halted_request;
        let planned = &context.plan.requests[ordinal];
        ensure!(
            halted == &halted_request(planned, "transport_error", halted.attempts.clone()),
            "recovered failure does not match the frozen request"
        );
        validate_attempts(context, &halted.attempts, previous_start)?;
        validate_halted_request(halted)?;
        if let Some(next) = next_attempt {
            ensure!(
                parse_timestamp(&next.started_at, "recovered attempt started_at")?
                    >= parse_timestamp(&review.reviewed_at, "recovery reviewed_at")?,
                "retrieval precedes its recovery review"
            );
        }
    }
    Ok(())
}

fn validate_receipt(
    context: &ExecutionContext,
    receipt: &SuccessfulReceipt,
    ordinal: usize,
    previous_start: &mut Option<DateTime<Utc>>,
) -> Result<()> {
    let planned = &context.plan.requests[ordinal];
    ensure!(
        receipt.ordinal as usize == ordinal
            && receipt.ddd_prefix == planned.ddd_prefix
            && receipt.method == planned.method
            && receipt.url == planned.url
            && receipt.request_sha256 == planned.request_sha256,
        "successful receipt does not match plan ordinal {ordinal}"
    );
    ensure!(
        (1..=MAX_ATTEMPTS).contains(&receipt.attempts.len())
            && receipt.successful_attempt_number as usize == receipt.attempts.len(),
        "successful receipt has invalid attempt numbering at ordinal {ordinal}"
    );
    validate_attempts(context, &receipt.attempts, previous_start)?;
    let final_attempt = receipt.attempts.last().unwrap();
    ensure!(
        final_attempt.outcome == "http_response"
            && final_attempt.status_code == Some(200)
            && final_attempt.retry_disposition == "success"
            && final_attempt.raw_body.as_ref() == Some(&receipt.response.raw_body),
        "successful response does not match final attempt at ordinal {ordinal}"
    );
    ensure!(
        receipt.response.status_code == 200
            && ["application/json", "text/json", "text/plain"]
                .contains(&receipt.response.content_type.as_str()),
        "successful response content metadata is invalid at ordinal {ordinal}"
    );
    parse_timestamp(&receipt.response.retrieved_at, "response retrieved_at")?;
    ensure!(
        receipt.response.retrieved_at == final_attempt.finished_at,
        "successful response retrieval timestamp does not match its final attempt"
    );
    let final_content_type = final_attempt
        .content_type
        .as_ref()
        .and_then(|value| value.as_deref());
    ensure!(
        content_type_essence(final_content_type).as_deref()
            == Some(receipt.response.content_type.as_str()),
        "successful response content type does not match its final attempt"
    );
    let bytes = read_and_validate_raw_body(context, &receipt.response.raw_body)?;
    let validation = validate_cod_response(&bytes, &receipt.ddd_prefix)
        .map_err(|error| anyhow::anyhow!(error))?;
    ensure!(
        receipt.response.json_validation
            == (JsonValidation {
                valid: true,
                maximum_depth: validation.maximum_depth as u32,
                row_count: validation.row_count as u32,
            }),
        "stored JSON validation drifted at ordinal {ordinal}"
    );
    Ok(())
}

fn validate_attempts(
    context: &ExecutionContext,
    attempts: &[ExecutionAttempt],
    previous_start: &mut Option<DateTime<Utc>>,
) -> Result<()> {
    for (position, attempt) in attempts.iter().enumerate() {
        validate_attempt(context, attempt, position + 1, previous_start)?;
        if let Some(previous) = position
            .checked_sub(1)
            .and_then(|index| attempts.get(index))
        {
            validate_retry_wait(previous, attempt)?;
        }
        if position + 1 < attempts.len() {
            ensure!(
                matches!(
                    attempt.retry_disposition.as_str(),
                    "retry_fixed_backoff" | "retry_retry_after"
                ),
                "a non-final attempt does not authorize the next retry"
            );
        }
    }
    Ok(())
}

fn validate_attempt(
    context: &ExecutionContext,
    attempt: &ExecutionAttempt,
    expected_number: usize,
    previous_start: &mut Option<DateTime<Utc>>,
) -> Result<()> {
    ensure!(
        [
            "success",
            "retry_fixed_backoff",
            "retry_retry_after",
            "stop_nonretryable_status",
            "stop_attempt_limit",
            "stop_retry_after_cap",
            "stop_response_header_limit",
            "stop_response_limit",
            "stop_transport_error",
            "stop_invalid_content_encoding",
            "stop_invalid_content_type",
            "stop_invalid_json",
            "stop_invalid_semantics",
            "stop_json_depth_limit",
            "stop_row_limit",
            "stop_total_limit",
        ]
        .contains(&attempt.retry_disposition.as_str()),
        "attempt has an unknown retry disposition"
    );
    ensure!(
        attempt.attempt_number as usize == expected_number
            && attempt.elapsed_ms <= ATTEMPT_TIMEOUT_MS,
        "attempt numbering or elapsed time is invalid"
    );
    let started = parse_timestamp(&attempt.started_at, "attempt started_at")?;
    let finished = parse_timestamp(&attempt.finished_at, "attempt finished_at")?;
    ensure!(finished >= started, "attempt finishes before it starts");
    if let Some(previous) = previous_start {
        ensure!(
            started.signed_duration_since(*previous).num_milliseconds()
                >= ATTEMPT_START_INTERVAL_MS as i64,
            "attempt start spacing is below 12000ms"
        );
    }
    *previous_start = Some(started);
    match attempt.outcome.as_str() {
        "http_response" => {
            ensure!(
                attempt.status_code.is_some()
                    && attempt.content_type.is_some()
                    && attempt.retry_after.is_some()
                    && attempt.raw_body.is_some()
                    && attempt.bytes_received.is_none()
                    && attempt.error_kind.is_none(),
                "HTTP attempt has incompatible fields"
            );
            let status = attempt.status_code.unwrap();
            ensure!((100..=599).contains(&status), "HTTP status is out of range");
            validate_nullable_header(attempt.content_type.as_ref().unwrap(), "content_type")?;
            validate_nullable_header(attempt.retry_after.as_ref().unwrap(), "retry_after")?;
            let raw_body = attempt.raw_body.as_ref().unwrap();
            ensure!(
                raw_body.bytes
                    <= if status == 200 {
                        MAX_SUCCESS_RESPONSE_BYTES as u64
                    } else {
                        MAX_ERROR_RESPONSE_BYTES as u64
                    },
                "HTTP response body exceeds its status-specific limit"
            );
            read_and_validate_raw_body(context, raw_body)?;
            match attempt.retry_disposition.as_str() {
                "success" => ensure!(status == 200, "only HTTP 200 can be successful"),
                "retry_fixed_backoff"
                | "retry_retry_after"
                | "stop_attempt_limit"
                | "stop_retry_after_cap" => ensure!(
                    context
                        .plan
                        .transport
                        .retryable_http_statuses
                        .contains(&status),
                    "HTTP retry disposition is attached to a nonretryable status"
                ),
                "stop_nonretryable_status" => ensure!(
                    status != 200
                        && !context
                            .plan
                            .transport
                            .retryable_http_statuses
                            .contains(&status),
                    "nonretryable disposition is attached to a retryable status"
                ),
                "stop_invalid_content_encoding" => {}
                "stop_invalid_content_type"
                | "stop_invalid_json"
                | "stop_invalid_semantics"
                | "stop_json_depth_limit"
                | "stop_row_limit" => {
                    ensure!(status == 200, "success-validation stop requires HTTP 200")
                }
                other => bail!("HTTP response has incompatible disposition {other}"),
            }
            if attempt.retry_disposition == "retry_retry_after" {
                let retry_after = attempt
                    .retry_after
                    .as_ref()
                    .and_then(|value| value.as_deref())
                    .context("retry_retry_after requires a Retry-After value")?;
                let finished = parse_timestamp(&attempt.finished_at, "attempt finished_at")?;
                ensure!(
                    retry_after_delay_ms(retry_after, finished)
                        .is_some_and(|delay| delay <= RETRY_AFTER_CAP_MS),
                    "retry_retry_after has an invalid or excessive delay"
                );
            }
            if attempt.retry_disposition == "stop_retry_after_cap" {
                let retry_after = attempt
                    .retry_after
                    .as_ref()
                    .and_then(|value| value.as_deref())
                    .context("stop_retry_after_cap requires a Retry-After value")?;
                let finished = parse_timestamp(&attempt.finished_at, "attempt finished_at")?;
                ensure!(
                    retry_after_delay_ms(retry_after, finished)
                        .is_some_and(|delay| delay > RETRY_AFTER_CAP_MS),
                    "Retry-After cap stop does not exceed the frozen cap"
                );
            }
        }
        "transport_error" => {
            ensure!(
                attempt.error_kind.is_some()
                    && attempt.status_code.is_none()
                    && attempt.content_type.is_none()
                    && attempt.retry_after.is_none()
                    && attempt.raw_body.is_none()
                    && attempt.bytes_received.is_none(),
                "transport-error attempt has incompatible fields"
            );
            validate_error_kind(attempt.error_kind.as_deref().unwrap())?;
            ensure!(
                matches!(
                    attempt.retry_disposition.as_str(),
                    "retry_fixed_backoff" | "stop_transport_error"
                ),
                "transport error has an incompatible retry disposition"
            );
        }
        "response_limit_exceeded" => {
            ensure!(
                attempt.status_code.is_some()
                    && attempt.bytes_received.is_some()
                    && attempt.error_kind.is_some()
                    && attempt.content_type.is_none()
                    && attempt.retry_after.is_none()
                    && attempt.raw_body.is_none(),
                "response-limit attempt has incompatible fields"
            );
            ensure!(
                (100..=599).contains(&attempt.status_code.unwrap())
                    && attempt.bytes_received.unwrap() <= (MAX_SUCCESS_RESPONSE_BYTES as u64 + 1),
                "response-limit status or byte count is invalid"
            );
            validate_error_kind(attempt.error_kind.as_deref().unwrap())?;
            ensure!(
                matches!(
                    attempt.retry_disposition.as_str(),
                    "stop_response_header_limit" | "stop_response_limit" | "stop_total_limit"
                ),
                "response limit has an incompatible retry disposition"
            );
        }
        other => bail!("unknown attempt outcome {other}"),
    }
    Ok(())
}

fn validate_nullable_header(value: &Option<String>, label: &str) -> Result<()> {
    if let Some(value) = value {
        ensure!(
            !value.is_empty() && value.len() <= 512,
            "attempt {label} is outside the execution-index length limit"
        );
    }
    Ok(())
}

fn validate_error_kind(value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    ensure!(
        (1..=80).contains(&bytes.len())
            && bytes[0].is_ascii_lowercase()
            && bytes[1..].iter().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_'
            }),
        "attempt error_kind is invalid"
    );
    Ok(())
}

fn validate_retry_wait(previous: &ExecutionAttempt, current: &ExecutionAttempt) -> Result<()> {
    let current_started = parse_timestamp(&current.started_at, "attempt started_at")?;
    let previous_finished = parse_timestamp(&previous.finished_at, "attempt finished_at")?;
    let delay = match previous.retry_disposition.as_str() {
        "retry_fixed_backoff" => RETRY_BACKOFF_MS
            .get(previous.attempt_number as usize)
            .copied()
            .context("fixed retry follows the maximum attempt number")?,
        "retry_retry_after" => {
            let value = previous
                .retry_after
                .as_ref()
                .and_then(|value| value.as_deref())
                .context("Retry-After retry is missing its header")?;
            retry_after_delay_ms(value, previous_finished).context("Retry-After is invalid")?
        }
        other => bail!("attempt after non-retry disposition {other}"),
    };
    let required = previous_finished
        + ChronoDuration::milliseconds(delay.try_into().context("retry delay exceeds i64")?);
    ensure!(
        current_started >= required,
        "attempt starts before its fixed or Retry-After delay"
    );
    Ok(())
}

fn validate_halted_request(halted: &HaltedRequest) -> Result<()> {
    let final_attempt = halted
        .attempts
        .last()
        .context("halted request has no attempts")?;
    let valid = match halted.reason.as_str() {
        "attempts_exhausted" => {
            halted.attempts.len() == MAX_ATTEMPTS
                && final_attempt.retry_disposition == "stop_attempt_limit"
        }
        "nonretryable_http_status" => final_attempt.retry_disposition == "stop_nonretryable_status",
        "retry_after_exceeds_cap" => final_attempt.retry_disposition == "stop_retry_after_cap",
        "response_header_limit" => final_attempt.retry_disposition == "stop_response_header_limit",
        "response_body_limit" => final_attempt.retry_disposition == "stop_response_limit",
        "total_body_limit" => final_attempt.retry_disposition == "stop_total_limit",
        "invalid_content_encoding" => {
            final_attempt.retry_disposition == "stop_invalid_content_encoding"
        }
        "invalid_content_type" => final_attempt.retry_disposition == "stop_invalid_content_type",
        "invalid_json" => final_attempt.retry_disposition == "stop_invalid_json",
        "invalid_semantics" => final_attempt.retry_disposition == "stop_invalid_semantics",
        "json_depth_limit" => final_attempt.retry_disposition == "stop_json_depth_limit",
        "row_limit" => final_attempt.retry_disposition == "stop_row_limit",
        "transport_error" => final_attempt.retry_disposition == "stop_transport_error",
        "operator_interrupt" => {
            halted.attempts.len() < MAX_ATTEMPTS
                && matches!(
                    final_attempt.retry_disposition.as_str(),
                    "retry_fixed_backoff" | "retry_retry_after"
                )
        }
        _ => false,
    };
    ensure!(valid, "halted reason does not match its final attempt");
    Ok(())
}

fn snapshot_identity(
    plan_sha256: &str,
    receipts: &[SuccessfulReceipt],
) -> Result<SnapshotIdentity> {
    ensure!(
        receipts.len() == REQUEST_COUNT,
        "snapshot identity requires 1000 receipts"
    );
    let mut context = DigestContext::new(&SHA256);
    context.update(plan_sha256.as_bytes());
    for receipt in receipts {
        context.update(&[0_u8]);
        context.update(receipt.request_sha256.as_bytes());
        context.update(&[0_u8]);
        context.update(receipt.response.raw_body.sha256.as_bytes());
    }
    Ok(SnapshotIdentity {
        algorithm: "sha256".to_string(),
        input: "UTF8(plan_sha256) || for ordinal 0..999: 0x00 || UTF8(request_sha256) || 0x00 || UTF8(successful_raw_body_sha256)".to_string(),
        request_count: REQUEST_COUNT as u32,
        sha256: format_sha256_identifier(context.finish().as_ref()),
    })
}

fn execution_snapshot_identity(
    plan_sha256: &str,
    receipts: &[SuccessfulReceipt],
    recoveries: &[TransportRecovery],
) -> Result<SnapshotIdentity> {
    let mut identity = snapshot_identity(plan_sha256, receipts)?;
    if !recoveries.is_empty() {
        let mut digest = DigestContext::new(&SHA256);
        digest.update(identity.sha256.as_bytes());
        digest.update(&[0_u8]);
        digest.update(&canonical_file_bytes(&recoveries)?);
        identity.sha256 = format_sha256_identifier(digest.finish().as_ref());
        identity.input =
            "UTF8(v1_snapshot_sha256) || 0x00 || canonical_JSON_plus_LF(transport_recoveries)"
                .to_string();
    }
    Ok(identity)
}

fn halted_request(
    request: &PlannedRequest,
    reason: &str,
    attempts: Vec<ExecutionAttempt>,
) -> HaltedRequest {
    HaltedRequest {
        ordinal: request.ordinal,
        ddd_prefix: request.ddd_prefix.clone(),
        method: request.method.clone(),
        url: request.url.clone(),
        request_sha256: request.request_sha256.clone(),
        reason: reason.to_string(),
        attempts,
    }
}

fn reserved_attempt(attempt_number: usize) -> ExecutionAttempt {
    let now = timestamp_now();
    ExecutionAttempt {
        attempt_number: attempt_number as u32,
        started_at: now.clone(),
        finished_at: now,
        elapsed_ms: 0,
        outcome: "transport_error".to_string(),
        status_code: None,
        content_type: None,
        retry_after: None,
        raw_body: None,
        bytes_received: None,
        error_kind: Some("attempt_reserved".to_string()),
        retry_disposition: if attempt_number < MAX_ATTEMPTS {
            "retry_fixed_backoff"
        } else {
            "stop_transport_error"
        }
        .to_string(),
    }
}

fn timestamp_now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn parse_timestamp(value: &str, label: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .with_context(|| format!("invalid RFC 3339 timestamp for {label}"))
}

fn build_http_client(plan: &QueryPlan) -> Result<Client> {
    Client::builder()
        .https_only(true)
        .redirect(Policy::none())
        .connect_timeout(Duration::from_millis(plan.transport.connect_timeout_ms))
        .timeout(Duration::from_millis(plan.transport.attempt_timeout_ms))
        .build()
        .context("failed to build the frozen COD HTTP client")
}

async fn wait_for_attempt_start(
    index: &DiscoveryExecutionIndex,
    current_attempts: &[ExecutionAttempt],
) -> Result<()> {
    let latest = current_attempts.last().or_else(|| {
        index
            .halted_request
            .as_ref()
            .and_then(|halted| halted.attempts.last())
            .or_else(|| {
                index
                    .transport_recoveries
                    .last()
                    .filter(|review| {
                        review.halted_request.ordinal as usize == index.successful_receipts.len()
                    })
                    .and_then(|review| review.halted_request.attempts.last())
            })
            .or_else(|| {
                index
                    .successful_receipts
                    .last()
                    .and_then(|receipt| receipt.attempts.last())
            })
    });
    let Some(latest) = latest else {
        return Ok(());
    };

    let started = parse_timestamp(&latest.started_at, "previous attempt started_at")?;
    let finished = parse_timestamp(&latest.finished_at, "previous attempt finished_at")?;
    let spacing_target = started
        + ChronoDuration::milliseconds(
            ATTEMPT_START_INTERVAL_MS
                .try_into()
                .expect("frozen interval fits i64"),
        );
    let retry_target = if latest.retry_disposition == "retry_retry_after" {
        let raw = latest
            .retry_after
            .as_ref()
            .and_then(|value| value.as_deref())
            .context("retry_retry_after attempt is missing Retry-After")?;
        finished
            + ChronoDuration::milliseconds(
                retry_after_delay_ms(raw, finished)
                    .context("stored Retry-After is invalid")?
                    .try_into()
                    .context("Retry-After does not fit i64")?,
            )
    } else if latest.retry_disposition == "retry_fixed_backoff" {
        let next_attempt = latest.attempt_number as usize;
        finished
            + ChronoDuration::milliseconds(
                RETRY_BACKOFF_MS[next_attempt]
                    .try_into()
                    .expect("frozen backoff fits i64"),
            )
    } else {
        finished
    };
    let target = spacing_target.max(retry_target);
    let now = Utc::now();
    if target > now {
        let delay = (target - now)
            .to_std()
            .context("attempt delay cannot be represented")?;
        sleep(delay).await;
    }
    Ok(())
}

async fn perform_attempt(
    client: &Client,
    context: &ExecutionContext,
    request: &PlannedRequest,
    attempt_number: usize,
    current_total_raw_bytes: u64,
) -> Result<AttemptDecision> {
    let headers = request_headers(&context.plan)?;
    let started_at = timestamp_now();
    let started = Instant::now();
    let response = client.get(&request.url).headers(headers).send().await;

    let mut response = match response {
        Ok(response) => response,
        Err(error) => {
            log_transport_error(request.ordinal, attempt_number, &error);
            let finished_at = timestamp_now();
            let attempt = ExecutionAttempt {
                attempt_number: attempt_number as u32,
                started_at,
                finished_at,
                elapsed_ms: elapsed_millis(started),
                outcome: "transport_error".to_string(),
                status_code: None,
                content_type: None,
                retry_after: None,
                raw_body: None,
                bytes_received: None,
                error_kind: Some(reqwest_error_kind(&error).to_string()),
                retry_disposition: if attempt_number < MAX_ATTEMPTS {
                    "retry_fixed_backoff"
                } else {
                    "stop_transport_error"
                }
                .to_string(),
            };
            return Ok(if attempt_number < MAX_ATTEMPTS {
                AttemptDecision::Retry(attempt)
            } else {
                AttemptDecision::Halt {
                    attempt,
                    reason: "transport_error".to_string(),
                }
            });
        }
    };
    ensure!(
        exact_response_url_matches(response.url().as_str(), &request.url),
        "HTTP client returned a response for an unexpected final URL"
    );

    let status_code = response.status().as_u16();
    let response_headers = response.headers();
    if response_header_bytes(status_code, response_headers) > MAX_RESPONSE_HEADER_BYTES {
        return Ok(AttemptDecision::Halt {
            attempt: response_limit_attempt(
                attempt_number,
                started_at,
                started,
                status_code,
                0,
                "response_header_limit",
                "stop_response_header_limit",
            ),
            reason: "response_header_limit".to_string(),
        });
    }

    let content_type_header = indexed_header(response_headers, "content-type");
    let retry_after_header = indexed_header(response_headers, "retry-after");
    let identity_encoding = has_identity_content_encoding(response_headers);
    let content_type = content_type_essence(content_type_header.as_deref());
    let response_limit = if status_code == 200 {
        MAX_SUCCESS_RESPONSE_BYTES
    } else {
        MAX_ERROR_RESPONSE_BYTES
    };
    let remaining_total = MAX_TOTAL_RAW_RESPONSE_BYTES
        .checked_sub(current_total_raw_bytes)
        .context("execution already exceeds the total raw-response limit")?;
    let effective_limit = response_limit.min(remaining_total.try_into().unwrap_or(usize::MAX));
    if let Some(length) = response.content_length() {
        if length > response_limit as u64 || length > remaining_total {
            let total_limited =
                length > remaining_total && remaining_total <= response_limit as u64;
            return Ok(AttemptDecision::Halt {
                attempt: response_limit_attempt(
                    attempt_number,
                    started_at,
                    started,
                    status_code,
                    0,
                    if total_limited {
                        "total_body_limit"
                    } else {
                        "response_body_limit"
                    },
                    if total_limited {
                        "stop_total_limit"
                    } else {
                        "stop_response_limit"
                    },
                ),
                reason: if total_limited {
                    "total_body_limit"
                } else {
                    "response_body_limit"
                }
                .to_string(),
            });
        }
    }

    let body = match read_bounded_body(&mut response, effective_limit).await {
        Ok(BoundedBody::Complete(bytes)) => bytes,
        Ok(BoundedBody::LimitExceeded(received)) => {
            let total_limited = remaining_total <= response_limit as u64;
            return Ok(AttemptDecision::Halt {
                attempt: response_limit_attempt(
                    attempt_number,
                    started_at,
                    started,
                    status_code,
                    received,
                    if total_limited {
                        "total_body_limit"
                    } else {
                        "response_body_limit"
                    },
                    if total_limited {
                        "stop_total_limit"
                    } else {
                        "stop_response_limit"
                    },
                ),
                reason: if total_limited {
                    "total_body_limit"
                } else {
                    "response_body_limit"
                }
                .to_string(),
            });
        }
        Err(error) => {
            let attempt = ExecutionAttempt {
                attempt_number: attempt_number as u32,
                started_at,
                finished_at: timestamp_now(),
                elapsed_ms: elapsed_millis(started),
                outcome: "transport_error".to_string(),
                status_code: None,
                content_type: None,
                retry_after: None,
                raw_body: None,
                bytes_received: None,
                error_kind: Some(reqwest_error_kind(&error).to_string()),
                retry_disposition: if attempt_number < MAX_ATTEMPTS {
                    "retry_fixed_backoff"
                } else {
                    "stop_transport_error"
                }
                .to_string(),
            };
            return Ok(if attempt_number < MAX_ATTEMPTS {
                AttemptDecision::Retry(attempt)
            } else {
                AttemptDecision::Halt {
                    attempt,
                    reason: "transport_error".to_string(),
                }
            });
        }
    };

    let raw_body = store_raw_body(context, &body)?;
    let finished_at = timestamp_now();
    let retrieved_at = finished_at.clone();
    let mut attempt = ExecutionAttempt {
        attempt_number: attempt_number as u32,
        started_at,
        finished_at,
        elapsed_ms: elapsed_millis(started),
        outcome: "http_response".to_string(),
        status_code: Some(status_code),
        content_type: Some(content_type_header.clone()),
        retry_after: Some(retry_after_header.clone()),
        raw_body: Some(raw_body.clone()),
        bytes_received: None,
        error_kind: None,
        retry_disposition: String::new(),
    };

    if !identity_encoding {
        attempt.retry_disposition = "stop_invalid_content_encoding".to_string();
        return Ok(AttemptDecision::Halt {
            attempt,
            reason: "invalid_content_encoding".to_string(),
        });
    }

    if status_code != 200 {
        if !context
            .plan
            .transport
            .retryable_http_statuses
            .contains(&status_code)
        {
            attempt.retry_disposition = "stop_nonretryable_status".to_string();
            return Ok(AttemptDecision::Halt {
                attempt,
                reason: "nonretryable_http_status".to_string(),
            });
        }
        if attempt_number == MAX_ATTEMPTS {
            attempt.retry_disposition = "stop_attempt_limit".to_string();
            return Ok(AttemptDecision::Halt {
                attempt,
                reason: "attempts_exhausted".to_string(),
            });
        }
        if let Some(retry_after) = retry_after_header.as_deref() {
            if let Some(delay) = retry_after_delay_ms(retry_after, Utc::now()) {
                if delay > RETRY_AFTER_CAP_MS {
                    attempt.retry_disposition = "stop_retry_after_cap".to_string();
                    return Ok(AttemptDecision::Halt {
                        attempt,
                        reason: "retry_after_exceeds_cap".to_string(),
                    });
                }
                attempt.retry_disposition = "retry_retry_after".to_string();
                return Ok(AttemptDecision::Retry(attempt));
            }
        }
        attempt.retry_disposition = "retry_fixed_backoff".to_string();
        return Ok(AttemptDecision::Retry(attempt));
    }

    let Some(content_type) = content_type else {
        attempt.retry_disposition = "stop_invalid_content_type".to_string();
        return Ok(AttemptDecision::Halt {
            attempt,
            reason: "invalid_content_type".to_string(),
        });
    };
    if !context
        .plan
        .transport
        .accepted_success_content_types
        .contains(&content_type)
    {
        attempt.retry_disposition = "stop_invalid_content_type".to_string();
        return Ok(AttemptDecision::Halt {
            attempt,
            reason: "invalid_content_type".to_string(),
        });
    }
    let validation = match validate_cod_response(&body, &request.ddd_prefix) {
        Ok(validation) => validation,
        Err(error) => {
            let (disposition, reason) = match error {
                ResponseValidationError::JsonDepthLimit(_) => {
                    ("stop_json_depth_limit", "json_depth_limit")
                }
                ResponseValidationError::RowLimit(_) => ("stop_row_limit", "row_limit"),
                ResponseValidationError::InvalidJson(_) => ("stop_invalid_json", "invalid_json"),
                ResponseValidationError::InvalidSemantics(_) => {
                    ("stop_invalid_semantics", "invalid_semantics")
                }
            };
            attempt.retry_disposition = disposition.to_string();
            return Ok(AttemptDecision::Halt {
                attempt,
                reason: reason.to_string(),
            });
        }
    };
    attempt.retry_disposition = "success".to_string();
    Ok(AttemptDecision::Success {
        attempt,
        response: SuccessfulResponse {
            status_code,
            content_type,
            retrieved_at,
            raw_body,
            json_validation: JsonValidation {
                valid: true,
                maximum_depth: validation.maximum_depth as u32,
                row_count: validation.row_count as u32,
            },
        },
    })
}

fn request_headers(plan: &QueryPlan) -> Result<HeaderMap> {
    let mut headers = HeaderMap::with_capacity(plan.transport.headers.len());
    for (name, value) in &plan.transport.headers {
        let name = HeaderName::from_bytes(name.as_bytes())
            .with_context(|| format!("invalid frozen request header name {name:?}"))?;
        let value = HeaderValue::from_str(value)
            .with_context(|| format!("invalid frozen request header value for {name}"))?;
        headers.insert(name, value);
    }
    ensure!(
        headers.len() == 4,
        "frozen request must contain exactly four headers"
    );
    Ok(headers)
}

fn response_header_bytes(status_code: u16, headers: &HeaderMap) -> usize {
    let mut total = 16_usize;
    for (name, value) in headers {
        total = total
            .saturating_add(name.as_str().len())
            .saturating_add(value.as_bytes().len())
            .saturating_add(4);
    }
    total.saturating_add(status_code.to_string().len())
}

fn indexed_header(headers: &HeaderMap, name: &str) -> Option<String> {
    let value = headers.get(name)?;
    let value = value.to_str().ok()?;
    if value.is_empty() || value.len() > 512 {
        return None;
    }
    Some(value.to_string())
}

fn content_type_essence(value: Option<&str>) -> Option<String> {
    let essence = value?.split(';').next()?.trim().to_ascii_lowercase();
    if essence.is_empty() {
        None
    } else {
        Some(essence)
    }
}

fn has_identity_content_encoding(headers: &HeaderMap) -> bool {
    headers.get_all("content-encoding").iter().all(|value| {
        value
            .to_str()
            .map(|value| value.trim().eq_ignore_ascii_case("identity"))
            .unwrap_or(false)
    })
}

fn exact_response_url_matches(actual: &str, planned: &str) -> bool {
    actual.as_bytes() == planned.as_bytes()
}

async fn read_bounded_body(response: &mut Response, limit: usize) -> reqwest::Result<BoundedBody> {
    let mut body = Vec::with_capacity(limit.min(64 * 1024));
    while let Some(chunk) = response.chunk().await? {
        if chunk.len() > limit.saturating_sub(body.len()) {
            return Ok(BoundedBody::LimitExceeded((limit as u64).saturating_add(1)));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(BoundedBody::Complete(body))
}

fn response_limit_attempt(
    attempt_number: usize,
    started_at: String,
    started: Instant,
    status_code: u16,
    bytes_received: u64,
    error_kind: &str,
    retry_disposition: &str,
) -> ExecutionAttempt {
    ExecutionAttempt {
        attempt_number: attempt_number as u32,
        started_at,
        finished_at: timestamp_now(),
        elapsed_ms: elapsed_millis(started),
        outcome: "response_limit_exceeded".to_string(),
        status_code: Some(status_code),
        content_type: None,
        retry_after: None,
        raw_body: None,
        bytes_received: Some(bytes_received.min((MAX_SUCCESS_RESPONSE_BYTES + 1) as u64)),
        error_kind: Some(error_kind.to_string()),
        retry_disposition: retry_disposition.to_string(),
    }
}

fn elapsed_millis(started: Instant) -> u64 {
    started
        .elapsed()
        .as_millis()
        .min(ATTEMPT_TIMEOUT_MS as u128) as u64
}

fn reqwest_error_kind(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "timeout"
    } else if error.is_connect() {
        "connect"
    } else if error.is_body() {
        "body"
    } else if error.is_decode() {
        "decode"
    } else if error.is_request() {
        "request"
    } else {
        "transport"
    }
}

fn log_transport_error(ordinal: u32, attempt: usize, error: &reqwest::Error) {
    eprintln!(
        "COD shard {ordinal:03}, attempt {attempt}/{MAX_ATTEMPTS}: {}: {error}",
        reqwest_error_kind(error)
    );
    let mut source = std::error::Error::source(error);
    for _ in 0..8 {
        let Some(cause) = source else { break };
        eprintln!("  caused by: {cause}");
        source = cause.source();
    }
}

fn retry_after_delay_ms(value: &str, relative_to: DateTime<Utc>) -> Option<u64> {
    let value = value.trim();
    if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
        return value.parse::<u64>().ok()?.checked_mul(1_000);
    }
    let target = DateTime::parse_from_rfc2822(value)
        .ok()?
        .with_timezone(&Utc);
    let delay = target.signed_duration_since(relative_to).num_milliseconds();
    Some(delay.max(0) as u64)
}

static CAS_TEMP_NONCE: AtomicU64 = AtomicU64::new(0);

fn store_raw_body(context: &ExecutionContext, bytes: &[u8]) -> Result<RawBodyBinding> {
    let sha256 = sha256_identifier(bytes);
    let bare = sha256
        .strip_prefix("sha256:")
        .expect("internal SHA-256 identifiers are prefixed")
        .to_string();
    let prefix = &bare[..2];
    let relative = format!("{PILOT_ROOT_RELATIVE}/{OBJECTS_RELATIVE}/{prefix}/{bare}");
    let directory = ensure_cas_directory(&context.pilot_root, prefix)?;
    let path = directory.join(&bare);
    let binding = RawBodyBinding {
        path: relative,
        sha256,
        bytes: bytes.len() as u64,
    };
    if path.exists() {
        verify_cas_object(&path, &binding)?;
        return Ok(binding);
    }

    let nonce = CAS_TEMP_NONCE.fetch_add(1, Ordering::Relaxed);
    let temporary = directory.join(format!(".{bare}.tmp.{}.{}", std::process::id(), nonce));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .with_context(|| {
            format!(
                "failed to create CAS temporary file: {}",
                temporary.display()
            )
        })?;
    file.write_all(bytes).with_context(|| {
        format!(
            "failed to write CAS temporary file: {}",
            temporary.display()
        )
    })?;
    file.sync_all().with_context(|| {
        format!(
            "failed to synchronize CAS temporary file: {}",
            temporary.display()
        )
    })?;
    drop(file);

    match fs::hard_link(&temporary, &path) {
        Ok(()) => {
            fs::remove_file(&temporary).with_context(|| {
                format!(
                    "failed to remove published CAS temporary: {}",
                    temporary.display()
                )
            })?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            verify_cas_object(&path, &binding)?;
            fs::remove_file(&temporary).with_context(|| {
                format!(
                    "failed to remove redundant CAS temporary: {}",
                    temporary.display()
                )
            })?;
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            return Err(error).with_context(|| {
                format!("failed to publish immutable CAS object: {}", path.display())
            });
        }
    }
    verify_cas_object(&path, &binding)?;
    Ok(binding)
}

fn ensure_cas_directory(pilot_root: &Path, prefix: &str) -> Result<PathBuf> {
    ensure!(
        prefix.len() == 2 && prefix.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "invalid CAS prefix"
    );
    let mut current = pilot_root.to_path_buf();
    for component in ["objects", "sha256", prefix] {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "CAS path component must be a real directory: {}",
                current.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match fs::create_dir(&current) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => {
                        return Err(error).with_context(|| {
                            format!("failed to create CAS directory: {}", current.display())
                        })
                    }
                }
                let metadata = fs::symlink_metadata(&current)?;
                ensure!(
                    metadata.is_dir() && !metadata.file_type().is_symlink(),
                    "created CAS component is not a real directory: {}",
                    current.display()
                );
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(current)
}

fn read_and_validate_raw_body(
    context: &ExecutionContext,
    binding: &RawBodyBinding,
) -> Result<Vec<u8>> {
    validate_sha256_identifier(&binding.sha256, "raw body sha256")?;
    let bare = binding
        .sha256
        .strip_prefix("sha256:")
        .expect("validated SHA-256 identifier has prefix");
    let expected_relative = format!(
        "{PILOT_ROOT_RELATIVE}/{OBJECTS_RELATIVE}/{}/{}",
        &bare[..2],
        bare
    );
    ensure!(
        binding.path == expected_relative,
        "raw-body path does not match its SHA-256"
    );
    let path = safe_repo_file(&context.repo_root, &binding.path, "raw COD response body")?;
    read_cas_object(&path, binding)
}

fn verify_cas_object(path: &Path, binding: &RawBodyBinding) -> Result<()> {
    read_cas_object(path, binding).map(|_| ())
}

fn read_cas_object(path: &Path, binding: &RawBodyBinding) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect CAS object: {}", path.display()))?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "CAS object must be a regular non-symlink file: {}",
        path.display()
    );
    ensure!(
        metadata.len() == binding.bytes,
        "CAS object byte count drifted"
    );
    let bytes =
        fs::read(path).with_context(|| format!("failed to read CAS object: {}", path.display()))?;
    let actual = sha256_identifier(&bytes);
    ensure!(
        actual == binding.sha256 && bytes.len() as u64 == binding.bytes,
        "CAS object does not match its immutable binding: {}",
        path.display()
    );
    Ok(bytes)
}

fn validate_cod_response(
    bytes: &[u8],
    ddd_prefix: &str,
) -> std::result::Result<ResponseValidation, ResponseValidationError> {
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        return Err(ResponseValidationError::InvalidJson(
            "UTF-8 BOM is forbidden".to_string(),
        ));
    }
    std::str::from_utf8(bytes).map_err(|error| {
        ResponseValidationError::InvalidJson(format!("response is not UTF-8: {error}"))
    })?;
    if ddd_prefix.len() != 3 || !ddd_prefix.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ResponseValidationError::InvalidSemantics(
            "invalid planned DDD prefix".to_string(),
        ));
    }

    let maximum_depth = Cell::new(0_usize);
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = StrictJsonSeed {
        depth: 1,
        maximum_depth: &maximum_depth,
    }
    .deserialize(&mut deserializer)
    .map_err(|error| classify_strict_json_error(&error.to_string()))?;
    deserializer
        .end()
        .map_err(|error| ResponseValidationError::InvalidJson(error.to_string()))?;
    let rows = value.as_array().ok_or_else(|| {
        ResponseValidationError::InvalidSemantics(
            "top-level JSON value must be an array".to_string(),
        )
    })?;
    if rows.len() > MAX_ROWS_PER_SHARD {
        return Err(ResponseValidationError::RowLimit(rows.len()));
    }

    let mut records: BTreeMap<(String, u64), String> = BTreeMap::new();
    for (row_index, row) in rows.iter().enumerate() {
        let object = row.as_object().ok_or_else(|| {
            ResponseValidationError::InvalidSemantics(format!("row {row_index} is not an object"))
        })?;
        let file_value = object.get("file").ok_or_else(|| {
            ResponseValidationError::InvalidSemantics(format!("row {row_index} is missing file"))
        })?;
        let revision_value = object.get("svnrevision").ok_or_else(|| {
            ResponseValidationError::InvalidSemantics(format!(
                "row {row_index} is missing svnrevision"
            ))
        })?;
        let file = parse_cod_file(file_value).map_err(|message| {
            ResponseValidationError::InvalidSemantics(format!("row {row_index} file: {message}"))
        })?;
        if !file.starts_with(ddd_prefix) {
            return Err(ResponseValidationError::InvalidSemantics(format!(
                "row {row_index} file {file} does not match prefix {ddd_prefix}"
            )));
        }
        let revision = parse_positive_decimal(revision_value).map_err(|message| {
            ResponseValidationError::InvalidSemantics(format!(
                "row {row_index} svnrevision: {message}"
            ))
        })?;
        let mut semantic_row = object.clone();
        semantic_row.insert("file".to_string(), Value::String(file.clone()));
        semantic_row.insert(
            "svnrevision".to_string(),
            Value::Number(serde_json::Number::from(revision)),
        );
        let semantic_digest = sha256_identifier(
            &canonical_json_bytes(&Value::Object(semantic_row))
                .map_err(|error| ResponseValidationError::InvalidSemantics(error.to_string()))?,
        );
        if let Some(previous) = records.insert((file.clone(), revision), semantic_digest.clone()) {
            if previous != semantic_digest {
                return Err(ResponseValidationError::InvalidSemantics(format!(
                    "row {row_index} conflicts with a duplicate ({file}, {revision}) key"
                )));
            }
        }
    }
    Ok(ResponseValidation {
        maximum_depth: maximum_depth.get(),
        row_count: rows.len(),
    })
}

fn parse_cod_file(value: &Value) -> std::result::Result<String, String> {
    if let Some(value) = value.as_str() {
        if value.len() == 7 && value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Ok(value.to_string());
        }
        return Err("string form must be exactly seven ASCII digits".to_string());
    }
    let number = value.as_u64().ok_or_else(|| {
        "must be an unsigned JSON integer or seven-digit decimal string".to_string()
    })?;
    if !(1_000_000..=9_999_999).contains(&number) {
        return Err("integer form must be between 1000000 and 9999999".to_string());
    }
    Ok(format!("{number:07}"))
}

fn parse_positive_decimal(value: &Value) -> std::result::Result<u64, String> {
    if let Some(value) = value.as_str() {
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("string form must contain only ASCII decimal digits".to_string());
        }
        if value.starts_with('0') {
            return Err("string form must be positive and have no leading zero".to_string());
        }
        return value
            .parse::<u64>()
            .map_err(|_| "string form exceeds the unsigned 64-bit range".to_string());
    }
    let number = value.as_u64().ok_or_else(|| {
        "must be an unsigned JSON integer or canonical decimal string".to_string()
    })?;
    if number == 0 {
        return Err("must be greater than zero".to_string());
    }
    Ok(number)
}

fn classify_strict_json_error(message: &str) -> ResponseValidationError {
    if let Some(marker) = message.find("__depth_limit__:") {
        let depth = message[marker + "__depth_limit__:".len()..]
            .split_whitespace()
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(MAX_JSON_DEPTH + 1);
        ResponseValidationError::JsonDepthLimit(depth)
    } else if let Some(marker) = message.find("__row_limit__:") {
        let rows = message[marker + "__row_limit__:".len()..]
            .split_whitespace()
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(MAX_ROWS_PER_SHARD + 1);
        ResponseValidationError::RowLimit(rows)
    } else {
        ResponseValidationError::InvalidJson(message.to_string())
    }
}

struct StrictJsonSeed<'a> {
    depth: usize,
    maximum_depth: &'a Cell<usize>,
}

impl<'de> DeserializeSeed<'de> for StrictJsonSeed<'_> {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        if self.depth > MAX_JSON_DEPTH {
            return Err(de::Error::custom(format!("__depth_limit__:{}", self.depth)));
        }
        self.maximum_depth
            .set(self.maximum_depth.get().max(self.depth));
        deserializer.deserialize_any(StrictJsonVisitor {
            depth: self.depth,
            maximum_depth: self.maximum_depth,
        })
    }
}

struct StrictJsonVisitor<'a> {
    depth: usize,
    maximum_depth: &'a Cell<usize>,
}

impl<'de> Visitor<'de> for StrictJsonVisitor<'_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_unit<E>(self) -> std::result::Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E>(self) -> std::result::Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_bool<E>(self, value: bool) -> std::result::Result<Self::Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> std::result::Result<Self::Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_u64<E>(self, value: u64) -> std::result::Result<Self::Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_f64<E>(self, value: f64) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        serde_json::Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("non-finite JSON number"))
    }

    fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E> {
        Ok(Value::String(value.to_string()))
    }

    fn visit_string<E>(self, value: String) -> std::result::Result<Self::Value, E> {
        Ok(Value::String(value))
    }

    fn visit_seq<A>(self, mut sequence: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element_seed(StrictJsonSeed {
            depth: self.depth + 1,
            maximum_depth: self.maximum_depth,
        })? {
            if self.depth == 1 && values.len() == MAX_ROWS_PER_SHARD {
                return Err(de::Error::custom(format!(
                    "__row_limit__:{}",
                    MAX_ROWS_PER_SHARD + 1
                )));
            }
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut values = serde_json::Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(de::Error::custom(format!(
                    "duplicate JSON object key {key:?}"
                )));
            }
            let value = map.next_value_seed(StrictJsonSeed {
                depth: self.depth + 1,
                maximum_depth: self.maximum_depth,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

fn build_prepared_artifacts(repo_root: &Path) -> Result<PreparedArtifacts> {
    let repo_root = require_real_directory(repo_root, "repository root")?;
    let pilot = validate_pilot_contract(&repo_root)?;
    let catalog_root = safe_repo_path(&repo_root, PUBLIC_CATALOG_PATH, "public catalog")?;
    let catalog = validate_public_catalog_release(&catalog_root)
        .context("committed public catalog validation failed")?;
    validate_catalog_binding(&catalog, &pilot)?;

    let population_entries = load_population(&catalog_root, &catalog)?;
    let population = PopulationSnapshot {
        format: "waajacu-cod-population-snapshot".to_string(),
        schema_version: 1,
        pilot_id: PILOT_ID.to_string(),
        source: PopulationSource {
            catalog_format: catalog.format.clone(),
            catalog_schema_version: catalog.schema_version,
            release_id: catalog.release_id.clone(),
            database_sha256: catalog.database.sha256.clone(),
            database_bytes: catalog.database.bytes,
            table: "minerals".to_string(),
            id_column: "public_id".to_string(),
            canonical_name_column: "canonical_name".to_string(),
        },
        count: population_entries.len() as u64,
        entries: population_entries,
    };
    let population_bytes = canonical_file_bytes(&population)?;
    let population_binding = artifact_binding(POPULATION_SNAPSHOT_FILE, &population_bytes);

    let baseline = build_baseline_selection(&population, &population_binding)?;
    let baseline_bytes = canonical_file_bytes(&baseline)?;
    let baseline_binding = artifact_binding(BASELINE_SELECTION_FILE, &baseline_bytes);

    let query_plan = build_query_plan()?;
    let query_plan_bytes = canonical_file_bytes(&query_plan)?;
    let query_plan_binding = artifact_binding(QUERY_PLAN_FILE, &query_plan_bytes);

    let manifest = PreparationManifest {
        format: "waajacu-cod-pilot-preparation".to_string(),
        schema_version: 1,
        pilot_id: PILOT_ID.to_string(),
        execution_boundary: ExecutionBoundary {
            network_access: "forbidden_during_prepare_and_verify".to_string(),
            mutable_registry_access: "forbidden".to_string(),
            database_writes: "forbidden".to_string(),
        },
        inputs: PreparationInputs {
            pilot_contract: InputArtifact {
                path: PILOT_CONTRACT_PATH.to_string(),
                sha256: pilot.file_sha256,
                bytes: pilot.file_bytes,
            },
            public_catalog: PublicCatalogBinding {
                path: PUBLIC_CATALOG_PATH.to_string(),
                format: catalog.format,
                schema_version: catalog.schema_version,
                release_id: catalog.release_id,
                mineral_count: catalog.mineral_count,
                database_path: catalog.database.path,
                database_sha256: catalog.database.sha256,
                database_bytes: catalog.database.bytes,
            },
        },
        artifacts: PreparedArtifactSet {
            population_snapshot: population_binding,
            baseline_selection: baseline_binding,
            query_plan: query_plan_binding,
        },
    };
    let manifest_bytes = canonical_file_bytes(&manifest)?;

    Ok(PreparedArtifacts {
        population_bytes,
        baseline_bytes,
        query_plan_bytes,
        manifest,
        manifest_bytes,
    })
}

fn validate_pilot_contract(repo_root: &Path) -> Result<PilotBinding> {
    let path = safe_repo_file(repo_root, PILOT_CONTRACT_PATH, "pilot contract")?;
    let bytes = read_regular_file(&path, "pilot contract")?;
    let pilot: Value = serde_json::from_slice(&bytes).context("failed to parse pilot contract")?;

    expect_string(
        &pilot,
        "/format",
        "waajacu-cod-crystallography-pilot-contract",
    )?;
    expect_u64(&pilot, "/schema_version", 1)?;
    expect_string(&pilot, "/pilot_id", PILOT_ID)?;
    expect_string(
        &pilot,
        "/status",
        "contract_ready_candidate_discovery_not_run",
    )?;
    expect_string(
        &pilot,
        "/digest_policy/registry_and_matrix",
        "waajacu-canonical-json-v1",
    )?;
    expect_string(
        &pilot,
        "/digest_policy/schema_artifacts",
        "sha256_of_exact_file_bytes",
    )?;
    expect_string(&pilot, "/digest_policy/prefix", "sha256:")?;

    let release_id = pointer_string(&pilot, "/bindings/population/release_id")?.to_string();
    let mineral_count = pointer_u64(&pilot, "/bindings/population/mineral_count")?;
    expect_string(
        &pilot,
        "/bindings/population/identity_policy",
        "fixed_existing_public_ids_only",
    )?;
    ensure!(
        release_id == EXPECTED_RELEASE_ID,
        "pilot population release_id drifted"
    );
    ensure!(
        mineral_count == EXPECTED_POPULATION_COUNT,
        "pilot population count drifted"
    );

    expect_u64(&pilot, "/sample_design/counts/baseline", 60)?;
    expect_u64(&pilot, "/sample_design/counts/baseline_development", 45)?;
    expect_u64(&pilot, "/sample_design/counts/baseline_holdout", 15)?;
    expect_string(&pilot, "/sample_design/baseline/salt", BASELINE_SALT)?;
    expect_string(
        &pilot,
        "/sample_design/baseline/hash_input",
        "UTF8(salt) || 0x00 || UTF8(public_id)",
    )?;
    expect_string(
        &pilot,
        "/sample_design/baseline/ranking",
        "ascending_sha256_bytes_then_ascending_utf8_public_id",
    )?;
    expect_string(&pilot, "/sample_design/baseline/development_ranks", "1-45")?;
    expect_string(&pilot, "/sample_design/baseline/holdout_ranks", "46-60")?;
    expect_string(
        &pilot,
        "/source_evidence_policy/discovery_endpoint",
        DISCOVERY_ENDPOINT,
    )?;

    validate_canonical_json_binding(repo_root, &pilot, "/bindings/questionnaire_registry")?;
    validate_canonical_json_binding(repo_root, &pilot, "/bindings/source_admission_matrix")?;
    validate_exact_file_bindings(repo_root, &pilot, "/schemas/value_schemas")?;
    validate_exact_file_bindings(repo_root, &pilot, "/schemas/supporting_schemas")?;

    Ok(PilotBinding {
        file_sha256: sha256_identifier(&bytes),
        file_bytes: bytes.len() as u64,
        population_release_id: release_id,
        population_count: mineral_count,
    })
}

fn validate_canonical_json_binding(repo_root: &Path, pilot: &Value, pointer: &str) -> Result<()> {
    let binding = pilot
        .pointer(pointer)
        .with_context(|| format!("pilot contract is missing {pointer}"))?;
    let relative = pointer_string(binding, "/path")?;
    let expected_sha256 = pointer_string(binding, "/sha256")?;
    validate_sha256_identifier(expected_sha256, &format!("{pointer}/sha256"))?;
    let path = safe_repo_file(repo_root, relative, "canonical JSON binding")?;
    let bytes = read_regular_file(&path, "canonical JSON binding")?;
    let value: Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse canonical JSON binding {relative}"))?;
    let actual = sha256_identifier(&canonical_json_bytes(&value)?);
    ensure!(
        actual == expected_sha256,
        "canonical JSON binding digest drifted for {relative}: expected {expected_sha256}, found {actual}"
    );
    Ok(())
}

fn validate_exact_file_bindings(repo_root: &Path, pilot: &Value, pointer: &str) -> Result<()> {
    let bindings = pilot
        .pointer(pointer)
        .and_then(Value::as_array)
        .with_context(|| format!("pilot contract is missing array {pointer}"))?;
    ensure!(
        !bindings.is_empty(),
        "pilot binding array is empty: {pointer}"
    );
    for (index, binding) in bindings.iter().enumerate() {
        let relative = pointer_string(binding, "/path")?;
        let expected_sha256 = pointer_string(binding, "/sha256")?;
        validate_sha256_identifier(expected_sha256, &format!("{pointer}/{index}/sha256"))?;
        let path = safe_repo_file(repo_root, relative, "schema binding")?;
        let bytes = read_regular_file(&path, "schema binding")?;
        let actual = sha256_identifier(&bytes);
        ensure!(
            actual == expected_sha256,
            "exact-file schema digest drifted for {relative}: expected {expected_sha256}, found {actual}"
        );
    }
    Ok(())
}

fn validate_catalog_binding(catalog: &PublicCatalogManifest, pilot: &PilotBinding) -> Result<()> {
    ensure!(
        catalog.format == PUBLIC_CATALOG_FORMAT,
        "catalog format drifted"
    );
    ensure!(
        catalog.schema_version == PUBLIC_CATALOG_SCHEMA_VERSION,
        "catalog schema version drifted"
    );
    ensure!(
        catalog.release_id == pilot.population_release_id
            && catalog.release_id == EXPECTED_RELEASE_ID,
        "catalog release_id does not match the frozen pilot population"
    );
    ensure!(
        catalog.mineral_count == pilot.population_count
            && catalog.mineral_count == EXPECTED_POPULATION_COUNT,
        "catalog mineral count does not match the frozen pilot population"
    );
    ensure!(
        catalog.database.sha256 == EXPECTED_DATABASE_SHA256,
        "catalog database digest drifted"
    );
    ensure!(
        catalog.database.bytes == EXPECTED_DATABASE_BYTES,
        "catalog database byte count drifted"
    );
    Ok(())
}

fn load_population(
    catalog_root: &Path,
    catalog: &PublicCatalogManifest,
) -> Result<Vec<PopulationEntry>> {
    let database_path = safe_repo_file(
        catalog_root,
        &catalog.database.path,
        "manifest-derived public catalog database",
    )?;
    let connection = Connection::open_with_flags(
        &database_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| {
        format!(
            "failed to open public catalog database read-only: {}",
            database_path.display()
        )
    })?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .context("failed to configure public catalog read timeout")?;
    connection
        .execute_batch(
            "PRAGMA query_only = ON; PRAGMA trusted_schema = OFF; PRAGMA foreign_keys = ON;",
        )
        .context("failed to configure public catalog read-only connection")?;

    let mut statement = connection
        .prepare("SELECT public_id, canonical_name FROM minerals")
        .context("failed to prepare frozen population query")?;
    let rows = statement
        .query_map([], |row| {
            Ok(PopulationEntry {
                material_public_id: row.get(0)?,
                canonical_name_snapshot: row.get(1)?,
            })
        })
        .context("failed to query frozen population")?;
    let mut entries = rows
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("failed to read frozen population")?;
    drop(statement);
    drop(connection);

    validate_population_entries(&entries, catalog.mineral_count)?;
    entries.sort_unstable_by(|left, right| {
        left.material_public_id
            .as_bytes()
            .cmp(right.material_public_id.as_bytes())
    });

    // Detect a catalog replacement between validation and the completed read.
    let (final_sha256, final_bytes) = hash_file(&database_path)?;
    ensure!(
        final_sha256 == catalog.database.sha256 && final_bytes == catalog.database.bytes,
        "public catalog database changed while the population was read"
    );
    Ok(entries)
}

fn validate_population_entries(entries: &[PopulationEntry], expected_count: u64) -> Result<()> {
    ensure!(
        entries.len() as u64 == expected_count && expected_count == EXPECTED_POPULATION_COUNT,
        "population contains {} rows, expected {expected_count}",
        entries.len()
    );
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    for entry in entries {
        validate_material_public_id(&entry.material_public_id)?;
        ensure!(
            ids.insert(entry.material_public_id.as_str()),
            "population repeats material_public_id {}",
            entry.material_public_id
        );
        let name = &entry.canonical_name_snapshot;
        validate_canonical_name(name, &entry.material_public_id)?;
        ensure!(
            names.insert(name.as_str()),
            "population repeats canonical name {name}"
        );
    }
    Ok(())
}

fn validate_canonical_name(name: &str, material_public_id: &str) -> Result<()> {
    ensure!(
        !name.is_empty(),
        "population contains an empty canonical name"
    );
    ensure!(
        name.chars().count() <= 500,
        "canonical name exceeds 500 Unicode scalars for {material_public_id}"
    );
    ensure!(
        name.nfc().eq(name.chars()),
        "canonical name is not NFC for {material_public_id}"
    );
    ensure!(
        !name.chars().any(char::is_control),
        "canonical name contains a control character for {material_public_id}"
    );
    Ok(())
}

fn validate_material_public_id(value: &str) -> Result<()> {
    let Some(hex) = value.strip_prefix("mat_") else {
        bail!("invalid material_public_id {value}");
    };
    ensure!(
        hex.len() == 32
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "invalid material_public_id {value}"
    );
    Ok(())
}

fn build_baseline_selection(
    population: &PopulationSnapshot,
    population_binding: &ArtifactBinding,
) -> Result<BaselineSelection> {
    let mut ranked = population
        .entries
        .iter()
        .cloned()
        .map(|entry| RankedPopulationEntry {
            digest: baseline_digest(&entry.material_public_id),
            entry,
        })
        .collect::<Vec<_>>();
    ranked.sort_unstable_by(|left, right| {
        left.digest.cmp(&right.digest).then_with(|| {
            left.entry
                .material_public_id
                .as_bytes()
                .cmp(right.entry.material_public_id.as_bytes())
        })
    });

    ensure!(
        ranked_id_lines_sha256(&ranked) == EXPECTED_ALL_RANKED_ID_LINES_SHA256,
        "full baseline ranking regression anchor drifted"
    );
    ensure!(
        ranked_id_lines_sha256(&ranked[..BASELINE_COUNT]) == EXPECTED_TOP_60_ID_LINES_SHA256,
        "top-60 baseline ranking regression anchor drifted"
    );

    let entries = ranked
        .into_iter()
        .take(BASELINE_COUNT)
        .enumerate()
        .map(|(index, ranked)| BaselineEntry {
            position: (index + 1) as u32,
            material_public_id: ranked.entry.material_public_id,
            canonical_name_snapshot: ranked.entry.canonical_name_snapshot,
            selection_kind: "baseline".to_string(),
            split: if index < BASELINE_DEVELOPMENT_COUNT {
                "development"
            } else {
                "holdout"
            }
            .to_string(),
            selection_digest: format_sha256_identifier(&ranked.digest),
        })
        .collect::<Vec<_>>();
    validate_baseline_anchors(&entries)?;

    Ok(BaselineSelection {
        format: "waajacu-cod-baseline-selection".to_string(),
        schema_version: 1,
        pilot_id: PILOT_ID.to_string(),
        population: BaselinePopulationBinding {
            release_id: population.source.release_id.clone(),
            mineral_count: population.count,
            snapshot_path: population_binding.path.clone(),
            snapshot_sha256: population_binding.sha256.clone(),
        },
        selection_policy: BaselineSelectionPolicy {
            version: "questionnaire-pilot-selection-v1".to_string(),
            salt: BASELINE_SALT.to_string(),
            hash_input: "UTF8(salt) || 0x00 || UTF8(public_id)".to_string(),
            ranking: "ascending_sha256_bytes_then_ascending_utf8_public_id".to_string(),
            development_ranks: "1-45".to_string(),
            holdout_ranks: "46-60".to_string(),
        },
        counts: BaselineCounts {
            total: 60,
            development: 45,
            holdout: 15,
        },
        entries,
    })
}

fn baseline_digest(material_public_id: &str) -> [u8; 32] {
    let mut context = DigestContext::new(&SHA256);
    context.update(BASELINE_SALT.as_bytes());
    context.update(&[0_u8]);
    context.update(material_public_id.as_bytes());
    context
        .finish()
        .as_ref()
        .try_into()
        .expect("SHA-256 always has 32 bytes")
}

fn ranked_id_lines_sha256(ranked: &[RankedPopulationEntry]) -> String {
    let mut context = DigestContext::new(&SHA256);
    for ranked in ranked {
        context.update(ranked.entry.material_public_id.as_bytes());
        context.update(b"\n");
    }
    format_sha256_identifier(context.finish().as_ref())
}

fn validate_baseline_anchors(entries: &[BaselineEntry]) -> Result<()> {
    ensure!(
        entries.len() == BASELINE_COUNT,
        "baseline must contain 60 entries"
    );
    for (position, id, name, digest, split) in [
        (
            1,
            "mat_ed21ff65c82544c1f32742d62ba72e0e",
            "Piypite",
            "sha256:001edf284ad58ab4391219d02a017c0c1e16ad86f29aa577d736a1f35e7e4a04",
            "development",
        ),
        (
            45,
            "mat_9594e46803b54845c3959a8df1703c94",
            "Hydroxylbenyacarite",
            "sha256:021b4e03abbdea2e649bd6c03d2d6f4403661bdcedbd5cb1c5823f0c7722d3d6",
            "development",
        ),
        (
            46,
            "mat_42c3f138e60acde15fabe899a30b3362",
            "Quartz",
            "sha256:02267ab9db2cd11b4938fca5e0777190904e79cf893797b01087da5a2fa5b03b",
            "holdout",
        ),
        (
            60,
            "mat_6285d195a7238143434586596651b16d",
            "Katophorite",
            "sha256:02dafe9b0ea628b1183b0ce9e5c9b914a62b0e16658f829e4f79a6921427f14c",
            "holdout",
        ),
    ] {
        let entry = &entries[position - 1];
        ensure!(
            entry.position == position as u32
                && entry.material_public_id == id
                && entry.canonical_name_snapshot == name
                && entry.selection_digest == digest
                && entry.split == split,
            "baseline regression anchor drifted at rank {position}"
        );
    }
    Ok(())
}

fn build_query_plan() -> Result<QueryPlan> {
    let headers = BTreeMap::from([
        ("Accept".to_string(), "application/json".to_string()),
        ("Accept-Encoding".to_string(), "identity".to_string()),
        ("Cache-Control".to_string(), "no-cache".to_string()),
        ("User-Agent".to_string(), USER_AGENT.to_string()),
    ]);
    let mut requests = Vec::with_capacity(REQUEST_COUNT);
    let mut request_digests = BTreeSet::new();
    for ordinal in 0..REQUEST_COUNT {
        let ddd_prefix = format!("{ordinal:03}");
        let url = format!(
            "{DISCOVERY_ENDPOINT}?format=json&id={ddd_prefix}%25&include_duplicates=1&include_errors=1&include_theoretical=1"
        );
        let identity = serde_json::json!({
            "method": "GET",
            "url": url,
            "headers": headers,
        });
        let request_sha256 = sha256_identifier(&canonical_json_bytes(&identity)?);
        ensure!(
            request_digests.insert(request_sha256.clone()),
            "query plan repeats request_sha256 {request_sha256}"
        );
        requests.push(PlannedRequest {
            ordinal: ordinal as u32,
            ddd_prefix,
            method: "GET".to_string(),
            url,
            request_sha256,
        });
    }
    validate_query_plan_anchors(&requests)?;

    Ok(QueryPlan {
        format: "waajacu-cod-metadata-discovery-query-plan".to_string(),
        schema_version: 1,
        pilot_id: PILOT_ID.to_string(),
        plan_revision: 1,
        status: "frozen_before_retrieval".to_string(),
        endpoint: DISCOVERY_ENDPOINT.to_string(),
        request_digest_policy: RequestDigestPolicy {
            algorithm: "sha256".to_string(),
            canonicalization: "waajacu-canonical-json-v1".to_string(),
            input: "waajacu-canonical-json-v1({headers: transport.headers, method: request.method, url: request.url})".to_string(),
            prefix: "sha256:".to_string(),
        },
        response_validation: cod_response_validation_policy(),
        transport: FrozenTransport {
            scheme: "https".to_string(),
            host: "www.crystallography.net".to_string(),
            port: 443,
            method: "GET".to_string(),
            headers,
            accepted_success_content_types: vec![
                "application/json".to_string(),
                "text/json".to_string(),
                "text/plain".to_string(),
            ],
            concurrency: 1,
            minimum_attempt_start_interval_ms: 12_000,
            connect_timeout_ms: 15_000,
            attempt_timeout_ms: 180_000,
            maximum_attempts_per_request: 4,
            attempt_backoff_ms: vec![0, 12_000, 24_000, 48_000],
            retry_after_cap_ms: 300_000,
            retryable_http_statuses: vec![408, 425, 429, 500, 502, 503, 504],
            redirects: "forbidden".to_string(),
            authentication: "forbidden".to_string(),
            cookies: "forbidden".to_string(),
            request_body: "forbidden".to_string(),
            conditional_requests: "forbidden".to_string(),
            jitter: "forbidden".to_string(),
        },
        limits: QueryLimits {
            planned_requests: 1_000,
            maximum_response_header_bytes: 65_536,
            maximum_success_response_bytes: 134_217_728,
            maximum_error_response_bytes: 1_048_576,
            maximum_total_raw_response_bytes: 8_589_934_592,
            maximum_json_depth: 64,
            maximum_rows_per_shard: 10_000,
        },
        requests,
        safety_assertions: SafetyAssertions {
            ordinals_are_contiguous_zero_through_999: true,
            ddd_prefix_is_zero_padded_ordinal: true,
            requests_are_in_ordinal_order: true,
            every_url_matches_the_frozen_template: true,
            request_digests_match_the_frozen_policy: true,
            prefixes_form_a_complete_nonoverlapping_partition: true,
            raw_response_bytes_are_immutable: true,
            candidate_discovery_precedes_normalization: true,
            normalized_output_is_forbidden: true,
            catalog_and_registry_database_writes_are_forbidden: true,
        },
    })
}

fn cod_response_validation_policy() -> CodResponseValidationPolicy {
    CodResponseValidationPolicy {
        profile: "cod_metadata_json_rows_v1".to_string(),
        top_level: "array_of_objects".to_string(),
        text_encoding: "utf8".to_string(),
        byte_order_mark: "forbidden".to_string(),
        duplicate_object_keys: "forbidden".to_string(),
        depth: DepthValidationPolicy {
            root_value_depth: 1,
            maximum: MAX_JSON_DEPTH as u32,
        },
        rows: RowValidationPolicy {
            maximum: MAX_ROWS_PER_SHARD as u32,
            enforcement: "during_top_level_array_parse".to_string(),
        },
        scalar_policy: "decimal_string_or_unsigned_json_integer_v1".to_string(),
        file: FileValidationPolicy {
            canonical_form: "exactly_seven_ascii_digits".to_string(),
            request_prefix_match: "first_three_digits".to_string(),
        },
        svnrevision: RevisionValidationPolicy { minimum: 1 },
        unknown_row_fields: "allowed".to_string(),
        row_order: "ignored".to_string(),
        record_key: "canonical_file_and_numeric_svnrevision".to_string(),
        duplicate_record_keys: "identical_rows_allowed_conflicting_rows_block".to_string(),
    }
}

fn validate_query_plan_anchors(requests: &[PlannedRequest]) -> Result<()> {
    ensure!(
        requests.len() == REQUEST_COUNT,
        "query plan must contain 1000 requests"
    );
    for (ordinal, expected_url, expected_request_sha256) in [
        (
            0,
            "https://www.crystallography.net/cod/result?format=json&id=000%25&include_duplicates=1&include_errors=1&include_theoretical=1",
            "sha256:0bc33a749de0f3b5233d5d80268a9bad6189fa628d2f8ff88aeb2378582ad067",
        ),
        (
            999,
            "https://www.crystallography.net/cod/result?format=json&id=999%25&include_duplicates=1&include_errors=1&include_theoretical=1",
            "sha256:5019fa6485298662ce430e9ee3deca36a0e699e9b18f63273ced929a969a44e9",
        ),
    ] {
        let request = &requests[ordinal];
        ensure!(
            request.ordinal == ordinal as u32
                && request.url == expected_url
                && request.request_sha256 == expected_request_sha256,
            "query-plan regression anchor drifted at ordinal {ordinal}"
        );
    }
    Ok(())
}

fn artifact_binding(path: &str, bytes: &[u8]) -> ArtifactBinding {
    ArtifactBinding {
        path: path.to_string(),
        sha256: sha256_identifier(bytes),
        bytes: bytes.len() as u64,
    }
}

fn canonical_file_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let value = serde_json::to_value(value).context("failed to serialize canonical artifact")?;
    let mut bytes = canonical_json_bytes(&value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn canonical_json_bytes(value: &Value) -> Result<Vec<u8>> {
    let mut output = String::new();
    write_canonical_json(value, &mut output)?;
    Ok(output.into_bytes())
}

fn write_canonical_json(value: &Value, output: &mut String) -> Result<()> {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        Value::Number(value) => output.push_str(&value.to_string()),
        Value::String(value) => output.push_str(&serde_json::to_string(value)?),
        Value::Array(values) => {
            output.push('[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                write_canonical_json(value, output)?;
            }
            output.push(']');
        }
        Value::Object(values) => {
            output.push('{');
            let mut keys = values.keys().collect::<Vec<_>>();
            keys.sort_unstable_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
            for (index, key) in keys.into_iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                output.push_str(&serde_json::to_string(key)?);
                output.push(':');
                write_canonical_json(&values[key], output)?;
            }
            output.push('}');
        }
    }
    Ok(())
}

fn ensure_canonical_json_file(bytes: &[u8], label: &str) -> Result<()> {
    let value: Value = serde_json::from_slice(bytes)
        .with_context(|| format!("failed to parse {label} as JSON"))?;
    let mut expected = canonical_json_bytes(&value)?;
    expected.push(b'\n');
    ensure!(
        bytes == expected,
        "{label} is not canonical JSON plus one LF"
    );
    Ok(())
}

fn verify_canonical_file(path: &Path, expected: &[u8], binding: &ArtifactBinding) -> Result<()> {
    let bytes = read_regular_file(path, "prepared artifact")?;
    ensure_canonical_json_file(&bytes, &binding.path)?;
    ensure!(
        bytes.len() as u64 == binding.bytes && sha256_identifier(&bytes) == binding.sha256,
        "prepared artifact hash or byte count does not match manifest: {}",
        binding.path
    );
    ensure!(
        bytes == expected,
        "prepared artifact does not match committed inputs: {}",
        binding.path
    );
    Ok(())
}

fn sha256_identifier(bytes: &[u8]) -> String {
    let digest = ring::digest::digest(&SHA256, bytes);
    format_sha256_identifier(digest.as_ref())
}

fn format_sha256_identifier(digest: &[u8]) -> String {
    debug_assert_eq!(digest.len(), 32);
    let mut value = String::with_capacity("sha256:".len() + 64);
    value.push_str("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut value, "{byte:02x}").expect("writing to a String cannot fail");
    }
    value
}

fn hash_file(path: &Path) -> Result<(String, u64)> {
    let mut file = File::open(path)
        .with_context(|| format!("failed to open file for hashing: {}", path.display()))?;
    let mut context = DigestContext::new(&SHA256);
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .with_context(|| format!("failed to hash file: {}", path.display()))?;
        if count == 0 {
            break;
        }
        context.update(&buffer[..count]);
        total = total
            .checked_add(count as u64)
            .context("file byte count overflow")?;
    }
    Ok((format_sha256_identifier(context.finish().as_ref()), total))
}

fn validate_sha256_identifier(value: &str, label: &str) -> Result<()> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        bail!("{label} must start with sha256:");
    };
    ensure!(
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "{label} must contain 64 lowercase hexadecimal characters"
    );
    Ok(())
}

fn pointer_string<'a>(value: &'a Value, pointer: &str) -> Result<&'a str> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .with_context(|| format!("missing string at JSON pointer {pointer}"))
}

fn pointer_u64(value: &Value, pointer: &str) -> Result<u64> {
    value
        .pointer(pointer)
        .and_then(Value::as_u64)
        .with_context(|| format!("missing unsigned integer at JSON pointer {pointer}"))
}

fn expect_string(value: &Value, pointer: &str, expected: &str) -> Result<()> {
    let actual = pointer_string(value, pointer)?;
    ensure!(
        actual == expected,
        "value drifted at {pointer}: expected {expected:?}, found {actual:?}"
    );
    Ok(())
}

fn expect_u64(value: &Value, pointer: &str, expected: u64) -> Result<()> {
    let actual = pointer_u64(value, pointer)?;
    ensure!(
        actual == expected,
        "value drifted at {pointer}: expected {expected}, found {actual}"
    );
    Ok(())
}

fn safe_repo_file(repo_root: &Path, relative: &str, label: &str) -> Result<PathBuf> {
    let path = safe_repo_path(repo_root, relative, label)?;
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("failed to inspect {label}: {}", path.display()))?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "{label} must be a regular non-symlink file: {}",
        path.display()
    );
    Ok(path)
}

fn safe_repo_path(repo_root: &Path, relative: &str, label: &str) -> Result<PathBuf> {
    let relative = Path::new(relative);
    ensure!(!relative.is_absolute(), "{label} path must be relative");
    ensure!(
        relative
            .components()
            .all(|component| matches!(component, Component::Normal(_))),
        "{label} path contains an unsafe component: {}",
        relative.display()
    );
    let mut path = repo_root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            unreachable!("components were validated")
        };
        path.push(name);
        let metadata = fs::symlink_metadata(&path)
            .with_context(|| format!("failed to inspect {label}: {}", path.display()))?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "{label} path cannot traverse a symlink: {}",
            path.display()
        );
    }
    Ok(path)
}

fn require_real_directory(path: &Path, label: &str) -> Result<PathBuf> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {label}: {}", path.display()))?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "{label} must be a real non-symlink directory: {}",
        path.display()
    );
    path.canonicalize()
        .with_context(|| format!("failed to canonicalize {label}: {}", path.display()))
}

fn read_regular_file(path: &Path, label: &str) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {label}: {}", path.display()))?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "{label} must be a regular non-symlink file: {}",
        path.display()
    );
    fs::read(path).with_context(|| format!("failed to read {label}: {}", path.display()))
}

fn create_new_output_directory(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => bail!(
            "output directory must not already exist: {}",
            path.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect output path: {}", path.display()))
        }
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    require_real_directory(parent, "output parent")?;
    fs::create_dir(path)
        .with_context(|| format!("failed to create output directory: {}", path.display()))
}

fn write_new_file(output: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    let path = output.join(name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .with_context(|| format!("failed to create prepared artifact: {}", path.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("failed to write prepared artifact: {}", path.display()))?;
    file.sync_all().with_context(|| {
        format!(
            "failed to synchronize prepared artifact: {}",
            path.display()
        )
    })
}

fn validate_prepared_directory_entries(input: &Path) -> Result<()> {
    let expected = BTreeSet::from([
        POPULATION_SNAPSHOT_FILE,
        BASELINE_SELECTION_FILE,
        QUERY_PLAN_FILE,
        PREPARATION_MANIFEST_FILE,
    ]);
    let mut actual = BTreeSet::new();
    for entry in fs::read_dir(input)
        .with_context(|| format!("failed to inspect prepared directory: {}", input.display()))?
    {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "prepared directory contains a non-file or symlink: {}",
            entry.path().display()
        );
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("prepared directory contains a non-Unicode file name"))?;
        actual.insert(name);
    }
    let actual_refs = actual.iter().map(String::as_str).collect::<BTreeSet<_>>();
    ensure!(
        actual_refs == expected,
        "prepared directory must contain exactly the four managed artifacts"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn canonical_json_sorts_utf8_keys_recursively() -> Result<()> {
        let value = serde_json::json!({
            "z": 1,
            "a": {"two": 2, "one": 1},
            "list": [{"b": false, "a": true}]
        });
        assert_eq!(
            String::from_utf8(canonical_json_bytes(&value)?)?,
            r#"{"a":{"one":1,"two":2},"list":[{"a":true,"b":false}],"z":1}"#
        );
        Ok(())
    }

    #[test]
    fn baseline_digest_includes_the_literal_zero_separator() {
        assert_eq!(
            format_sha256_identifier(&baseline_digest("mat_ed21ff65c82544c1f32742d62ba72e0e")),
            "sha256:001edf284ad58ab4391219d02a017c0c1e16ad86f29aa577d736a1f35e7e4a04"
        );
    }

    #[test]
    fn query_plan_has_every_ddd_percent_partition_and_frozen_transport() -> Result<()> {
        let plan = build_query_plan()?;
        assert_eq!(plan.requests.len(), 1_000);
        assert_eq!(plan.requests[0].ddd_prefix, "000");
        assert_eq!(plan.requests[999].ddd_prefix, "999");
        assert_eq!(
            plan.requests[0].request_sha256,
            "sha256:0bc33a749de0f3b5233d5d80268a9bad6189fa628d2f8ff88aeb2378582ad067"
        );
        assert_eq!(
            plan.requests[999].request_sha256,
            "sha256:5019fa6485298662ce430e9ee3deca36a0e699e9b18f63273ced929a969a44e9"
        );
        assert_eq!(plan.transport.concurrency, 1);
        assert_eq!(plan.transport.minimum_attempt_start_interval_ms, 12_000);
        assert_eq!(plan.limits.planned_requests, 1_000);
        assert_eq!(
            plan.transport.accepted_success_content_types,
            ["application/json", "text/json", "text/plain"]
        );
        assert_eq!(plan.limits.maximum_response_header_bytes, 65_536);
        assert_eq!(plan.response_validation, cod_response_validation_policy());
        assert!(plan.safety_assertions.normalized_output_is_forbidden);
        Ok(())
    }

    #[tokio::test]
    async fn fetch_rejects_zero_before_touching_paths_or_network() {
        let missing = Path::new("definitely-missing-cod-pilot-path");
        let error = fetch(missing, missing, missing, Some(0))
            .await
            .expect_err("zero must be rejected");
        assert!(error.to_string().contains("between 1 and 1000"));
    }

    #[test]
    fn execution_attempt_round_trip_preserves_present_null_headers() -> Result<()> {
        let attempt = ExecutionAttempt {
            attempt_number: 1,
            started_at: "2026-08-29T18:56:19.832Z".to_string(),
            finished_at: "2026-08-29T18:56:20.793Z".to_string(),
            elapsed_ms: 961,
            outcome: "http_response".to_string(),
            status_code: Some(200),
            content_type: Some(Some("application/json".to_string())),
            retry_after: Some(None),
            raw_body: Some(RawBodyBinding {
                path: "data/pilots/example".to_string(),
                sha256: "sha256:ce8124bc1b0fbc0cb5cd47338ca0c7d5f5446d79936e443a201d96b192a7bd65"
                    .to_string(),
                bytes: 4,
            }),
            bytes_received: None,
            error_kind: None,
            retry_disposition: "success".to_string(),
        };

        let encoded = serde_json::to_vec(&attempt)?;
        assert!(String::from_utf8_lossy(&encoded).contains(r#""retry_after":null"#));
        let decoded: ExecutionAttempt = serde_json::from_slice(&encoded)?;
        assert_eq!(decoded, attempt);
        Ok(())
    }

    #[test]
    fn cod_response_validation_enforces_wire_identity_and_record_keys() -> Result<()> {
        let valid = br#"[
            {"file":"1234567","svnrevision":"42","extra":{"value":true}},
            {"file":1234568,"svnrevision":43},
            {"file":1234567,"svnrevision":42,"extra":{"value":true}}
        ]"#;
        let validation = validate_cod_response(valid, "123")?;
        assert_eq!(validation.row_count, 3);
        assert!(validation.maximum_depth >= 3);

        for invalid in [
            br#"[{"file":"1234567","svnrevision":"01"}]"#.as_slice(),
            br#"[{"file":123456,"svnrevision":1}]"#.as_slice(),
            br#"[{"file":"1244567","svnrevision":1}]"#.as_slice(),
            br#"[{"file":"1234567","svnrevision":0}]"#.as_slice(),
            br#"[{"file":"1234567","svnrevision":1},{"file":1234567,"svnrevision":"1","different":true}]"#.as_slice(),
            br#"[{"file":"1234567","file":"1234568","svnrevision":1}]"#.as_slice(),
        ] {
            assert!(validate_cod_response(invalid, "123").is_err());
        }
        assert!(validate_cod_response(&[0xef, 0xbb, 0xbf, b'[', b']'], "123").is_err());
        assert!(validate_cod_response(&[0xff], "123").is_err());
        Ok(())
    }

    #[test]
    fn cod_response_validation_enforces_depth_and_row_limits() {
        let nested = format!(
            "[{{\"file\":\"1234567\",\"svnrevision\":1,\"nested\":{}0{}}}]",
            "[".repeat(MAX_JSON_DEPTH),
            "]".repeat(MAX_JSON_DEPTH)
        );
        assert!(matches!(
            validate_cod_response(nested.as_bytes(), "123"),
            Err(ResponseValidationError::JsonDepthLimit(_))
        ));

        let row = r#"{"file":"1234567","svnrevision":1}"#;
        let too_many = format!("[{}]", vec![row; MAX_ROWS_PER_SHARD + 1].join(","));
        assert!(matches!(
            validate_cod_response(too_many.as_bytes(), "123"),
            Err(ResponseValidationError::RowLimit(rows)) if rows == MAX_ROWS_PER_SHARD + 1
        ));
    }

    #[test]
    fn response_identity_helpers_are_exact() {
        let mut headers = HeaderMap::new();
        assert!(has_identity_content_encoding(&headers));
        headers.insert("content-encoding", HeaderValue::from_static("Identity"));
        assert!(has_identity_content_encoding(&headers));
        headers.append("content-encoding", HeaderValue::from_static("identity"));
        assert!(has_identity_content_encoding(&headers));
        headers.insert("content-encoding", HeaderValue::from_static("gzip"));
        assert!(!has_identity_content_encoding(&headers));

        let planned = "https://www.crystallography.net/cod/result?format=json&id=123%25&include_duplicates=1&include_errors=1&include_theoretical=1";
        assert!(exact_response_url_matches(planned, planned));
        assert!(!exact_response_url_matches(
            &format!("{planned}#redirected"),
            planned
        ));
    }

    #[test]
    fn cas_reuses_exact_bytes_and_never_replaces_a_mismatch() -> Result<()> {
        let temporary = TempDir::new()?;
        let repo_root = temporary.path().canonicalize()?;
        let pilot_root = repo_root.join(PILOT_ROOT_RELATIVE);
        fs::create_dir_all(&pilot_root)?;
        let pilot_root = pilot_root.canonicalize()?;
        let context = ExecutionContext {
            repo_root,
            pilot_root,
            plan: build_query_plan()?,
            plan_binding: ExecutionPlanBinding {
                path: format!("{PILOT_ROOT_RELATIVE}/prepared/{QUERY_PLAN_FILE}"),
                sha256: "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                    .to_string(),
                request_count: REQUEST_COUNT as u32,
            },
        };
        let binding = store_raw_body(&context, b"[]")?;
        assert_eq!(store_raw_body(&context, b"[]")?, binding);
        assert_eq!(read_and_validate_raw_body(&context, &binding)?, b"[]");

        let object_path = context.repo_root.join(&binding.path);
        fs::write(&object_path, b"tampered")?;
        assert!(store_raw_body(&context, b"[]").is_err());
        assert_eq!(fs::read(object_path)?, b"tampered");
        Ok(())
    }

    #[test]
    fn fetch_lock_is_exclusive_and_stale_locks_fail_closed() -> Result<()> {
        let temporary = TempDir::new()?;
        let first = FetchLock::acquire(temporary.path())?;
        assert!(FetchLock::acquire(temporary.path()).is_err());
        drop(first);
        let second = FetchLock::acquire(temporary.path())?;
        drop(second);

        fs::create_dir(temporary.path().join(FETCH_LOCK_NAME))?;
        assert!(FetchLock::acquire(temporary.path()).is_err());
        Ok(())
    }

    fn recovery_fixture() -> Result<(TempDir, ExecutionContext, DiscoveryExecutionIndex)> {
        let temporary = TempDir::new()?;
        let repo_root = temporary.path().canonicalize()?;
        let pilot_root = repo_root.join(PILOT_ROOT_RELATIVE);
        fs::create_dir_all(&pilot_root)?;
        let context = ExecutionContext {
            repo_root,
            pilot_root: pilot_root.canonicalize()?,
            plan: build_query_plan()?,
            plan_binding: ExecutionPlanBinding {
                path: format!("{PILOT_ROOT_RELATIVE}/prepared/{QUERY_PLAN_FILE}"),
                sha256: format!("sha256:{}", "0".repeat(64)),
                request_count: REQUEST_COUNT as u32,
            },
        };
        let mut index = new_execution_index(context.plan_binding.clone());
        index.index_revision = 10;
        index.started_at = "2026-01-01T00:00:00.000Z".to_string();
        index.updated_at = "2026-01-01T00:01:29.000Z".to_string();
        let raw = store_raw_body(&context, b"[]")?;
        let mut success = reserved_attempt(1);
        success.started_at = "2026-01-01T00:00:00.000Z".to_string();
        success.finished_at = success.started_at.clone();
        success.outcome = "http_response".to_string();
        success.status_code = Some(200);
        success.content_type = Some(Some("application/json".to_string()));
        success.retry_after = Some(None);
        success.raw_body = Some(raw.clone());
        success.error_kind = None;
        success.retry_disposition = "success".to_string();
        let request = &context.plan.requests[0];
        index.successful_receipts.push(SuccessfulReceipt {
            ordinal: 0,
            ddd_prefix: request.ddd_prefix.clone(),
            method: request.method.clone(),
            url: request.url.clone(),
            request_sha256: request.request_sha256.clone(),
            attempts: vec![success.clone()],
            successful_attempt_number: 1,
            response: SuccessfulResponse {
                status_code: 200,
                content_type: "application/json".to_string(),
                retrieved_at: success.finished_at.clone(),
                raw_body: raw,
                json_validation: JsonValidation {
                    valid: true,
                    maximum_depth: 1,
                    row_count: 0,
                },
            },
        });
        let base = parse_timestamp("2026-01-01T00:00:00.000Z", "fixture base")?;
        let attempts = [12, 24, 48, 96]
            .into_iter()
            .enumerate()
            .map(|(position, second)| {
                let mut attempt = reserved_attempt(position + 1);
                let time = base + ChronoDuration::seconds(second);
                attempt.started_at = time.to_rfc3339_opts(SecondsFormat::Millis, true);
                attempt.finished_at = attempt.started_at.clone();
                attempt.error_kind = Some("connect".to_string());
                attempt
            })
            .collect();
        index.halted_request = Some(halted_request(
            &context.plan.requests[1],
            "transport_error",
            attempts,
        ));
        index.updated_at = "2026-01-01T00:01:36.000Z".to_string();
        index.counts = recompute_execution_counts(&index)?;
        validate_execution_index(&context, &index)?;
        Ok((temporary, context, index))
    }

    #[test]
    fn reviewed_recovery_preserves_receipts_counts_and_exact_failed_index() -> Result<()> {
        let (_temporary, context, index) = recovery_fixture()?;
        let recovered = reviewed_transport_recovery(
            &context,
            &index,
            "codex/operator",
            "Connectivity verified; no HTTP response received",
            "2026-01-01T00:10:00.000Z".to_string(),
        )?;
        assert_eq!(recovered.schema_version, 2);
        assert_eq!(recovered.successful_receipts, index.successful_receipts);
        assert_eq!(recovered.counts, index.counts);
        assert!(recovered.halted_request.is_none());
        assert!(
            !recovered
                .safety_assertions
                .request_start_spacing_and_retry_policy_were_enforced
        );
        assert_eq!(
            read_and_validate_raw_body(
                &context,
                &recovered.transport_recoveries[0].previous_index
            )?,
            canonical_file_bytes(&index)?
        );
        validate_execution_index(&context, &recovered)?;
        let mut duplicate = recovered.clone();
        duplicate.halted_request = index.halted_request.clone();
        for attempt in &mut duplicate.halted_request.as_mut().unwrap().attempts {
            let shifted = parse_timestamp(&attempt.started_at, "fixture attempt")?
                + ChronoDuration::minutes(20);
            attempt.started_at = shifted.to_rfc3339_opts(SecondsFormat::Millis, true);
            attempt.finished_at = attempt.started_at.clone();
        }
        duplicate.counts = recompute_execution_counts(&duplicate)?;
        assert!(reviewed_transport_recovery(
            &context,
            &duplicate,
            "operator",
            "Repeat",
            "2026-01-01T00:30:00.000Z".to_string()
        )
        .unwrap_err()
        .to_string()
        .contains("already received"));
        Ok(())
    }

    #[test]
    fn recovered_success_keeps_failures_and_cannot_precede_review() -> Result<()> {
        let (_temporary, context, index) = recovery_fixture()?;
        let mut recovered = reviewed_transport_recovery(
            &context,
            &index,
            "operator",
            "Reviewed connectivity",
            "2026-01-01T00:10:00.000Z".to_string(),
        )?;
        let planned = &context.plan.requests[1];
        let mut receipt = recovered.successful_receipts[0].clone();
        receipt.ordinal = 1;
        receipt.ddd_prefix = planned.ddd_prefix.clone();
        receipt.url = planned.url.clone();
        receipt.request_sha256 = planned.request_sha256.clone();
        receipt.attempts[0].started_at = "2026-01-01T00:10:00.000Z".to_string();
        receipt.attempts[0].finished_at = receipt.attempts[0].started_at.clone();
        receipt.response.retrieved_at = receipt.attempts[0].finished_at.clone();
        recovered.successful_receipts.push(receipt);
        recovered.counts = recompute_execution_counts(&recovered)?;
        assert_eq!(recovered.counts.attempts, 6);
        assert_eq!(recovered.counts.transport_error_attempts, 4);
        validate_execution_index(&context, &recovered)?;
        recovered.successful_receipts[1].attempts[0].started_at =
            "2026-01-01T00:09:59.000Z".to_string();
        assert!(validate_execution_index(&context, &recovered).is_err());
        let receipts = vec![index.successful_receipts[0].clone(); REQUEST_COUNT];
        let original = execution_snapshot_identity(&index.plan.sha256, &receipts, &[])?;
        let reviewed = execution_snapshot_identity(
            &index.plan.sha256,
            &receipts,
            &recovered.transport_recoveries,
        )?;
        assert_ne!(original.sha256, reviewed.sha256);
        Ok(())
    }

    #[test]
    fn recovery_rejects_early_review_reserved_attempts_and_response_failures() -> Result<()> {
        let (_temporary, context, index) = recovery_fixture()?;
        assert!(reviewed_transport_recovery(
            &context,
            &index,
            "operator",
            "Reviewed",
            "2026-01-01T00:06:35.000Z".to_string()
        )
        .is_err());
        for kind in ["attempt_reserved", "body", "decode", "request"] {
            let mut invalid = index.halted_request.clone().unwrap();
            invalid.attempts[0].error_kind = Some(kind.to_string());
            assert!(
                validate_transport_recovery_halt(&invalid, "2026-01-01T00:10:00.000Z").is_err()
            );
        }
        let mut invalid = index.halted_request.clone().unwrap();
        invalid.reason = "invalid_json".to_string();
        assert!(validate_transport_recovery_halt(&invalid, "2026-01-01T00:10:00.000Z").is_err());
        assert!(reviewed_transport_recovery(
            &context,
            &index,
            "operator",
            "",
            "2026-01-01T00:10:00.000Z".to_string()
        )
        .is_err());
        Ok(())
    }

    #[test]
    fn recovered_history_fails_closed_on_tampering_and_wrong_version() -> Result<()> {
        let (_temporary, context, index) = recovery_fixture()?;
        let recovered = reviewed_transport_recovery(
            &context,
            &index,
            "operator",
            "Reviewed connectivity",
            "2026-01-01T00:10:00.000Z".to_string(),
        )?;
        let mut invalid = recovered.clone();
        invalid.schema_version = 1;
        assert!(validate_execution_index(&context, &invalid).is_err());
        invalid = recovered.clone();
        invalid.transport_recoveries[0].halted_request.attempts[0].elapsed_ms = 1;
        assert!(validate_execution_index(&context, &invalid).is_err());
        invalid = recovered.clone();
        invalid.counts.attempts -= 4;
        assert!(validate_execution_index(&context, &invalid).is_err());
        fs::write(
            context
                .repo_root
                .join(&recovered.transport_recoveries[0].previous_index.path),
            b"damaged",
        )?;
        assert!(validate_execution_index(&context, &recovered).is_err());
        Ok(())
    }

    #[test]
    fn retry_wait_and_halt_reason_are_verified() -> Result<()> {
        let plan = build_query_plan()?;
        let mut previous = reserved_attempt(1);
        previous.started_at = "2026-01-01T00:00:00.000Z".to_string();
        previous.finished_at = "2026-01-01T00:00:01.000Z".to_string();
        let mut current = reserved_attempt(2);
        current.started_at = "2026-01-01T00:00:12.999Z".to_string();
        current.finished_at = current.started_at.clone();
        assert!(validate_retry_wait(&previous, &current).is_err());
        current.started_at = "2026-01-01T00:00:13.000Z".to_string();
        current.finished_at = current.started_at.clone();
        validate_retry_wait(&previous, &current)?;

        let request = &plan.requests[0];
        let mut halted = halted_request(request, "operator_interrupt", vec![previous]);
        validate_halted_request(&halted)?;
        halted.reason = "unknown_reason".to_string();
        assert!(validate_halted_request(&halted).is_err());
        Ok(())
    }

    #[test]
    fn population_names_reject_controls_without_rewriting_valid_names() {
        let id = "mat_ed21ff65c82544c1f32742d62ba72e0e";
        assert!(validate_canonical_name("Quartz", id).is_ok());
        assert!(validate_canonical_name("Quartz\n", id).is_err());
    }

    #[test]
    fn committed_inputs_prepare_verify_and_detect_tampering() -> Result<()> {
        let root = workspace_root()?;
        let temporary = TempDir::new()?;
        let output = temporary.path().join("prepared");

        let prepared = prepare(&root, &output)?;
        assert_eq!(prepared.inputs.public_catalog.mineral_count, 6_226);
        assert_eq!(
            prepared.inputs.public_catalog.database_sha256,
            EXPECTED_DATABASE_SHA256
        );
        assert_eq!(verify(&root, &output)?, prepared);

        let baseline_bytes = fs::read(output.join(BASELINE_SELECTION_FILE))?;
        let baseline: BaselineSelection = serde_json::from_slice(&baseline_bytes)?;
        validate_baseline_anchors(&baseline.entries)?;
        assert_eq!(baseline.entries[45].canonical_name_snapshot, "Quartz");

        let mut tampered = baseline_bytes;
        tampered.push(b' ');
        fs::write(output.join(BASELINE_SELECTION_FILE), tampered)?;
        assert!(verify(&root, &output).is_err());
        Ok(())
    }

    fn workspace_root() -> Result<PathBuf> {
        if let Some(path) = std::env::var_os("MINERALS_TEST_REPO_ROOT") {
            return PathBuf::from(path)
                .canonicalize()
                .context("failed to canonicalize MINERALS_TEST_REPO_ROOT");
        }
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .context("failed to locate workspace root")
    }
}
