use std::{
    fs::{self, File, OpenOptions},
    io::{self, ErrorKind, Read, Seek, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{bail, Context, Result};
use chrono::{SecondsFormat, Utc};
use getrandom::getrandom;
use ring::digest::{Context as DigestContext, SHA256};
use rusqlite::{params, Connection, OpenFlags, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};

const LIVE_DATABASE_FILE: &str = "minerals.db";
const COMPRESSION_BUFFER_BYTES: usize = 64 * 1024;
const BROTLI_QUALITY: i32 = 9;
const BROTLI_WINDOW_BITS: i32 = 22;
pub const PUBLIC_CATALOG_FORMAT: &str = "waajacu-public-catalog-v1";
pub const PUBLIC_CATALOG_SCHEMA_VERSION: u32 = 1;
pub const PUBLIC_CATALOG_MANIFEST_FILE: &str = "catalog-manifest.json";
const PUBLIC_CATALOG_PAGE_SIZE: i64 = 8192;
const PUBLIC_CATALOG_MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const PUBLIC_CATALOG_MAX_DATABASE_BYTES: u64 = 512 * 1024 * 1024;

const PUBLIC_TABLES: &[&str] = &[
    "catalog_meta",
    "evidence",
    "mineral_search",
    "mineral_search_config",
    "mineral_search_content",
    "mineral_search_data",
    "mineral_search_docsize",
    "mineral_search_idx",
    "minerals",
    "offers",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCatalogManifest {
    pub format: String,
    pub schema_version: u32,
    pub generated_at: String,
    pub release_id: String,
    pub mineral_count: u64,
    pub database: PublicCatalogDatabase,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCatalogDatabase {
    pub path: String,
    /// `sha256:` followed by a lowercase, 64-character SHA-256 hex digest.
    pub sha256: String,
    pub bytes: u64,
}

/// Builds a sanitized public SQLite projection and atomically publishes its
/// manifest into `output`.
///
/// The source database is opened read-only and all rows are copied from one
/// SQLite read transaction. The database is content-addressed and immutable;
/// an existing hashed database is verified rather than replaced. Brotli and
/// gzip representations are published beside it for transparent HTTP content
/// negotiation. The manifest is the final commit point and is replaced
/// atomically.
pub fn export_public_catalog(data_root: &Path, output: &Path) -> Result<PublicCatalogManifest> {
    let live_database = data_root.join(LIVE_DATABASE_FILE);
    require_regular_non_symlink_file(&live_database, "live registry database")?;
    let canonical_data_root = data_root.canonicalize().with_context(|| {
        format!(
            "failed to canonicalize live registry data root {}",
            data_root.display()
        )
    })?;
    let output = prepare_output_directory(output)?;
    if output.starts_with(&canonical_data_root) || canonical_data_root.starts_with(&output) {
        bail!(
            "public catalog output and private data root must be separate: {} and {}",
            output.display(),
            canonical_data_root.display()
        );
    }
    validate_manifest_destination(&output)?;
    let database_output = prepare_output_directory(&output.join("data"))?;

    let mut source = Connection::open_with_flags(
        &live_database,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| {
        format!(
            "failed to open live registry database {} read-only",
            live_database.display()
        )
    })?;
    source
        .busy_timeout(Duration::from_secs(5))
        .context("failed to configure public catalog source timeout")?;
    source
        .execute_batch("PRAGMA query_only = ON; PRAGMA foreign_keys = ON;")
        .context("failed to configure public catalog source connection")?;

    let generated = Utc::now();
    let generated_at = generated.to_rfc3339_opts(SecondsFormat::Millis, true);
    let offer_cutoff = generated.format("%Y-%m-%d %H:%M:%S").to_string();
    let release_seed = format!("{generated_at}\0{}", random_hex(32)?);
    let release_id = format!("sha256:{}", hash_bytes(release_seed.as_bytes()));

    let source_tx = source
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .context("failed to start the public catalog source snapshot")?;
    validate_source_schema(&source_tx)?;

    let mut database_temp = TemporaryFile::create(&database_output, "catalog-db", "sqlite3")?;
    let mut destination = Connection::open_with_flags(
        database_temp.path(),
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .context("failed to open the temporary public catalog database")?;
    destination
        .busy_timeout(Duration::from_secs(5))
        .context("failed to configure public catalog destination timeout")?;
    destination
        .execute_batch(
            r#"
            PRAGMA page_size = 8192;
            PRAGMA journal_mode = DELETE;
            PRAGMA synchronous = FULL;
            PRAGMA foreign_keys = ON;
            PRAGMA trusted_schema = OFF;
            PRAGMA user_version = 1;
            "#,
        )
        .context("failed to configure the public catalog database")?;

    let mineral_count = {
        let destination_tx = destination
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("failed to start the public catalog destination transaction")?;
        create_public_schema(&destination_tx)?;
        let mineral_count = copy_public_snapshot(
            &source_tx,
            &destination_tx,
            &generated_at,
            &release_id,
            &offer_cutoff,
        )?;
        destination_tx
            .commit()
            .context("failed to commit the public catalog database")?;
        mineral_count
    };

    source_tx
        .commit()
        .context("failed to finish the public catalog source snapshot")?;
    destination
        .execute_batch("VACUUM; PRAGMA journal_mode = DELETE;")
        .context("failed to compact the public catalog database")?;
    validate_public_database(&destination, mineral_count)?;
    drop(destination);

    OpenOptions::new()
        .read(true)
        .write(true)
        .open(database_temp.path())
        .and_then(|file| file.sync_all())
        .context("failed to synchronize the public catalog database")?;
    let (database_sha256, database_bytes) = hash_file(database_temp.path())?;
    let database_filename = format!("catalog-{database_sha256}.sqlite3");
    let database_path = database_output.join(&database_filename);
    let mut brotli_temp = compress_brotli(database_temp.path(), &database_output)?;
    let mut gzip_temp = compress_gzip(database_temp.path(), &database_output)?;
    let brotli_path = database_output.join(format!("{database_filename}.br"));
    let gzip_path = database_output.join(format!("{database_filename}.gz"));
    publish_content_addressed(
        database_temp.path(),
        &database_path,
        &database_sha256,
        database_bytes,
    )?;
    publish_precompressed(
        brotli_temp.path(),
        &brotli_path,
        CompressionEncoding::Brotli,
        &database_sha256,
        database_bytes,
    )?;
    publish_precompressed(
        gzip_temp.path(),
        &gzip_path,
        CompressionEncoding::Gzip,
        &database_sha256,
        database_bytes,
    )?;
    database_temp.remove()?;
    brotli_temp.remove()?;
    gzip_temp.remove()?;

    let manifest = PublicCatalogManifest {
        format: PUBLIC_CATALOG_FORMAT.to_string(),
        schema_version: PUBLIC_CATALOG_SCHEMA_VERSION,
        generated_at,
        release_id,
        mineral_count,
        database: PublicCatalogDatabase {
            path: format!("data/{database_filename}"),
            sha256: format!("sha256:{database_sha256}"),
            bytes: database_bytes,
        },
    };
    publish_manifest(&output, &manifest)?;
    Ok(manifest)
}

/// Validates a previously exported public catalog without trusting its
/// manifest, file names, compressed representations, or SQLite schema.
///
/// This is intended for release publication gates. It opens the SQLite file
/// read-only, rejects symlinks and unexpected data artifacts, and returns the
/// validated manifest only after every public-catalog invariant has passed.
pub fn validate_public_catalog_release(output: &Path) -> Result<PublicCatalogManifest> {
    let output = require_real_directory(output, "public catalog release")?;
    let manifest_path = output.join(PUBLIC_CATALOG_MANIFEST_FILE);
    require_regular_non_symlink_file(&manifest_path, "public catalog manifest")?;
    let manifest_bytes = fs::metadata(&manifest_path)
        .with_context(|| {
            format!(
                "failed to inspect public catalog manifest {}",
                manifest_path.display()
            )
        })?
        .len();
    if manifest_bytes == 0 || manifest_bytes > PUBLIC_CATALOG_MAX_MANIFEST_BYTES {
        bail!(
            "public catalog manifest size is {manifest_bytes}, expected 1..={PUBLIC_CATALOG_MAX_MANIFEST_BYTES} bytes"
        );
    }
    let manifest: PublicCatalogManifest = serde_json::from_reader(io::BufReader::new(
        File::open(&manifest_path).with_context(|| {
            format!(
                "failed to open public catalog manifest {}",
                manifest_path.display()
            )
        })?,
    ))
    .context("failed to parse public catalog manifest")?;
    validate_manifest_contract(&manifest)?;

    let digest = manifest
        .database
        .sha256
        .strip_prefix("sha256:")
        .context("public catalog database digest must start with 'sha256:'")?;
    let database_relative = PathBuf::from(format!("data/catalog-{digest}.sqlite3"));
    if manifest.database.path != database_relative.to_string_lossy() {
        bail!("public catalog manifest database path does not match its SHA-256 digest");
    }
    let data_directory = output.join("data");
    let data_directory = require_real_directory(&data_directory, "public catalog data directory")?;
    let database_path = data_directory.join(format!("catalog-{digest}.sqlite3"));
    let brotli_path = data_directory.join(format!("catalog-{digest}.sqlite3.br"));
    let gzip_path = data_directory.join(format!("catalog-{digest}.sqlite3.gz"));
    validate_exact_data_artifacts(&data_directory, [&database_path, &brotli_path, &gzip_path])?;

    let (actual_digest, actual_bytes) = hash_file(&database_path)?;
    if actual_digest != digest || actual_bytes != manifest.database.bytes {
        bail!(
            "public catalog database does not match the manifest digest and size: found sha256:{actual_digest} and {actual_bytes} bytes"
        );
    }
    verify_precompressed(
        &brotli_path,
        CompressionEncoding::Brotli,
        digest,
        actual_bytes,
    )?;
    verify_precompressed(&gzip_path, CompressionEncoding::Gzip, digest, actual_bytes)?;

    let database = Connection::open_with_flags(
        &database_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| {
        format!(
            "failed to open public catalog database {} read-only",
            database_path.display()
        )
    })?;
    database
        .busy_timeout(Duration::from_secs(5))
        .context("failed to configure public catalog validation timeout")?;
    database
        .execute_batch(
            "PRAGMA query_only = ON; PRAGMA trusted_schema = OFF; PRAGMA foreign_keys = ON;",
        )
        .context("failed to configure public catalog validation connection")?;
    validate_public_schema(&database)?;
    validate_public_database_invariants(&database, manifest.mineral_count)?;
    validate_public_metadata(&database, &manifest)?;
    validate_public_query_invariants(&database)?;
    drop(database);

    let validation_copy =
        TemporaryValidationDatabase::create(&database_path, digest, manifest.database.bytes)?;
    let copied_database = Connection::open_with_flags(
        validation_copy.path(),
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .context("failed to open temporary public catalog integrity-check copy")?;
    copied_database
        .execute_batch("PRAGMA trusted_schema = OFF; PRAGMA foreign_keys = ON;")
        .context("failed to configure temporary public catalog integrity-check copy")?;
    validate_public_database_integrity(&copied_database)?;
    drop(copied_database);

    let (final_digest, final_bytes) = hash_file(&database_path)?;
    if final_digest != digest || final_bytes != manifest.database.bytes {
        bail!("public catalog database changed during release validation");
    }
    validate_exact_data_artifacts(&data_directory, [&database_path, &brotli_path, &gzip_path])?;
    Ok(manifest)
}

fn validate_manifest_contract(manifest: &PublicCatalogManifest) -> Result<()> {
    if manifest.format != PUBLIC_CATALOG_FORMAT {
        bail!(
            "public catalog manifest format is '{}', expected '{}'",
            manifest.format,
            PUBLIC_CATALOG_FORMAT
        );
    }
    if manifest.schema_version != PUBLIC_CATALOG_SCHEMA_VERSION {
        bail!(
            "public catalog manifest schema version is {}, expected {}",
            manifest.schema_version,
            PUBLIC_CATALOG_SCHEMA_VERSION
        );
    }
    let generated_at = chrono::DateTime::parse_from_rfc3339(&manifest.generated_at)
        .context("public catalog manifest generated_at is not RFC 3339")?;
    if generated_at.offset().local_minus_utc() != 0
        || generated_at.to_rfc3339_opts(SecondsFormat::Millis, true) != manifest.generated_at
    {
        bail!("public catalog manifest generated_at is not canonical UTC millisecond RFC 3339");
    }
    require_sha256_identifier(&manifest.release_id, "public catalog release_id")?;
    require_sha256_identifier(&manifest.database.sha256, "public catalog database sha256")?;
    if manifest.mineral_count > i64::MAX as u64 {
        bail!("public catalog mineral_count exceeds SQLite's signed integer range");
    }
    if manifest.database.bytes == 0 || manifest.database.bytes > PUBLIC_CATALOG_MAX_DATABASE_BYTES {
        bail!(
            "public catalog database size is {}, expected 1..={PUBLIC_CATALOG_MAX_DATABASE_BYTES} bytes",
            manifest.database.bytes
        );
    }
    Ok(())
}

fn require_sha256_identifier<'a>(value: &'a str, label: &str) -> Result<&'a str> {
    let digest = value
        .strip_prefix("sha256:")
        .with_context(|| format!("{label} must start with 'sha256:'"))?;
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("{label} must contain exactly 64 lowercase hexadecimal characters");
    }
    Ok(digest)
}

fn validate_exact_data_artifacts<const N: usize>(
    data_directory: &Path,
    expected: [&Path; N],
) -> Result<()> {
    let expected = expected
        .into_iter()
        .map(Path::to_path_buf)
        .collect::<std::collections::BTreeSet<_>>();
    let mut actual = std::collections::BTreeSet::new();
    for entry in fs::read_dir(data_directory).with_context(|| {
        format!(
            "failed to inspect public catalog data directory {}",
            data_directory.display()
        )
    })? {
        let entry = entry?;
        let path = entry.path();
        require_regular_non_symlink_file(&path, "public catalog data artifact")?;
        actual.insert(path);
    }
    if actual != expected {
        bail!(
            "public catalog data directory must contain exactly the manifest database and its .br and .gz representations"
        );
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PublicColumn {
    name: &'static str,
    data_type: &'static str,
    not_null: i64,
    primary_key: i64,
}

const fn column(
    name: &'static str,
    data_type: &'static str,
    not_null: bool,
    primary_key: i64,
) -> PublicColumn {
    PublicColumn {
        name,
        data_type,
        not_null: not_null as i64,
        primary_key,
    }
}

const CATALOG_META_COLUMNS: &[PublicColumn] = &[
    column("key", "TEXT", true, 1),
    column("value", "TEXT", true, 0),
];

const MINERALS_COLUMNS: &[PublicColumn] = &[
    column("slug", "TEXT", true, 1),
    column("public_id", "TEXT", true, 0),
    column("canonical_name", "TEXT", true, 0),
    column("formula", "TEXT", true, 0),
    column("description", "TEXT", true, 0),
    column("mineral_family", "TEXT", true, 0),
    column("nomenclature_status", "TEXT", true, 0),
    column("verification_status", "TEXT", true, 0),
    column("data_quality_score", "REAL", true, 0),
    column("source_kind", "TEXT", true, 0),
    column("license_spdx", "TEXT", true, 0),
    column("cas_number", "TEXT", false, 0),
    column("identifiers_json", "TEXT", true, 0),
    column("properties_json", "TEXT", true, 0),
    column("safety_json", "TEXT", true, 0),
    column("discovery_country", "TEXT", true, 0),
    column("first_reference", "TEXT", true, 0),
    column("second_reference", "TEXT", true, 0),
    column("source_status", "TEXT", true, 0),
    column("evidence_count", "INTEGER", true, 0),
    column("active_offer_count", "INTEGER", true, 0),
];

const EVIDENCE_COLUMNS: &[PublicColumn] = &[
    column("mineral_slug", "TEXT", true, 1),
    column("position", "INTEGER", true, 2),
    column("title", "TEXT", true, 0),
    column("publisher", "TEXT", true, 0),
    column("canonical_url", "TEXT", true, 0),
    column("license_spdx", "TEXT", true, 0),
    column("claim_scope", "TEXT", true, 0),
    column("claim_json", "TEXT", true, 0),
    column("confidence", "REAL", true, 0),
    column("review_status", "TEXT", true, 0),
    column("retrieved_at", "TEXT", true, 0),
    column("content_hash", "TEXT", true, 0),
    column("attribution_party", "TEXT", false, 0),
    column("work_title", "TEXT", false, 0),
    column("work_url", "TEXT", false, 0),
    column("license_url", "TEXT", false, 0),
    column("changes_notice", "TEXT", false, 0),
    column("no_endorsement_notice", "TEXT", false, 0),
    column("derived_output_license_spdx", "TEXT", false, 0),
];

const OFFERS_COLUMNS: &[PublicColumn] = &[
    column("mineral_slug", "TEXT", true, 1),
    column("position", "INTEGER", true, 2),
    column("provider_name", "TEXT", true, 0),
    column("provider_slug", "TEXT", true, 0),
    column("provider_verification_status", "TEXT", true, 0),
    column("provider_trust_score", "REAL", true, 0),
    column("title", "TEXT", true, 0),
    column("product_url", "TEXT", true, 0),
    column("currency_code", "TEXT", true, 0),
    column("price_minor", "INTEGER", false, 0),
    column("currency_exponent", "INTEGER", true, 0),
    column("pricing_basis", "TEXT", true, 0),
    column("minimum_order_quantity", "REAL", false, 0),
    column("minimum_order_unit", "TEXT", true, 0),
    column("stock_status", "TEXT", true, 0),
    column("purity_text", "TEXT", true, 0),
    column("grade", "TEXT", true, 0),
    column("origin_country_code", "TEXT", true, 0),
    column("verification_status", "TEXT", true, 0),
    column("last_checked_at", "TEXT", true, 0),
    column("expires_at", "TEXT", false, 0),
];

const MINERAL_SEARCH_COLUMNS: &[PublicColumn] = &[
    column("slug", "", false, 0),
    column("canonical_name", "", false, 0),
    column("formula", "", false, 0),
    column("mineral_family", "", false, 0),
    column("search_text", "", false, 0),
];

fn validate_public_schema(database: &Connection) -> Result<()> {
    let user_version: i64 = database
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .context("failed to inspect public catalog user_version")?;
    if user_version != i64::from(PUBLIC_CATALOG_SCHEMA_VERSION) {
        bail!(
            "public catalog user_version is {user_version}, expected {PUBLIC_CATALOG_SCHEMA_VERSION}"
        );
    }
    for (table, columns) in [
        ("catalog_meta", CATALOG_META_COLUMNS),
        ("minerals", MINERALS_COLUMNS),
        ("evidence", EVIDENCE_COLUMNS),
        ("offers", OFFERS_COLUMNS),
        ("mineral_search", MINERAL_SEARCH_COLUMNS),
    ] {
        validate_public_columns(database, table, columns)?;
    }

    let unexpected_executable_schema: i64 = database.query_row(
        "SELECT COUNT(*) FROM sqlite_schema WHERE type IN ('trigger', 'view')",
        [],
        |row| row.get(0),
    )?;
    if unexpected_executable_schema != 0 {
        bail!("public catalog cannot contain triggers or views");
    }
    let explicit_indexes: i64 = database.query_row(
        "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'index' AND sql IS NOT NULL",
        [],
        |row| row.get(0),
    )?;
    if explicit_indexes != 0 {
        bail!("public catalog contains unexpected explicit indexes");
    }

    for table in ["catalog_meta", "minerals", "evidence", "offers"] {
        let definition: String = database.query_row(
            "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = ?1",
            [table],
            |row| row.get(0),
        )?;
        if !definition
            .trim_end_matches(|character: char| character.is_ascii_whitespace() || character == ';')
            .to_ascii_uppercase()
            .ends_with("WITHOUT ROWID")
        {
            bail!("public catalog table '{table}' must use WITHOUT ROWID");
        }
    }
    validate_search_schema(database)?;
    validate_public_indexes(database)?;
    validate_public_foreign_key(database, "evidence")?;
    validate_public_foreign_key(database, "offers")?;
    Ok(())
}

fn validate_public_columns(
    database: &Connection,
    table: &str,
    expected: &[PublicColumn],
) -> Result<()> {
    let mut statement = database
        .prepare("SELECT name, type, \"notnull\", pk FROM pragma_table_info(?1) ORDER BY cid")?;
    let actual = statement
        .query_map([table], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let expected = expected
        .iter()
        .map(|column| {
            (
                column.name.to_string(),
                column.data_type.to_string(),
                column.not_null,
                column.primary_key,
            )
        })
        .collect::<Vec<_>>();
    if actual != expected {
        bail!("public catalog table '{table}' does not match the v1 column contract");
    }
    let xinfo_count: i64 = database.query_row(
        "SELECT COUNT(*) FROM pragma_table_xinfo(?1)",
        [table],
        |row| row.get(0),
    )?;
    let expected_xinfo_count = if table == "mineral_search" {
        expected.len() + 2
    } else {
        expected.len()
    };
    if xinfo_count != expected_xinfo_count as i64 {
        bail!("public catalog table '{table}' contains unexpected hidden or generated columns");
    }
    let default_count: i64 = database.query_row(
        "SELECT COUNT(*) FROM pragma_table_xinfo(?1) WHERE dflt_value IS NOT NULL",
        [table],
        |row| row.get(0),
    )?;
    if default_count != 0 {
        bail!("public catalog table '{table}' contains unexpected default values");
    }
    Ok(())
}

fn validate_search_schema(database: &Connection) -> Result<()> {
    let definition: String = database.query_row(
        "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = 'mineral_search'",
        [],
        |row| row.get(0),
    )?;
    let compact = definition
        .chars()
        .filter(|character| !character.is_ascii_whitespace())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    let expected = "createvirtualtablemineral_searchusingfts5(slugunindexed,canonical_name,formula,mineral_family,search_text,tokenize='unicode61remove_diacritics2')";
    if compact != expected {
        bail!("mineral_search does not use the public catalog v1 FTS5 tokenizer contract");
    }
    let hidden = database
        .prepare(
            "SELECT name, hidden FROM pragma_table_xinfo('mineral_search') WHERE hidden <> 0 ORDER BY cid",
        )?
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if hidden != [("mineral_search".to_string(), 1), ("rank".to_string(), 1)] {
        bail!("mineral_search contains unexpected hidden columns");
    }
    Ok(())
}

fn validate_public_indexes(database: &Connection) -> Result<()> {
    for (table, expected) in [
        (
            "catalog_meta",
            vec![(1_i64, "pk".to_string(), 0_i64, vec!["key".to_string()])],
        ),
        (
            "minerals",
            vec![
                (1, "pk".to_string(), 0, vec!["slug".to_string()]),
                (1, "u".to_string(), 0, vec!["public_id".to_string()]),
            ],
        ),
        (
            "evidence",
            vec![(
                1,
                "pk".to_string(),
                0,
                vec!["mineral_slug".to_string(), "position".to_string()],
            )],
        ),
        (
            "offers",
            vec![(
                1,
                "pk".to_string(),
                0,
                vec!["mineral_slug".to_string(), "position".to_string()],
            )],
        ),
    ] {
        let indexes = database
            .prepare(
                "SELECT name, \"unique\", origin, partial FROM pragma_index_list(?1) ORDER BY name COLLATE BINARY",
            )?
            .query_map([table], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut actual = Vec::with_capacity(indexes.len());
        for (name, unique, origin, partial) in indexes {
            let columns = database
                .prepare("SELECT name FROM pragma_index_info(?1) ORDER BY seqno")?
                .query_map([name], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            actual.push((unique, origin, partial, columns));
        }
        actual.sort();
        let mut expected = expected;
        expected.sort();
        if actual != expected {
            bail!("public catalog table '{table}' has an unexpected index contract");
        }
    }
    Ok(())
}

fn validate_public_foreign_key(database: &Connection, table: &str) -> Result<()> {
    let keys = database
        .prepare(
            "SELECT \"table\", \"from\", \"to\", on_update, on_delete, match FROM pragma_foreign_key_list(?1)",
        )?
        .query_map([table], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if keys
        != [(
            "minerals".to_string(),
            "mineral_slug".to_string(),
            "slug".to_string(),
            "NO ACTION".to_string(),
            "CASCADE".to_string(),
            "NONE".to_string(),
        )]
    {
        bail!("public catalog table '{table}' has an unexpected foreign-key contract");
    }
    Ok(())
}

fn validate_public_metadata(database: &Connection, manifest: &PublicCatalogManifest) -> Result<()> {
    let actual = database
        .prepare("SELECT key, value FROM catalog_meta ORDER BY key COLLATE BINARY")?
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<std::collections::BTreeMap<_, _>>>()?;
    let expected = std::collections::BTreeMap::from([
        ("format".to_string(), manifest.format.clone()),
        ("generated_at".to_string(), manifest.generated_at.clone()),
        (
            "mineral_count".to_string(),
            manifest.mineral_count.to_string(),
        ),
        ("release_id".to_string(), manifest.release_id.clone()),
        (
            "schema_version".to_string(),
            manifest.schema_version.to_string(),
        ),
    ]);
    if actual != expected {
        bail!("public catalog metadata does not exactly match the manifest");
    }
    Ok(())
}

fn validate_public_query_invariants(database: &Connection) -> Result<()> {
    for (label, query) in [
        (
            "invalid mineral JSON",
            "SELECT COUNT(*) FROM minerals WHERE json_valid(identifiers_json) <> 1 OR json_valid(properties_json) <> 1 OR json_valid(safety_json) <> 1",
        ),
        (
            "invalid evidence JSON",
            "SELECT COUNT(*) FROM evidence WHERE json_valid(claim_json) <> 1",
        ),
        (
            "minerals missing from search",
            "SELECT COUNT(*) FROM minerals m LEFT JOIN mineral_search s ON s.slug = m.slug WHERE s.slug IS NULL",
        ),
        (
            "search rows missing minerals",
            "SELECT COUNT(*) FROM mineral_search s LEFT JOIN minerals m ON m.slug = s.slug WHERE m.slug IS NULL",
        ),
        (
            "orphaned evidence",
            "SELECT COUNT(*) FROM evidence e LEFT JOIN minerals m ON m.slug = e.mineral_slug WHERE m.slug IS NULL",
        ),
        (
            "orphaned offers",
            "SELECT COUNT(*) FROM offers o LEFT JOIN minerals m ON m.slug = o.mineral_slug WHERE m.slug IS NULL",
        ),
    ] {
        let violations: i64 = database
            .query_row(query, [], |row| row.get(0))
            .with_context(|| format!("failed to validate public catalog invariant: {label}"))?;
        if violations != 0 {
            bail!("public catalog contains {violations} instances of {label}");
        }
    }
    let mut search_probe = database
        .prepare("SELECT slug FROM mineral_search WHERE mineral_search MATCH ?1 LIMIT 1")?;
    let mut rows = search_probe.query(["waajacuvalidationtokenunlikelytoexist"])?;
    let _ = rows
        .next()
        .context("failed to execute a public catalog FTS5 query probe")?;
    Ok(())
}

fn validate_source_schema(source: &Transaction<'_>) -> Result<()> {
    // These queries deliberately mention every live table and column used by
    // the projection. Preparing them before creating output fails closed on an
    // old or incomplete live schema without ever migrating it.
    source
        .prepare(
            r#"
            SELECT
                m.id, m.slug, m.public_id, m.canonical_name, m.formula,
                m.description, m.mineral_family, m.nomenclature_status,
                m.verification_status, m.data_quality_score, m.source_kind,
                m.license_spdx, m.cas_number, m.identifiers_json,
                m.properties_json, m.safety_json, m.search_text,
                m.publication_status, m.record_type, m.is_valid_species
            FROM materials m LIMIT 0
            "#,
        )
        .context("live registry materials schema is incompatible with public export")?;
    source
        .prepare(
            r#"
            SELECT
                me.material_id, me.source_id, me.claim_scope, me.claim_json,
                me.confidence, me.review_status, me.source_title,
                me.source_publisher, me.source_license_spdx,
                me.source_retrieved_at, me.source_content_hash,
                me.source_attribution_party, me.source_work_title,
                me.source_work_url, me.source_license_url,
                me.source_changes_notice, me.source_no_endorsement_notice,
                me.source_derived_output_license_spdx,
                es.canonical_url, es.title, es.publisher, es.license_spdx,
                es.retrieved_at, es.content_hash
            FROM material_evidence me
            JOIN evidence_sources es ON es.id = me.source_id
            LIMIT 0
            "#,
        )
        .context("live registry evidence schema is incompatible with public export")?;
    source
        .prepare(
            r#"
            SELECT
                o.material_id, o.provider_id, o.title, o.product_url,
                o.currency_code, o.price_minor, o.currency_exponent,
                o.pricing_basis, o.minimum_order_quantity,
                o.minimum_order_unit, o.stock_status, o.purity_text, o.grade,
                o.origin_country_code, o.verification_status,
                o.last_checked_at, o.expires_at, o.active,
                p.name, p.slug, p.verification_status, p.trust_score, p.active
            FROM offers o JOIN providers p ON p.id = o.provider_id
            LIMIT 0
            "#,
        )
        .context("live registry offer schema is incompatible with public export")?;
    source
        .prepare(
            r#"
            SELECT f.material_id, f.fact_key, f.fact_value
            FROM mineral_dataset_facts f
            JOIN mineral_ingestion_authorities a
              ON a.dataset_key = f.dataset_key
             AND a.policy = 'ima_identity_v1'
            LIMIT 0
            "#,
        )
        .context("live registry official-fact schema is incompatible with public export")?;
    let unsupported_official_fact_count: i64 = source
        .query_row(
            r#"
            SELECT COUNT(*)
            FROM mineral_dataset_facts f
            JOIN mineral_ingestion_authorities a
              ON a.dataset_key = f.dataset_key
             AND a.policy = 'ima_identity_v1'
            JOIN materials m ON m.id = f.material_id
            WHERE m.publication_status = 'published'
              AND m.record_type = 'mineral'
              AND m.is_valid_species = 1
              AND f.fact_key NOT IN (
                  'discovery_country', 'first_reference',
                  'second_reference', 'source_status'
              )
            "#,
            [],
            |row| row.get(0),
        )
        .context("failed to validate official public mineral facts")?;
    if unsupported_official_fact_count != 0 {
        bail!(
            "live registry contains {unsupported_official_fact_count} unsupported official facts for public minerals"
        );
    }
    Ok(())
}

fn create_public_schema(destination: &Transaction<'_>) -> Result<()> {
    destination
        .execute_batch(
            r#"
            CREATE TABLE catalog_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            ) WITHOUT ROWID;

            CREATE TABLE minerals (
                slug TEXT PRIMARY KEY,
                public_id TEXT NOT NULL UNIQUE,
                canonical_name TEXT NOT NULL,
                formula TEXT NOT NULL,
                description TEXT NOT NULL,
                mineral_family TEXT NOT NULL,
                nomenclature_status TEXT NOT NULL,
                verification_status TEXT NOT NULL,
                data_quality_score REAL NOT NULL
                    CHECK(data_quality_score >= 0.0 AND data_quality_score <= 1.0),
                source_kind TEXT NOT NULL,
                license_spdx TEXT NOT NULL,
                cas_number TEXT,
                identifiers_json TEXT NOT NULL CHECK(json_valid(identifiers_json)),
                properties_json TEXT NOT NULL CHECK(json_valid(properties_json)),
                safety_json TEXT NOT NULL CHECK(json_valid(safety_json)),
                discovery_country TEXT NOT NULL,
                first_reference TEXT NOT NULL,
                second_reference TEXT NOT NULL,
                source_status TEXT NOT NULL,
                evidence_count INTEGER NOT NULL CHECK(evidence_count >= 0),
                active_offer_count INTEGER NOT NULL CHECK(active_offer_count >= 0)
            ) WITHOUT ROWID;

            CREATE TABLE evidence (
                mineral_slug TEXT NOT NULL,
                position INTEGER NOT NULL CHECK(position >= 0),
                title TEXT NOT NULL,
                publisher TEXT NOT NULL,
                canonical_url TEXT NOT NULL,
                license_spdx TEXT NOT NULL,
                claim_scope TEXT NOT NULL,
                claim_json TEXT NOT NULL CHECK(json_valid(claim_json)),
                confidence REAL NOT NULL CHECK(confidence >= 0.0 AND confidence <= 1.0),
                review_status TEXT NOT NULL,
                retrieved_at TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                attribution_party TEXT,
                work_title TEXT,
                work_url TEXT,
                license_url TEXT,
                changes_notice TEXT,
                no_endorsement_notice TEXT,
                derived_output_license_spdx TEXT,
                PRIMARY KEY(mineral_slug, position),
                FOREIGN KEY(mineral_slug) REFERENCES minerals(slug) ON DELETE CASCADE
            ) WITHOUT ROWID;

            CREATE TABLE offers (
                mineral_slug TEXT NOT NULL,
                position INTEGER NOT NULL CHECK(position >= 0),
                provider_name TEXT NOT NULL,
                provider_slug TEXT NOT NULL,
                provider_verification_status TEXT NOT NULL,
                provider_trust_score REAL NOT NULL
                    CHECK(provider_trust_score >= 0.0 AND provider_trust_score <= 1.0),
                title TEXT NOT NULL,
                product_url TEXT NOT NULL,
                currency_code TEXT NOT NULL,
                price_minor INTEGER CHECK(price_minor IS NULL OR price_minor >= 0),
                currency_exponent INTEGER NOT NULL CHECK(currency_exponent BETWEEN 0 AND 6),
                pricing_basis TEXT NOT NULL,
                minimum_order_quantity REAL
                    CHECK(minimum_order_quantity IS NULL OR minimum_order_quantity > 0),
                minimum_order_unit TEXT NOT NULL,
                stock_status TEXT NOT NULL,
                purity_text TEXT NOT NULL,
                grade TEXT NOT NULL,
                origin_country_code TEXT NOT NULL,
                verification_status TEXT NOT NULL,
                last_checked_at TEXT NOT NULL,
                expires_at TEXT,
                PRIMARY KEY(mineral_slug, position),
                FOREIGN KEY(mineral_slug) REFERENCES minerals(slug) ON DELETE CASCADE
            ) WITHOUT ROWID;

            CREATE VIRTUAL TABLE mineral_search USING fts5(
                slug UNINDEXED,
                canonical_name,
                formula,
                mineral_family,
                search_text,
                tokenize='unicode61 remove_diacritics 2'
            );
            "#,
        )
        .context("failed to create the sanitized public catalog schema")?;
    Ok(())
}

fn copy_public_snapshot(
    source: &Transaction<'_>,
    destination: &Transaction<'_>,
    generated_at: &str,
    release_id: &str,
    offer_cutoff: &str,
) -> Result<u64> {
    let mineral_count = copy_minerals(source, destination, offer_cutoff)?;
    copy_evidence(source, destination)?;
    copy_offers(source, destination, offer_cutoff)?;

    let metadata = [
        ("format", PUBLIC_CATALOG_FORMAT.to_string()),
        ("schema_version", PUBLIC_CATALOG_SCHEMA_VERSION.to_string()),
        ("generated_at", generated_at.to_string()),
        ("mineral_count", mineral_count.to_string()),
        ("release_id", release_id.to_string()),
    ];
    let mut insert = destination
        .prepare("INSERT INTO catalog_meta(key, value) VALUES (?1, ?2)")
        .context("failed to prepare public catalog metadata insertion")?;
    for (key, value) in metadata {
        insert
            .execute(params![key, value])
            .with_context(|| format!("failed to insert public catalog metadata key '{key}'"))?;
    }
    Ok(mineral_count)
}

fn copy_minerals(
    source: &Transaction<'_>,
    destination: &Transaction<'_>,
    offer_cutoff: &str,
) -> Result<u64> {
    let mut select = source
        .prepare(
            r#"
            SELECT
                m.slug, m.public_id, m.canonical_name, m.formula,
                m.description, m.mineral_family, m.nomenclature_status,
                m.verification_status, m.data_quality_score, m.source_kind,
                m.license_spdx, m.cas_number, m.identifiers_json,
                m.properties_json, m.safety_json,
                COALESCE((
                    SELECT f.fact_value
                    FROM mineral_dataset_facts f
                    JOIN mineral_ingestion_authorities a
                      ON a.dataset_key = f.dataset_key
                     AND a.policy = 'ima_identity_v1'
                    WHERE f.material_id = m.id AND f.fact_key = 'discovery_country'
                    LIMIT 1
                ), ''),
                COALESCE((
                    SELECT f.fact_value
                    FROM mineral_dataset_facts f
                    JOIN mineral_ingestion_authorities a
                      ON a.dataset_key = f.dataset_key
                     AND a.policy = 'ima_identity_v1'
                    WHERE f.material_id = m.id AND f.fact_key = 'first_reference'
                    LIMIT 1
                ), ''),
                COALESCE((
                    SELECT f.fact_value
                    FROM mineral_dataset_facts f
                    JOIN mineral_ingestion_authorities a
                      ON a.dataset_key = f.dataset_key
                     AND a.policy = 'ima_identity_v1'
                    WHERE f.material_id = m.id AND f.fact_key = 'second_reference'
                    LIMIT 1
                ), ''),
                COALESCE((
                    SELECT f.fact_value
                    FROM mineral_dataset_facts f
                    JOIN mineral_ingestion_authorities a
                      ON a.dataset_key = f.dataset_key
                     AND a.policy = 'ima_identity_v1'
                    WHERE f.material_id = m.id AND f.fact_key = 'source_status'
                    LIMIT 1
                ), ''),
                (SELECT COUNT(*) FROM material_evidence me WHERE me.material_id = m.id),
                (
                    SELECT COUNT(*)
                    FROM offers o
                    JOIN providers p ON p.id = o.provider_id
                    WHERE o.material_id = m.id
                      AND o.active = 1
                      AND p.active = 1
                      AND p.verification_status <> 'suspended'
                      AND (o.expires_at IS NULL OR datetime(o.expires_at) > datetime(?1))
                ),
                m.search_text
            FROM materials m
            WHERE m.publication_status = 'published'
              AND m.record_type = 'mineral'
              AND m.is_valid_species = 1
            ORDER BY m.slug COLLATE BINARY
            "#,
        )
        .context("failed to prepare the public mineral projection")?;
    let mut rows = select
        .query(params![offer_cutoff])
        .context("failed to query public minerals")?;
    let mut insert_mineral = destination
        .prepare(
            r#"
            INSERT INTO minerals(
                slug, public_id, canonical_name, formula, description,
                mineral_family, nomenclature_status, verification_status,
                data_quality_score, source_kind, license_spdx, cas_number,
                identifiers_json, properties_json, safety_json,
                discovery_country, first_reference, second_reference,
                source_status, evidence_count, active_offer_count
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21
            )
            "#,
        )
        .context("failed to prepare public mineral insertion")?;
    let mut insert_search = destination
        .prepare(
            r#"
            INSERT INTO mineral_search(
                slug, canonical_name, formula, mineral_family, search_text
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
        )
        .context("failed to prepare public mineral search insertion")?;

    let mut count = 0_u64;
    while let Some(row) = rows.next().context("failed to read a public mineral")? {
        let slug = row.get::<_, String>(0)?;
        let canonical_name = row.get::<_, String>(2)?;
        let formula = row.get::<_, String>(3)?;
        let mineral_family = row.get::<_, String>(5)?;
        insert_mineral
            .execute(params![
                slug,
                row.get::<_, String>(1)?,
                canonical_name,
                formula,
                row.get::<_, String>(4)?,
                mineral_family,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, f64>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, String>(12)?,
                row.get::<_, String>(13)?,
                row.get::<_, String>(14)?,
                row.get::<_, String>(15)?,
                row.get::<_, String>(16)?,
                row.get::<_, String>(17)?,
                row.get::<_, String>(18)?,
                row.get::<_, i64>(19)?,
                row.get::<_, i64>(20)?,
            ])
            .with_context(|| format!("failed to export public mineral '{slug}'"))?;
        insert_search
            .execute(params![
                slug,
                canonical_name,
                formula,
                mineral_family,
                row.get::<_, String>(21)?,
            ])
            .with_context(|| format!("failed to index public mineral '{slug}'"))?;
        count += 1;
    }
    Ok(count)
}

fn copy_evidence(source: &Transaction<'_>, destination: &Transaction<'_>) -> Result<()> {
    let mut select = source
        .prepare(
            r#"
            SELECT
                m.slug,
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
            JOIN materials m ON m.id = me.material_id
            WHERE m.publication_status = 'published'
              AND m.record_type = 'mineral'
              AND m.is_valid_species = 1
            ORDER BY
                m.slug COLLATE BINARY,
                CASE me.review_status
                    WHEN 'verified' THEN 0 WHEN 'reviewed' THEN 1
                    WHEN 'unreviewed' THEN 2 ELSE 3 END,
                me.confidence DESC,
                COALESCE(me.source_publisher, es.publisher) COLLATE NOCASE,
                es.canonical_url COLLATE BINARY,
                me.claim_scope COLLATE BINARY,
                me.id
            "#,
        )
        .context("failed to prepare the public evidence projection")?;
    let mut rows = select
        .query([])
        .context("failed to query public evidence")?;
    let mut insert = destination
        .prepare(
            r#"
            INSERT INTO evidence(
                mineral_slug, position, title, publisher, canonical_url,
                license_spdx, claim_scope, claim_json, confidence,
                review_status, retrieved_at, content_hash, attribution_party,
                work_title, work_url, license_url, changes_notice,
                no_endorsement_notice, derived_output_license_spdx
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19
            )
            "#,
        )
        .context("failed to prepare public evidence insertion")?;

    let mut current_slug = String::new();
    let mut position = 0_i64;
    while let Some(row) = rows.next().context("failed to read public evidence")? {
        let slug = row.get::<_, String>(0)?;
        if slug != current_slug {
            current_slug.clone_from(&slug);
            position = 0;
        }
        let attribution = (11..=17)
            .map(|index| row.get::<_, Option<String>>(index))
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let has_attribution = attribution.iter().any(Option::is_some);
        if has_attribution
            && attribution
                .iter()
                .any(|value| value.as_deref().is_none_or(|value| value.trim().is_empty()))
        {
            bail!("public evidence attribution snapshot is incomplete for mineral '{slug}'");
        }
        insert
            .execute(params![
                slug,
                position,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, f64>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                attribution[0],
                attribution[1],
                attribution[2],
                attribution[3],
                attribution[4],
                attribution[5],
                attribution[6],
            ])
            .with_context(|| format!("failed to export evidence for mineral '{slug}'"))?;
        position += 1;
    }
    Ok(())
}

fn copy_offers(
    source: &Transaction<'_>,
    destination: &Transaction<'_>,
    offer_cutoff: &str,
) -> Result<()> {
    let mut select = source
        .prepare(
            r#"
            SELECT
                m.slug, p.name, p.slug, p.verification_status, p.trust_score,
                o.title, o.product_url, o.currency_code, o.price_minor,
                o.currency_exponent, o.pricing_basis,
                o.minimum_order_quantity, o.minimum_order_unit,
                o.stock_status, o.purity_text, o.grade,
                o.origin_country_code, o.verification_status,
                o.last_checked_at, o.expires_at
            FROM offers o
            JOIN providers p ON p.id = o.provider_id
            JOIN materials m ON m.id = o.material_id
            WHERE m.publication_status = 'published'
              AND m.record_type = 'mineral'
              AND m.is_valid_species = 1
              AND o.active = 1
              AND p.active = 1
              AND p.verification_status <> 'suspended'
              AND (o.expires_at IS NULL OR datetime(o.expires_at) > datetime(?1))
            ORDER BY
                m.slug COLLATE BINARY,
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
                o.last_checked_at DESC,
                p.slug COLLATE BINARY,
                o.external_id COLLATE BINARY
            "#,
        )
        .context("failed to prepare the public offer projection")?;
    let mut rows = select
        .query(params![offer_cutoff])
        .context("failed to query public offers")?;
    let mut insert = destination
        .prepare(
            r#"
            INSERT INTO offers(
                mineral_slug, position, provider_name, provider_slug,
                provider_verification_status, provider_trust_score, title,
                product_url, currency_code, price_minor, currency_exponent,
                pricing_basis, minimum_order_quantity, minimum_order_unit,
                stock_status, purity_text, grade, origin_country_code,
                verification_status, last_checked_at, expires_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21
            )
            "#,
        )
        .context("failed to prepare public offer insertion")?;

    let mut current_slug = String::new();
    let mut position = 0_i64;
    while let Some(row) = rows.next().context("failed to read a public offer")? {
        let slug = row.get::<_, String>(0)?;
        if slug != current_slug {
            current_slug.clone_from(&slug);
            position = 0;
        }
        insert
            .execute(params![
                slug,
                position,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, f64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Option<i64>>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, Option<f64>>(11)?,
                row.get::<_, String>(12)?,
                row.get::<_, String>(13)?,
                row.get::<_, String>(14)?,
                row.get::<_, String>(15)?,
                row.get::<_, String>(16)?,
                row.get::<_, String>(17)?,
                row.get::<_, String>(18)?,
                row.get::<_, Option<String>>(19)?,
            ])
            .with_context(|| format!("failed to export offer for mineral '{slug}'"))?;
        position += 1;
    }
    Ok(())
}

fn validate_public_database(destination: &Connection, mineral_count: u64) -> Result<()> {
    validate_public_database_integrity(destination)?;
    validate_public_database_invariants(destination, mineral_count)
}

fn validate_public_database_integrity(destination: &Connection) -> Result<()> {
    // SQLite's FTS5 integrity callback enters an internal write-command path.
    // Existing-release validation therefore runs this full check only on an
    // independently re-hashed temporary copy, never on the published file.
    let integrity: String = destination
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .context("failed to run public catalog integrity check")?;
    if integrity != "ok" {
        bail!("public catalog integrity check failed: {integrity}");
    }
    destination
        .execute(
            "INSERT INTO mineral_search(mineral_search, rank) VALUES('integrity-check', 1)",
            [],
        )
        .context("public catalog FTS5 integrity check failed")?;
    Ok(())
}

fn validate_public_database_invariants(destination: &Connection, mineral_count: u64) -> Result<()> {
    let page_size: i64 = destination
        .query_row("PRAGMA page_size", [], |row| row.get(0))
        .context("failed to verify public catalog page size")?;
    if page_size != PUBLIC_CATALOG_PAGE_SIZE {
        bail!("public catalog page size is {page_size}, expected {PUBLIC_CATALOG_PAGE_SIZE}");
    }
    let foreign_key_violations: i64 = destination
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .context("failed to run public catalog foreign-key check")?;
    if foreign_key_violations != 0 {
        bail!("public catalog contains {foreign_key_violations} foreign-key violations");
    }
    let stored_count: i64 = destination
        .query_row("SELECT COUNT(*) FROM minerals", [], |row| row.get(0))
        .context("failed to verify public mineral count")?;
    if stored_count < 0 || stored_count as u64 != mineral_count {
        bail!(
            "public catalog mineral count mismatch: expected {mineral_count}, found {stored_count}"
        );
    }
    let fts_count: i64 = destination
        .query_row("SELECT COUNT(*) FROM mineral_search", [], |row| row.get(0))
        .context("failed to verify public mineral search count")?;
    if fts_count != stored_count {
        bail!("public catalog FTS count mismatch: expected {stored_count}, found {fts_count}");
    }
    let expected_evidence: i64 = destination.query_row(
        "SELECT COALESCE(SUM(evidence_count), 0) FROM minerals",
        [],
        |row| row.get(0),
    )?;
    let actual_evidence: i64 =
        destination.query_row("SELECT COUNT(*) FROM evidence", [], |row| row.get(0))?;
    if actual_evidence != expected_evidence {
        bail!(
            "public catalog evidence count mismatch: expected {expected_evidence}, found {actual_evidence}"
        );
    }
    let expected_offers: i64 = destination.query_row(
        "SELECT COALESCE(SUM(active_offer_count), 0) FROM minerals",
        [],
        |row| row.get(0),
    )?;
    let actual_offers: i64 =
        destination.query_row("SELECT COUNT(*) FROM offers", [], |row| row.get(0))?;
    if actual_offers != expected_offers {
        bail!(
            "public catalog offer count mismatch: expected {expected_offers}, found {actual_offers}"
        );
    }
    let metadata_count: i64 = destination
        .query_row("SELECT COUNT(*) FROM catalog_meta", [], |row| row.get(0))
        .context("failed to verify public catalog metadata count")?;
    if metadata_count != 5 {
        bail!("public catalog metadata count is {metadata_count}, expected 5");
    }
    let mut stmt = destination
        .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' ORDER BY name COLLATE BINARY")
        .context("failed to inspect public catalog table names")?;
    let tables = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if tables != PUBLIC_TABLES {
        bail!("public catalog contains unexpected tables: {tables:?}");
    }
    let journal_mode: String = destination
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .context("failed to verify public catalog journal mode")?;
    if !journal_mode.eq_ignore_ascii_case("delete") {
        bail!("public catalog journal mode is '{journal_mode}', expected DELETE");
    }
    Ok(())
}

fn prepare_output_directory(output: &Path) -> Result<PathBuf> {
    let absolute = if output.is_absolute() {
        output.to_path_buf()
    } else {
        std::env::current_dir()
            .context("failed to resolve the current directory")?
            .join(output)
    };
    reject_existing_symlink_components(&absolute)?;
    fs::create_dir_all(&absolute)
        .with_context(|| format!("failed to create export directory {}", absolute.display()))?;
    reject_existing_symlink_components(&absolute)?;
    let metadata = fs::symlink_metadata(&absolute)
        .with_context(|| format!("failed to inspect export directory {}", absolute.display()))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        bail!(
            "public catalog output must be a real directory, not a symlink: {}",
            absolute.display()
        );
    }
    absolute.canonicalize().with_context(|| {
        format!(
            "failed to canonicalize export directory {}",
            absolute.display()
        )
    })
}

fn require_real_directory(path: &Path, label: &str) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .context("failed to resolve the current directory")?
            .join(path)
    };
    reject_existing_symlink_components(&absolute)?;
    let metadata = fs::symlink_metadata(&absolute)
        .with_context(|| format!("failed to inspect {label} {}", absolute.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("{label} must be a real directory: {}", absolute.display());
    }
    absolute
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {label} {}", absolute.display()))
}

fn reject_existing_symlink_components(path: &Path) -> Result<()> {
    let mut ancestors = path.ancestors().collect::<Vec<_>>();
    ancestors.reverse();
    for ancestor in ancestors {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!(
                    "refusing path with symlink component: {}",
                    ancestor.display()
                )
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to inspect path component {}", ancestor.display())
                })
            }
        }
    }
    Ok(())
}

fn require_regular_non_symlink_file(path: &Path, label: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {label} {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!(
            "{label} must be a regular non-symlink file: {}",
            path.display()
        );
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum CompressionEncoding {
    Brotli,
    Gzip,
}

impl CompressionEncoding {
    fn label(self) -> &'static str {
        match self {
            Self::Brotli => "Brotli",
            Self::Gzip => "gzip",
        }
    }
}

fn compress_brotli(source: &Path, output: &Path) -> Result<TemporaryFile> {
    let temporary = TemporaryFile::create(output, "catalog-db-brotli", "br")?;
    let mut input = File::open(source)
        .with_context(|| format!("failed to open public catalog {}", source.display()))?;
    let size_hint = usize::try_from(input.metadata()?.len())
        .context("public catalog is too large for the Brotli compressor")?;
    let mut compressed = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(temporary.path())
        .context("failed to open the temporary Brotli catalog representation")?;
    let parameters = brotli::enc::BrotliEncoderParams {
        quality: BROTLI_QUALITY,
        lgwin: BROTLI_WINDOW_BITS,
        size_hint,
        ..Default::default()
    };
    brotli::BrotliCompress(&mut input, &mut compressed, &parameters)
        .context("failed to Brotli-compress the public catalog")?;
    compressed
        .sync_all()
        .context("failed to synchronize the Brotli catalog representation")?;
    Ok(temporary)
}

fn compress_gzip(source: &Path, output: &Path) -> Result<TemporaryFile> {
    let temporary = TemporaryFile::create(output, "catalog-db-gzip", "gz")?;
    let input = File::open(source)
        .with_context(|| format!("failed to open public catalog {}", source.display()))?;
    let compressed = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(temporary.path())
        .context("failed to open the temporary gzip catalog representation")?;
    let mut encoder = flate2::write::GzEncoder::new(compressed, flate2::Compression::best());
    io::copy(&mut io::BufReader::new(input), &mut encoder)
        .context("failed to gzip-compress the public catalog")?;
    let compressed = encoder
        .finish()
        .context("failed to finish the gzip catalog representation")?;
    compressed
        .sync_all()
        .context("failed to synchronize the gzip catalog representation")?;
    Ok(temporary)
}

fn publish_precompressed(
    temporary: &Path,
    destination: &Path,
    encoding: CompressionEncoding,
    expected_sha256: &str,
    expected_bytes: u64,
) -> Result<()> {
    verify_precompressed(temporary, encoding, expected_sha256, expected_bytes)?;
    match fs::hard_link(temporary, destination) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            verify_precompressed(destination, encoding, expected_sha256, expected_bytes)
        }
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to publish the {} public catalog representation {}",
                encoding.label(),
                destination.display()
            )
        }),
    }
}

fn verify_precompressed(
    path: &Path,
    encoding: CompressionEncoding,
    expected_sha256: &str,
    expected_bytes: u64,
) -> Result<()> {
    require_regular_non_symlink_file(path, "precompressed public catalog")?;
    let file = File::open(path).with_context(|| {
        format!(
            "failed to open the {} public catalog representation {}",
            encoding.label(),
            path.display()
        )
    })?;
    let maximum_decoded_bytes = expected_bytes.saturating_add(1);
    let (actual_sha256, actual_bytes) = match encoding {
        CompressionEncoding::Brotli => {
            hash_complete_brotli_stream(file, maximum_decoded_bytes, path)
        }
        CompressionEncoding::Gzip => hash_complete_gzip_stream(file, maximum_decoded_bytes, path),
    }
    .with_context(|| {
        format!(
            "failed to verify the {} public catalog representation {}",
            encoding.label(),
            path.display()
        )
    })?;
    if actual_sha256 != expected_sha256 || actual_bytes != expected_bytes {
        bail!(
            "refusing conflicting {} public catalog representation {}",
            encoding.label(),
            path.display()
        );
    }
    Ok(())
}

fn hash_complete_gzip_stream(
    file: File,
    maximum_decoded_bytes: u64,
    path: &Path,
) -> Result<(String, u64)> {
    let compressed_bytes = file
        .metadata()
        .with_context(|| format!("failed to inspect {}", path.display()))?
        .len();
    let buffered = io::BufReader::with_capacity(COMPRESSION_BUFFER_BYTES, file);
    // The single-member decoder deliberately stops after the first gzip
    // member. Inspecting the BufReader's logical position then rejects both a
    // concatenated member (even an empty one) and arbitrary trailing bytes.
    let mut decoder = flate2::bufread::GzDecoder::new(buffered);
    let decoded = hash_reader((&mut decoder).take(maximum_decoded_bytes), path)?;
    let consumed_bytes = decoder
        .get_mut()
        .stream_position()
        .with_context(|| format!("failed to inspect the end of {}", path.display()))?;
    if consumed_bytes != compressed_bytes {
        bail!(
            "gzip public catalog representation {} has trailing bytes or multiple members",
            path.display()
        );
    }
    Ok(decoded)
}

fn hash_complete_brotli_stream(
    file: File,
    maximum_decoded_bytes: u64,
    path: &Path,
) -> Result<(String, u64)> {
    let compressed_bytes = file
        .metadata()
        .with_context(|| format!("failed to inspect {}", path.display()))?
        .len();
    let buffered = io::BufReader::with_capacity(COMPRESSION_BUFFER_BYTES, file);
    // Keep the decoder's input window at one byte so it cannot accept a
    // complete stream after buffering unrelated trailing bytes. The outer
    // BufReader keeps file reads efficient and reports its logical position.
    let mut decoder = brotli::Decompressor::new(buffered, 1);
    let decoded = hash_reader((&mut decoder).take(maximum_decoded_bytes), path)?;
    let consumed_bytes = decoder
        .get_mut()
        .stream_position()
        .with_context(|| format!("failed to inspect the end of {}", path.display()))?;
    if consumed_bytes != compressed_bytes {
        bail!(
            "Brotli public catalog representation {} has trailing bytes",
            path.display()
        );
    }
    Ok(decoded)
}

fn publish_content_addressed(
    temporary: &Path,
    destination: &Path,
    expected_sha256: &str,
    expected_bytes: u64,
) -> Result<()> {
    match fs::hard_link(temporary, destination) {
        Ok(()) => {
            verify_existing_content_address(destination, expected_sha256, expected_bytes)?;
            Ok(())
        }
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            verify_existing_content_address(destination, expected_sha256, expected_bytes)
        }
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to publish immutable public catalog {}",
                destination.display()
            )
        }),
    }
}

fn verify_existing_content_address(
    path: &Path,
    expected_sha256: &str,
    expected_bytes: u64,
) -> Result<()> {
    require_regular_non_symlink_file(path, "content-addressed public catalog")?;
    let (actual_sha256, actual_bytes) = hash_file(path)?;
    if actual_sha256 != expected_sha256 || actual_bytes != expected_bytes {
        bail!(
            "refusing conflicting content-addressed public catalog {}",
            path.display()
        );
    }
    Ok(())
}

fn publish_manifest(output: &Path, manifest: &PublicCatalogManifest) -> Result<()> {
    let mut temporary = TemporaryFile::create(output, "catalog-manifest", "json")?;
    {
        let mut file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(temporary.path())
            .context("failed to open the temporary public catalog manifest")?;
        serde_json::to_writer_pretty(&mut file, manifest)
            .context("failed to serialize the public catalog manifest")?;
        file.write_all(b"\n")
            .context("failed to finish the public catalog manifest")?;
        file.sync_all()
            .context("failed to synchronize the public catalog manifest")?;
    }
    let destination = output.join(PUBLIC_CATALOG_MANIFEST_FILE);
    validate_manifest_destination(output)?;
    atomic_replace(temporary.path(), &destination).with_context(|| {
        format!(
            "failed to atomically publish public catalog manifest {}",
            destination.display()
        )
    })?;
    temporary.disarm();
    Ok(())
}

fn validate_manifest_destination(output: &Path) -> Result<()> {
    let destination = output.join(PUBLIC_CATALOG_MANIFEST_FILE);
    match fs::symlink_metadata(&destination) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => bail!(
            "existing public catalog manifest must be a regular non-symlink file: {}",
            destination.display()
        ),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to inspect public catalog manifest {}",
                destination.display()
            )
        }),
    }
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    #[link(name = "Kernel32")]
    extern "system" {
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: both paths are owned, NUL-terminated UTF-16 buffers that remain
    // alive for the duration of the call, and no nullable out-pointers exist.
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

fn hash_file(path: &Path) -> Result<(String, u64)> {
    require_regular_non_symlink_file(path, "public catalog database")?;
    let file = File::open(path)
        .with_context(|| format!("failed to open public catalog file {}", path.display()))?;
    hash_reader(file, path)
}

fn hash_reader(mut reader: impl Read, path: &Path) -> Result<(String, u64)> {
    let mut digest = DigestContext::new(&SHA256);
    let mut buffer = [0_u8; 64 * 1024];
    let mut bytes = 0_u64;
    loop {
        let read = reader
            .read(&mut buffer)
            .with_context(|| format!("failed to hash public catalog file {}", path.display()))?;
        if read == 0 {
            break;
        }
        bytes = bytes
            .checked_add(read as u64)
            .context("public catalog file length overflowed u64")?;
        digest.update(&buffer[..read]);
    }
    let sha256 = digest
        .finish()
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok((sha256, bytes))
}

fn random_hex(bytes: usize) -> Result<String> {
    let mut value = vec![0_u8; bytes];
    getrandom(&mut value).map_err(|error| {
        anyhow::anyhow!("failed to obtain randomness for public catalog publication: {error}")
    })?;
    Ok(value.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn hash_bytes(value: &[u8]) -> String {
    ring::digest::digest(&SHA256, value)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

struct TemporaryValidationDatabase {
    directory: PathBuf,
    database: PathBuf,
}

impl TemporaryValidationDatabase {
    fn create(source: &Path, expected_sha256: &str, expected_bytes: u64) -> Result<Self> {
        require_regular_non_symlink_file(source, "public catalog validation source")?;
        // System temp locations are commonly exposed through a stable symlink
        // (for example /tmp on some platforms). Resolve that alias once, then
        // create an unpredictable exclusive child in the real directory.
        let temporary_root = std::env::temp_dir()
            .canonicalize()
            .context("failed to resolve system temporary directory for catalog validation")?;
        let temporary_root_metadata = fs::symlink_metadata(&temporary_root).with_context(|| {
            format!(
                "failed to inspect system temporary directory {}",
                temporary_root.display()
            )
        })?;
        if temporary_root_metadata.file_type().is_symlink() || !temporary_root_metadata.is_dir() {
            bail!(
                "resolved system temporary location is not a real directory: {}",
                temporary_root.display()
            );
        }
        for _ in 0..32 {
            let directory =
                temporary_root.join(format!(".waajacu-catalog-validation-{}", random_hex(16)?));
            match fs::create_dir(&directory) {
                Ok(()) => {
                    let database = directory.join("catalog.sqlite3");
                    let temporary = Self {
                        directory,
                        database,
                    };
                    let mut input = File::open(source).with_context(|| {
                        format!(
                            "failed to open public catalog validation source {}",
                            source.display()
                        )
                    })?;
                    let mut output = OpenOptions::new()
                        .read(true)
                        .write(true)
                        .create_new(true)
                        .open(temporary.path())
                        .context("failed to create temporary public catalog validation copy")?;
                    let copied = io::copy(&mut input, &mut output)
                        .context("failed to copy public catalog for isolated integrity checking")?;
                    if copied != expected_bytes {
                        bail!(
                            "public catalog changed while creating its integrity-check copy: copied {copied} bytes, expected {expected_bytes}"
                        );
                    }
                    output.sync_all().context(
                        "failed to synchronize temporary public catalog validation copy",
                    )?;
                    drop(output);
                    let (actual_sha256, actual_bytes) = hash_file(temporary.path())?;
                    if actual_sha256 != expected_sha256 || actual_bytes != expected_bytes {
                        bail!(
                            "temporary public catalog integrity-check copy failed re-verification"
                        );
                    }
                    return Ok(temporary);
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!(
                            "failed to create isolated public catalog validation directory {}",
                            directory.display()
                        )
                    })
                }
            }
        }
        bail!("failed to allocate an isolated public catalog validation directory")
    }

    fn path(&self) -> &Path {
        &self.database
    }
}

impl Drop for TemporaryValidationDatabase {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

struct TemporaryFile {
    path: PathBuf,
    armed: bool,
}

impl TemporaryFile {
    fn create(directory: &Path, label: &str, extension: &str) -> Result<Self> {
        for _ in 0..32 {
            let path = directory.join(format!(".{label}-{}.tmp.{extension}", random_hex(16)?));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => {
                    file.sync_all().with_context(|| {
                        format!("failed to initialize temporary file {}", path.display())
                    })?;
                    return Ok(Self { path, armed: true });
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("failed to create temporary file in {}", directory.display())
                    })
                }
            }
        }
        bail!(
            "failed to allocate a unique temporary file in {}",
            directory.display()
        )
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn remove(&mut self) -> Result<()> {
        fs::remove_file(&self.path)
            .with_context(|| format!("failed to remove temporary file {}", self.path.display()))?;
        self.armed = false;
        Ok(())
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, fs, io::Write};

    use anyhow::Result;
    use rusqlite::{params, Connection};
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn export_is_sanitized_searchable_integral_and_rerunnable() -> Result<()> {
        let data_root = TempDir::new()?;
        let output = TempDir::new()?;
        prepare_registry(data_root.path())?;
        seed_live_registry(data_root.path())?;

        let live_path = data_root.path().join(LIVE_DATABASE_FILE);
        let live_before = file_snapshot(&live_path)?;
        let first = export_public_catalog(data_root.path(), output.path())?;
        assert_eq!(
            file_snapshot(&live_path)?,
            live_before,
            "live database was modified"
        );
        assert_manifest(output.path(), &first, 1)?;

        let first_database_path = output.path().join(&first.database.path);
        let first_brotli_path = output.path().join(format!("{}.br", first.database.path));
        let first_gzip_path = output.path().join(format!("{}.gz", first.database.path));
        let first_brotli_snapshot = file_snapshot(&first_brotli_path)?;
        let first_gzip_snapshot = file_snapshot(&first_gzip_path)?;
        let public = Connection::open_with_flags(
            &first_database_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        let tables = public
            .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' ORDER BY name")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<BTreeSet<_>>>()?;
        let expected = PUBLIC_TABLES
            .iter()
            .map(|name| (*name).to_string())
            .collect::<BTreeSet<_>>();
        assert_eq!(tables, expected);
        for private in [
            "materials",
            "providers",
            "material_evidence",
            "evidence_sources",
            "mineral_review_revisions",
            "mineral_ingestion_batches",
            "material_publication_events",
        ] {
            assert!(!tables.contains(private), "private table leaked: {private}");
        }

        let slugs = public
            .prepare("SELECT slug FROM minerals ORDER BY slug")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        assert_eq!(slugs, ["public-quartz"]);
        let mineral = public.query_row(
            r#"
            SELECT discovery_country, first_reference, source_status,
                   evidence_count, active_offer_count
            FROM minerals WHERE slug = ?1
            "#,
            params!["public-quartz"],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )?;
        assert_eq!(
            mineral,
            ("Testland".into(), "Ref 1".into(), "A".into(), 1, 2)
        );
        assert_eq!(
            public.query_row("SELECT COUNT(*) FROM evidence", [], |row| row
                .get::<_, i64>(0))?,
            1
        );
        assert_eq!(
            public.query_row("SELECT COUNT(*) FROM offers", [], |row| row
                .get::<_, i64>(0))?,
            2
        );
        let expiries = public
            .prepare("SELECT expires_at FROM offers ORDER BY position")?
            .query_map([], |row| row.get::<_, Option<String>>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        assert!(expiries.contains(&Some("2999-01-01 00:00:00".to_string())));
        assert!(expiries.contains(&None));

        let fts_slug: String = public.query_row(
            "SELECT slug FROM mineral_search WHERE mineral_search MATCH ?1",
            params!["sentinel*"],
            |row| row.get(0),
        )?;
        assert_eq!(fts_slug, "public-quartz");
        let journal_mode: String = public.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
        assert_eq!(journal_mode.to_ascii_lowercase(), "delete");
        let page_size: i64 = public.query_row("PRAGMA page_size", [], |row| row.get(0))?;
        assert_eq!(page_size, PUBLIC_CATALOG_PAGE_SIZE);
        drop(public);

        let exported_bytes = fs::read(&first_database_path)?;
        let exported_text = String::from_utf8_lossy(&exported_bytes);
        for private_sentinel in [
            "withdrawn-secret",
            "invalid-secret",
            "compound-secret",
            "pending-review-secret",
            "expired-offer-secret",
            "suspended-offer-secret",
        ] {
            assert!(
                !exported_text.contains(private_sentinel),
                "private sentinel leaked: {private_sentinel}"
            );
        }

        let second = export_public_catalog(data_root.path(), output.path())?;
        assert_eq!(
            file_snapshot(&live_path)?,
            live_before,
            "rerun modified live database"
        );
        assert_manifest(output.path(), &second, 1)?;
        assert!(first_database_path.is_file(), "old hashed DB was removed");
        assert_eq!(
            file_snapshot(&first_brotli_path)?,
            first_brotli_snapshot,
            "old Brotli representation was replaced"
        );
        assert_eq!(
            file_snapshot(&first_gzip_path)?,
            first_gzip_snapshot,
            "old gzip representation was replaced"
        );
        assert!(output.path().join(&second.database.path).is_file());
        assert_eq!(
            Connection::open_with_flags(
                &first_database_path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )?
            .query_row("SELECT COUNT(*) FROM minerals", [], |row| row
                .get::<_, i64>(0))?,
            1,
            "old hashed DB was no longer usable after rerun"
        );
        let published: PublicCatalogManifest =
            serde_json::from_slice(&fs::read(output.path().join(PUBLIC_CATALOG_MANIFEST_FILE))?)?;
        assert_eq!(published, second);
        Ok(())
    }

    #[test]
    fn existing_release_validation_is_fail_closed() -> Result<()> {
        let data_root = TempDir::new()?;
        let output = TempDir::new()?;
        prepare_registry(data_root.path())?;
        seed_live_registry(data_root.path())?;
        let manifest = export_public_catalog(data_root.path(), output.path())?;
        let database_path = output.path().join(&manifest.database.path);
        let database_before = file_snapshot(&database_path)?;
        assert_eq!(validate_public_catalog_release(output.path())?, manifest);
        assert_eq!(file_snapshot(&database_path)?, database_before);
        assert!(!database_path.with_extension("sqlite3-journal").exists());
        assert!(!database_path.with_extension("sqlite3-wal").exists());
        assert!(!database_path.with_extension("sqlite3-shm").exists());

        let manifest_path = output.path().join(PUBLIC_CATALOG_MANIFEST_FILE);
        let original_manifest = fs::read(&manifest_path)?;
        let mut value: serde_json::Value = serde_json::from_slice(&original_manifest)?;
        value["unexpected_private_field"] = serde_json::json!(true);
        fs::write(&manifest_path, serde_json::to_vec_pretty(&value)?)?;
        assert!(validate_public_catalog_release(output.path()).is_err());
        fs::write(&manifest_path, &original_manifest)?;

        let mut wrong_size = manifest.clone();
        wrong_size.database.bytes += 1;
        fs::write(&manifest_path, serde_json::to_vec_pretty(&wrong_size)?)?;
        assert!(validate_public_catalog_release(output.path()).is_err());
        fs::write(&manifest_path, &original_manifest)?;

        let unexpected = output.path().join("data/private-backup.sqlite3");
        fs::write(&unexpected, b"not public")?;
        assert!(validate_public_catalog_release(output.path()).is_err());
        fs::remove_file(unexpected)?;

        let mut mismatched = manifest.clone();
        mismatched.release_id = format!("sha256:{}", "a".repeat(64));
        fs::write(&manifest_path, serde_json::to_vec_pretty(&mismatched)?)?;
        let error = validate_public_catalog_release(output.path()).unwrap_err();
        assert!(format!("{error:#}").contains("metadata"));

        fs::write(&manifest_path, &original_manifest)?;
        let original_database = fs::read(&database_path)?;
        let mut corrupted_database = original_database.clone();
        let corrupted_position = corrupted_database.len() / 2;
        corrupted_database[corrupted_position] ^= 1;
        fs::write(&database_path, &corrupted_database)?;
        assert!(validate_public_catalog_release(output.path()).is_err());
        fs::write(&database_path, original_database)?;
        Ok(())
    }

    #[test]
    fn fts_integrity_validation_rejects_a_corrupted_inverted_index() -> Result<()> {
        let data_root = TempDir::new()?;
        let output = TempDir::new()?;
        prepare_registry(data_root.path())?;
        seed_live_registry(data_root.path())?;
        let manifest = export_public_catalog(data_root.path(), output.path())?;
        let database_path = output.path().join(manifest.database.path);
        let database = Connection::open(&database_path)?;
        let removed = database.execute("DELETE FROM mineral_search_data WHERE id > 10", [])?;
        assert!(removed > 0, "fixture must contain an FTS5 index segment");
        let error = validate_public_database_integrity(&database).unwrap_err();
        assert!(format!("{error:#}").contains("integrity"));
        Ok(())
    }

    #[test]
    fn refuses_non_file_manifest_without_destroying_it() -> Result<()> {
        let data_root = TempDir::new()?;
        let output = TempDir::new()?;
        prepare_registry(data_root.path())?;
        let manifest_path = output.path().join(PUBLIC_CATALOG_MANIFEST_FILE);
        fs::create_dir(&manifest_path)?;

        let error = export_public_catalog(data_root.path(), output.path()).unwrap_err();
        assert!(error.to_string().contains("manifest"));
        assert!(manifest_path.is_dir());
        Ok(())
    }

    #[test]
    fn refuses_conflicting_precompressed_representations_without_replacing_them() -> Result<()> {
        let output = TempDir::new()?;
        let source = output.path().join("catalog.sqlite3");
        fs::write(
            &source,
            b"public catalog compression sentinel".repeat(8_192),
        )?;
        let (sha256, bytes) = hash_file(&source)?;

        let representations = [
            (
                CompressionEncoding::Brotli,
                compress_brotli(&source, output.path())?,
                output.path().join("catalog.sqlite3.br"),
            ),
            (
                CompressionEncoding::Gzip,
                compress_gzip(&source, output.path())?,
                output.path().join("catalog.sqlite3.gz"),
            ),
        ];
        for (encoding, temporary, destination) in representations {
            fs::write(&destination, b"existing conflicting representation")?;
            let before = file_snapshot(&destination)?;
            let error =
                publish_precompressed(temporary.path(), &destination, encoding, &sha256, bytes)
                    .unwrap_err();
            assert!(error.to_string().contains(encoding.label()));
            assert_eq!(file_snapshot(&destination)?, before);
        }
        Ok(())
    }

    #[test]
    fn refuses_invalid_new_precompressed_representations_before_publishing() -> Result<()> {
        let output = TempDir::new()?;
        let source = output.path().join("catalog.sqlite3");
        fs::write(
            &source,
            b"public catalog compression sentinel".repeat(8_192),
        )?;
        let (sha256, bytes) = hash_file(&source)?;

        for (encoding, extension) in [
            (CompressionEncoding::Brotli, "br"),
            (CompressionEncoding::Gzip, "gz"),
        ] {
            let temporary = output.path().join(format!("invalid.{extension}.tmp"));
            let destination = output.path().join(format!("catalog.sqlite3.{extension}"));
            fs::write(&temporary, b"not a compressed public catalog")?;

            let error = publish_precompressed(&temporary, &destination, encoding, &sha256, bytes)
                .unwrap_err();
            assert!(error.to_string().contains(encoding.label()));
            assert!(
                !destination.exists(),
                "invalid {} representation was published",
                encoding.label()
            );
        }
        Ok(())
    }

    #[test]
    fn refuses_gzip_members_or_trailing_bytes_after_the_catalog() -> Result<()> {
        let output = TempDir::new()?;
        let source = output.path().join("catalog.sqlite3");
        fs::write(
            &source,
            b"public catalog compression sentinel".repeat(8_192),
        )?;
        let (sha256, bytes) = hash_file(&source)?;
        let compressed = compress_gzip(&source, output.path())?;

        let multiple_members = output.path().join("multiple-members.sqlite3.gz");
        fs::copy(compressed.path(), &multiple_members)?;
        let appended = fs::OpenOptions::new()
            .append(true)
            .open(&multiple_members)?;
        let mut encoder = flate2::write::GzEncoder::new(appended, flate2::Compression::best());
        encoder.write_all(b"unexpected second member")?;
        encoder.finish()?.sync_all()?;
        assert!(
            verify_precompressed(&multiple_members, CompressionEncoding::Gzip, &sha256, bytes)
                .is_err()
        );

        let empty_second_member = output.path().join("empty-second-member.sqlite3.gz");
        fs::copy(compressed.path(), &empty_second_member)?;
        let appended = fs::OpenOptions::new()
            .append(true)
            .open(&empty_second_member)?;
        flate2::write::GzEncoder::new(appended, flate2::Compression::best())
            .finish()?
            .sync_all()?;
        assert!(verify_precompressed(
            &empty_second_member,
            CompressionEncoding::Gzip,
            &sha256,
            bytes,
        )
        .is_err());

        let trailing = output.path().join("trailing-bytes.sqlite3.gz");
        fs::copy(compressed.path(), &trailing)?;
        let mut trailing_file = fs::OpenOptions::new().append(true).open(&trailing)?;
        trailing_file.write_all(b"not another gzip member")?;
        trailing_file.sync_all()?;
        assert!(
            verify_precompressed(&trailing, CompressionEncoding::Gzip, &sha256, bytes).is_err()
        );
        Ok(())
    }

    #[test]
    fn refuses_trailing_bytes_after_the_brotli_catalog() -> Result<()> {
        let output = TempDir::new()?;
        let source = output.path().join("catalog.sqlite3");
        fs::write(
            &source,
            b"public catalog compression sentinel".repeat(8_192),
        )?;
        let (sha256, bytes) = hash_file(&source)?;
        let compressed = compress_brotli(&source, output.path())?;
        let trailing = output.path().join("trailing-bytes.sqlite3.br");
        fs::copy(compressed.path(), &trailing)?;
        let mut trailing_file = fs::OpenOptions::new().append(true).open(&trailing)?;
        trailing_file.write_all(b"unexpected bytes after Brotli stream")?;
        trailing_file.sync_all()?;

        assert!(
            verify_precompressed(&trailing, CompressionEncoding::Brotli, &sha256, bytes).is_err()
        );
        Ok(())
    }

    fn assert_manifest(
        output: &Path,
        manifest: &PublicCatalogManifest,
        expected_count: u64,
    ) -> Result<()> {
        assert_eq!(manifest.format, PUBLIC_CATALOG_FORMAT);
        assert_eq!(manifest.schema_version, PUBLIC_CATALOG_SCHEMA_VERSION);
        assert_eq!(manifest.mineral_count, expected_count);
        assert!(manifest.release_id.starts_with("sha256:"));
        assert_eq!(manifest.release_id.len(), "sha256:".len() + 64);
        assert!(manifest.database.sha256.starts_with("sha256:"));
        assert_eq!(manifest.database.sha256.len(), "sha256:".len() + 64);
        assert!(manifest
            .database
            .sha256
            .strip_prefix("sha256:")
            .unwrap()
            .chars()
            .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase()));
        let bare_sha256 = manifest.database.sha256.strip_prefix("sha256:").unwrap();
        assert_eq!(
            manifest.database.path,
            format!("data/catalog-{bare_sha256}.sqlite3")
        );
        let database_path = output.join(&manifest.database.path);
        let (sha256, bytes) = hash_file(&database_path)?;
        assert_eq!(sha256, bare_sha256);
        assert_eq!(bytes, manifest.database.bytes);
        let disk_manifest: PublicCatalogManifest =
            serde_json::from_slice(&fs::read(output.join(PUBLIC_CATALOG_MANIFEST_FILE))?)?;
        assert_eq!(&disk_manifest, manifest);

        let brotli_path = output.join(format!("{}.br", manifest.database.path));
        let gzip_path = output.join(format!("{}.gz", manifest.database.path));
        verify_precompressed(
            &brotli_path,
            CompressionEncoding::Brotli,
            bare_sha256,
            manifest.database.bytes,
        )?;
        verify_precompressed(
            &gzip_path,
            CompressionEncoding::Gzip,
            bare_sha256,
            manifest.database.bytes,
        )?;
        assert!(fs::metadata(&brotli_path)?.len() < manifest.database.bytes);
        assert!(fs::metadata(&gzip_path)?.len() < manifest.database.bytes);

        let database = Connection::open(database_path)?;
        let metadata = database
            .prepare("SELECT key, value FROM catalog_meta ORDER BY key")?
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<std::collections::BTreeMap<_, _>>>()?;
        assert_eq!(metadata.len(), 5);
        assert_eq!(
            metadata.get("format").map(String::as_str),
            Some(PUBLIC_CATALOG_FORMAT)
        );
        assert_eq!(
            metadata.get("schema_version").map(String::as_str),
            Some("1")
        );
        assert_eq!(metadata.get("mineral_count").map(String::as_str), Some("1"));
        assert_eq!(metadata.get("release_id"), Some(&manifest.release_id));
        assert_eq!(metadata.get("generated_at"), Some(&manifest.generated_at));
        Ok(())
    }

    fn seed_live_registry(data_root: &Path) -> Result<()> {
        let database_path = data_root.join(LIVE_DATABASE_FILE);
        let connection = Connection::open(database_path)?;
        connection.execute_batch("PRAGMA foreign_keys = OFF;")?;
        for (id, slug, public_id, publication_status, record_type, valid, search_text) in [
            (
                1,
                "public-quartz",
                "mat_public",
                "published",
                "mineral",
                1,
                "quartz sentinel alpha",
            ),
            (
                2,
                "withdrawn-secret",
                "mat_withdrawn",
                "withdrawn",
                "mineral",
                1,
                "withdrawn-secret",
            ),
            (
                3,
                "invalid-secret",
                "mat_invalid",
                "published",
                "mineral",
                0,
                "invalid-secret",
            ),
            (
                4,
                "compound-secret",
                "mat_compound",
                "published",
                "compound",
                1,
                "compound-secret",
            ),
        ] {
            connection.execute(
                r#"
                INSERT INTO materials(
                    id, public_id, slug, record_type, canonical_name, formula,
                    description, mineral_family, identifiers_json,
                    properties_json, safety_json, search_text,
                    verification_status, data_quality_score, source_kind,
                    license_spdx, publication_status, nomenclature_status,
                    is_valid_species
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?3, 'SiO2', ?3, 'silicate',
                    '{"ima":"IMA-test"}', '{"hardness":7}', '{}', ?7,
                    'verified', 0.9, 'registry_import', 'CC0-1.0', ?5,
                    'approved', ?6
                )
                "#,
                params![
                    id,
                    public_id,
                    slug,
                    record_type,
                    publication_status,
                    valid,
                    search_text
                ],
            )?;
        }

        connection.execute(
            "INSERT INTO mineral_ingestion_authorities(policy, dataset_key, source_key, bound_batch_id) VALUES ('ima_identity_v1', 'ima', 'ima', 'missing-test-batch')",
            [],
        )?;
        for (key, value) in [
            ("discovery_country", "Testland"),
            ("first_reference", "Ref 1"),
            ("second_reference", "Ref 2"),
            ("source_status", "A"),
        ] {
            connection.execute(
                "INSERT INTO mineral_dataset_facts(dataset_key, material_id, fact_key, fact_value, source_release_id) VALUES ('ima', 1, ?1, ?2, 'missing-test-batch')",
                params![key, value],
            )?;
        }

        for (id, suffix) in [
            (1, "public"),
            (2, "withdrawn-secret"),
            (3, "invalid-secret"),
            (4, "compound-secret"),
        ] {
            connection.execute(
                "INSERT INTO evidence_sources(id, canonical_url, title, publisher, license_spdx, retrieved_at, content_hash) VALUES (?1, ?2, ?3, 'Test publisher', 'CC0-1.0', '2026-01-01T00:00:00Z', ?4)",
                params![id, format!("https://example.test/{suffix}"), suffix, format!("hash-{suffix}")],
            )?;
            connection.execute(
                r#"
                INSERT INTO material_evidence(
                    material_id, source_id, claim_scope, claim_json,
                    confidence, review_status, source_title, source_publisher,
                    source_license_spdx, source_retrieved_at, source_content_hash,
                    source_attribution_party, source_work_title, source_work_url,
                    source_license_url, source_changes_notice,
                    source_no_endorsement_notice,
                    source_derived_output_license_spdx
                ) VALUES (
                    ?1, ?1, 'identity', '{"value":"test"}', 0.9, 'verified',
                    ?2, 'Test publisher', 'CC0-1.0', '2026-01-01T00:00:00Z',
                    ?3, 'Attributor', 'Test work', 'https://example.test/work',
                    'https://example.test/license', 'Changed for test',
                    'No endorsement', 'CC0-1.0'
                )
                "#,
                params![id, suffix, format!("hash-{suffix}")],
            )?;
        }

        connection.execute(
            "INSERT INTO providers(id, slug, name, website_url, verification_status, trust_score, active) VALUES (1, 'active-provider', 'Active provider', 'https://provider.test', 'verified', 0.9, 1)",
            [],
        )?;
        connection.execute(
            "INSERT INTO providers(id, slug, name, website_url, verification_status, trust_score, active) VALUES (2, 'suspended-provider', 'Suspended provider', 'https://suspended.test', 'suspended', 0.1, 1)",
            [],
        )?;
        for (external_id, material_id, provider_id, title, expires_at, active) in [
            ("live", 1, 1, "Live offer", None, 1),
            (
                "future",
                1,
                1,
                "Future offer",
                Some("2999-01-01 00:00:00"),
                1,
            ),
            (
                "expired",
                1,
                1,
                "expired-offer-secret",
                Some("2000-01-01 00:00:00"),
                1,
            ),
            ("inactive", 1, 1, "inactive-offer-secret", None, 0),
            ("suspended", 1, 2, "suspended-offer-secret", None, 1),
            ("withdrawn", 2, 1, "withdrawn-offer-secret", None, 1),
        ] {
            connection.execute(
                r#"
                INSERT INTO offers(
                    material_id, provider_id, external_id, title, product_url,
                    currency_code, price_minor, currency_exponent,
                    pricing_basis, minimum_order_unit, stock_status,
                    verification_status, last_checked_at, expires_at, active
                ) VALUES (
                    ?1, ?2, ?3, ?4, 'https://provider.test/product', 'USD',
                    1234, 2, 'unit', 'item', 'in_stock', 'verified',
                    '2026-01-01 00:00:00', ?5, ?6
                )
                "#,
                params![
                    material_id,
                    provider_id,
                    external_id,
                    title,
                    expires_at,
                    active
                ],
            )?;
        }

        connection.execute(
            "INSERT INTO ingestion_runs(id, source_label, status) VALUES (1, 'private-test', 'running')",
            [],
        )?;
        connection.execute(
            r#"
            INSERT INTO mineral_review_revisions(
                material_slug, revision, ingestion_run_id, source_label,
                payload_json, status
            ) VALUES (
                'pending-review-secret', 1, 1, 'private-test',
                '{"canonical_name":"pending-review-secret"}', 'pending'
            )
            "#,
            [],
        )?;
        drop(connection);
        Ok(())
    }

    fn prepare_registry(data_root: &Path) -> Result<()> {
        let connection = Connection::open(data_root.join(LIVE_DATABASE_FILE))?;
        connection.execute_batch(
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
        )?;
        drop(connection);
        minerals::registry::init_registry_database_with_options(data_root, false)
    }

    fn file_snapshot(path: &Path) -> Result<(u64, String)> {
        let bytes = fs::read(path)?;
        Ok((bytes.len() as u64, hash_bytes(&bytes)))
    }
}
