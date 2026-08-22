use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
    path::{Component, Path},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Context, Result};
use rusqlite::{params, types::ValueRef, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use tracing::warn;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const MINERALS_DB_FILE: &str = "minerals.db";
const LEGACY_JSON_DB_FILE: &str = "minerals.db.json";
const LEGACY_MINERALS_DIR: &str = "minerals";
const IMAGES_DIR: &str = "images";
const FALLBACK_LANGUAGE: &str = "en";
const METADATA_SCHEMA_VERSION: i64 = 1;
const ADMIN_SQL_MAX_ROWS: usize = 500;
const ADMIN_SQL_MAX_LENGTH: usize = 100_000;
const ADMIN_SQL_MAX_CELL_BYTES: usize = 100_000;
const ADMIN_SQL_MAX_OUTPUT_BYTES: usize = 1_000_000;
const ADMIN_SQL_MAX_RUNTIME: Duration = Duration::from_secs(2);
const LEGACY_IMAGE_MAX_BYTES: u64 = 20 * 1024 * 1024;
const ADMIN_SQL_FORBIDDEN_KEYWORDS: &[&str] = &[
    "alter",
    "analyze",
    "attach",
    "begin",
    "commit",
    "create",
    "detach",
    "drop",
    "pragma",
    "reindex",
    "release",
    "rollback",
    "savepoint",
    "transaction",
    "truncate",
    "vacuum",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mineral {
    pub slug: String,
    pub folder_name: String,
    pub common_name: String,
    pub description: String,
    pub mineral_family: String,
    pub formula: String,
    pub hardness_mohs: f32,
    pub density_g_cm3: f32,
    pub crystal_system: String,
    pub color: String,
    pub streak: String,
    pub luster: String,
    pub major_elements_pct: BTreeMap<String, f32>,
    pub notes: String,
    pub image_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ReportRequest {
    pub audience: String,
    pub purpose: String,
    pub site_context: String,
}

impl Default for ReportRequest {
    fn default() -> Self {
        Self {
            audience: "technical geologist".to_string(),
            purpose: "exploration briefing".to_string(),
            site_context: "pilot drill campaign".to_string(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MineralFormData {
    pub draft_id: Option<String>,
    pub common_name: String,
    pub description: String,
    pub suggestion_context: String,
    pub preview_image_data_url: String,
    pub mineral_family: String,
    pub formula: String,
    pub hardness_mohs: String,
    pub density_g_cm3: String,
    pub crystal_system: String,
    pub color: String,
    pub streak: String,
    pub luster: String,
    pub major_elements_pct_text: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MineralDiskRecord {
    pub common_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(alias = "mineral_group")]
    pub mineral_family: String,
    pub formula: String,
    pub hardness_mohs: f32,
    pub density_g_cm3: f32,
    pub crystal_system: String,
    pub color: String,
    pub streak: String,
    pub luster: String,
    #[serde(default)]
    pub major_elements_pct: BTreeMap<String, f32>,
    pub notes: String,
    #[serde(default)]
    pub image_file: Option<String>,
}

pub struct NewImageRecord<'a> {
    pub bytes: &'a [u8],
    pub ext: &'a str,
    pub original_name: Option<&'a str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdminSqlExecution {
    pub statement_type: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub row_count: usize,
    pub affected_rows: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LocalizedMineralRecord {
    slug: String,
    folder_name: String,
    lang_code: String,
    metadata: MineralDiskRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct LegacyJsonDatabase {
    records: Vec<LocalizedMineralRecord>,
}

#[derive(Debug, Clone)]
struct StoredImage {
    stored_name: String,
    original_name: Option<String>,
    content_type: String,
}

#[derive(Debug)]
struct PendingImageFile {
    path: std::path::PathBuf,
    keep: bool,
}

impl PendingImageFile {
    fn persist(mut self) {
        self.keep = true;
    }
}

impl Drop for PendingImageFile {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[derive(Debug, Clone)]
struct OwnedImageInput {
    bytes: Vec<u8>,
    ext: String,
    original_name: Option<String>,
}

impl<'a> From<NewImageRecord<'a>> for OwnedImageInput {
    fn from(value: NewImageRecord<'a>) -> Self {
        Self {
            bytes: value.bytes.to_vec(),
            ext: value.ext.to_string(),
            original_name: value.original_name.map(str::to_string),
        }
    }
}

pub fn init_minerals_database(data_root: &Path) -> Result<()> {
    fs::create_dir_all(data_root)
        .with_context(|| format!("failed to create {}", data_root.display()))?;
    set_private_directory_permissions(data_root)?;
    fs::create_dir_all(data_root.join(IMAGES_DIR))
        .with_context(|| format!("failed to create {}", data_root.join(IMAGES_DIR).display()))?;
    set_private_directory_permissions(&data_root.join(IMAGES_DIR))?;
    ensure_real_directory_within(data_root, &data_root.join(IMAGES_DIR), "images directory")?;
    validate_database_file_paths(data_root)?;

    let mut conn = open_connection(data_root)?;
    initialize_schema(&mut conn)?;

    let catalog_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM catalog", [], |row| row.get(0))
        .context("failed to count catalog rows")?;

    if catalog_count == 0 {
        let migrated_from_sql = migrate_from_legacy_sql_schema(data_root, &mut conn)?;
        if !migrated_from_sql {
            migrate_legacy_sources(data_root, &mut conn)?;
        }
        prune_orphan_images(&conn, data_root)?;
    }

    harden_database_file_permissions(data_root)?;

    Ok(())
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("failed to set private permissions on {}", path.display()))
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

fn validate_database_file_paths(data_root: &Path) -> Result<()> {
    for name in [
        MINERALS_DB_FILE.to_string(),
        format!("{MINERALS_DB_FILE}-wal"),
        format!("{MINERALS_DB_FILE}-shm"),
    ] {
        let path = data_root.join(name);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(anyhow!(
                    "database path {} must be a regular file, not a symlink",
                    path.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to inspect database path {}", path.display()))
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
fn harden_database_file_permissions(data_root: &Path) -> Result<()> {
    validate_database_file_paths(data_root)?;
    for name in [
        MINERALS_DB_FILE.to_string(),
        format!("{MINERALS_DB_FILE}-wal"),
        format!("{MINERALS_DB_FILE}-shm"),
    ] {
        let path = data_root.join(name);
        match fs::symlink_metadata(&path) {
            Ok(_) => {
                fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).with_context(
                    || format!("failed to set private permissions on {}", path.display()),
                )?;
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to inspect database path {}", path.display()))
            }
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn harden_database_file_permissions(_data_root: &Path) -> Result<()> {
    Ok(())
}

pub fn load_minerals(data_root: &Path, lang_code: &str) -> Result<Vec<Mineral>> {
    let conn = open_connection(data_root)?;
    let started_at = std::time::Instant::now();
    conn.progress_handler(
        1_000,
        Some(move || started_at.elapsed() >= ADMIN_SQL_MAX_RUNTIME),
    );
    let mut stmt = conn
        .prepare(
            "
            SELECT
              c.slug,
              c.folder_name,
              c.metadata_json,
              m.common_name,
              m.description,
              m.mineral_family,
              m.formula,
              m.hardness_mohs,
              m.density_g_cm3,
              m.crystal_system,
              m.color,
              m.streak,
              m.luster,
              m.major_elements_pct_json,
              m.notes,
              ci.stored_name AS catalog_image_name,
              mi.stored_name AS mineral_image_name
            FROM catalog c
            JOIN minerals m ON m.id = c.source_mineral_id
            LEFT JOIN images ci ON ci.id = c.image_id
            LEFT JOIN images mi ON mi.id = m.image_id
            ORDER BY c.slug ASC
            ",
        )
        .context("failed to prepare catalog query")?;

    let mut rows = stmt.query([]).context("failed to execute catalog query")?;

    let mut minerals = Vec::new();

    while let Some(row) = rows.next().context("failed to iterate catalog rows")? {
        let slug: String = row.get(0).context("missing slug")?;
        let folder_name: String = row.get(1).context("missing folder_name")?;
        let metadata_json: String = row.get(2).context("missing metadata_json")?;

        let base_record = MineralDiskRecord {
            common_name: row.get(3).context("missing common_name")?,
            description: row.get(4).context("missing description")?,
            mineral_family: row.get(5).context("missing mineral_family")?,
            formula: row.get(6).context("missing formula")?,
            hardness_mohs: row.get(7).context("missing hardness_mohs")?,
            density_g_cm3: row.get(8).context("missing density_g_cm3")?,
            crystal_system: row.get(9).context("missing crystal_system")?,
            color: row.get(10).context("missing color")?,
            streak: row.get(11).context("missing streak")?,
            luster: row.get(12).context("missing luster")?,
            major_elements_pct: major_elements_from_json(
                &row.get::<_, String>(13).context("missing major elements")?,
            )?,
            notes: row.get(14).context("missing notes")?,
            image_file: None,
        };

        let localized = localized_metadata_from_json(&metadata_json)?;
        let selected = localized
            .get(lang_code)
            .or_else(|| localized.get(FALLBACK_LANGUAGE))
            .or_else(|| localized.values().next())
            .cloned()
            .unwrap_or(base_record);

        let catalog_image_name: Option<String> = row.get(15).ok();
        let mineral_image_name: Option<String> = row.get(16).ok();
        let selected_image = catalog_image_name.or(mineral_image_name);

        minerals.push(Mineral {
            slug,
            folder_name,
            common_name: selected.common_name,
            description: selected.description,
            mineral_family: selected.mineral_family,
            formula: selected.formula,
            hardness_mohs: selected.hardness_mohs,
            density_g_cm3: selected.density_g_cm3,
            crystal_system: selected.crystal_system,
            color: selected.color,
            streak: selected.streak,
            luster: selected.luster,
            major_elements_pct: selected.major_elements_pct,
            notes: selected.notes,
            image_path: selected_image.map(|value| format!("/media/{IMAGES_DIR}/{value}")),
        });
    }

    minerals.sort_by(|a, b| a.common_name.cmp(&b.common_name));
    Ok(minerals)
}

pub fn load_registered_image(
    data_root: &Path,
    stored_name: &str,
) -> Result<Option<(Vec<u8>, String)>> {
    if stored_name.is_empty()
        || stored_name.len() > 255
        || !stored_name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_'))
        || matches!(stored_name, "." | "..")
    {
        return Ok(None);
    }

    let conn = open_connection(data_root)?;
    let stored_content_type = conn
        .query_row(
            "SELECT COALESCE(content_type, '') FROM images WHERE stored_name = ?1",
            params![stored_name],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .context("failed to resolve registered image")?;
    let Some(stored_content_type) = stored_content_type else {
        return Ok(None);
    };
    let content_type = if stored_content_type.is_empty() {
        Path::new(stored_name)
            .extension()
            .and_then(|value| value.to_str())
            .map(content_type_from_ext)
            .unwrap_or("")
            .to_string()
    } else {
        stored_content_type
    };
    if !matches!(
        content_type.as_str(),
        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    ) {
        return Ok(None);
    }

    let images_root = data_root.join(IMAGES_DIR);
    let image_path = images_root.join(stored_name);
    let metadata = match fs::symlink_metadata(&image_path) {
        Ok(metadata) if metadata.file_type().is_file() && !metadata.file_type().is_symlink() => {
            metadata
        }
        Ok(_) => return Ok(None),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err).context("failed to inspect registered image"),
    };
    if metadata.len() > 25 * 1024 * 1024 {
        return Ok(None);
    }
    let canonical_root = fs::canonicalize(&images_root)
        .with_context(|| format!("failed to resolve {}", images_root.display()))?;
    let canonical_image = fs::canonicalize(&image_path)
        .with_context(|| format!("failed to resolve {}", image_path.display()))?;
    if !canonical_image.starts_with(&canonical_root) {
        return Ok(None);
    }
    let bytes = fs::read(&canonical_image)
        .with_context(|| format!("failed to read registered image {}", image_path.display()))?;
    Ok(Some((bytes, content_type)))
}

pub fn save_localized_mineral_records(
    data_root: &Path,
    slug: &str,
    folder_name: &str,
    localized_records: &HashMap<String, MineralDiskRecord>,
    image: NewImageRecord<'_>,
) -> Result<()> {
    let normalized = normalize_localized_records(localized_records)?;
    let image_input = OwnedImageInput::from(image);
    let mut conn = open_connection(data_root)?;

    {
        let tx = conn
            .transaction()
            .context("failed to open transaction for save")?;
        let pending_image = upsert_catalog_bundle(
            &tx,
            data_root,
            slug,
            folder_name,
            &normalized,
            Some(&image_input),
        )?;
        tx.commit().context("failed to commit save transaction")?;
        if let Some(pending_image) = pending_image {
            pending_image.persist();
        }
    }

    if let Err(err) = prune_orphan_images(&conn, data_root) {
        warn!("post-commit orphan image cleanup failed: {err:#}");
    }
    Ok(())
}

pub fn delete_mineral_records(data_root: &Path, slug: &str) -> Result<usize> {
    let mut conn = open_connection(data_root)?;
    let deleted_rows = {
        let tx = conn
            .transaction()
            .context("failed to open delete transaction")?;

        let entry = tx
            .query_row(
                "SELECT id, source_mineral_id FROM catalog WHERE slug = ?1",
                params![slug],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()
            .context("failed to load catalog row for deletion")?;

        let Some((catalog_id, source_mineral_id)) = entry else {
            tx.commit().context("failed to close delete transaction")?;
            return Ok(0);
        };

        let deleted = tx
            .execute("DELETE FROM catalog WHERE id = ?1", params![catalog_id])
            .context("failed to delete catalog row")?;

        let remaining_refs: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM catalog WHERE source_mineral_id = ?1",
                params![source_mineral_id],
                |row| row.get(0),
            )
            .context("failed to count mineral references")?;

        if remaining_refs == 0 {
            tx.execute(
                "DELETE FROM minerals WHERE id = ?1",
                params![source_mineral_id],
            )
            .context("failed to delete orphan mineral")?;
        }

        tx.commit().context("failed to commit delete transaction")?;
        deleted
    };

    if let Err(err) = prune_orphan_images(&conn, data_root) {
        warn!("post-delete orphan image cleanup failed: {err:#}");
    }
    Ok(deleted_rows)
}

pub fn mineral_slug_exists(data_root: &Path, slug: &str) -> Result<bool> {
    let conn = open_connection(data_root)?;
    let exists: i64 = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM catalog WHERE slug = ?1 LIMIT 1)",
            params![slug],
            |row| row.get(0),
        )
        .context("failed to query slug existence")?;

    Ok(exists == 1)
}

pub fn execute_admin_sql(data_root: &Path, sql: &str) -> Result<AdminSqlExecution> {
    execute_admin_sql_with_runtime(data_root, sql, ADMIN_SQL_MAX_RUNTIME)
}

fn execute_admin_sql_with_runtime(
    data_root: &Path,
    sql: &str,
    max_runtime: Duration,
) -> Result<AdminSqlExecution> {
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("SQL cannot be empty"));
    }

    if trimmed.len() > ADMIN_SQL_MAX_LENGTH {
        return Err(anyhow!(
            "SQL exceeds the maximum length of {ADMIN_SQL_MAX_LENGTH} characters"
        ));
    }

    let analysis_sql = strip_sql_literals_and_comments(trimmed);
    enforce_single_statement(&analysis_sql)?;
    reject_forbidden_sql_keywords(&analysis_sql)?;

    let statement_type = first_sql_keyword(&analysis_sql)
        .unwrap_or_else(|| "unknown".to_string())
        .to_ascii_lowercase();

    let conn = open_connection(data_root)?;
    let started_at = Instant::now();
    conn.progress_handler(1_000, Some(move || started_at.elapsed() >= max_runtime));
    let mut stmt = conn
        .prepare(trimmed)
        .context("failed to prepare SQL statement")?;
    if !stmt.readonly() {
        return Err(anyhow!(
            "admin SQL console is read-only; use reviewed application workflows for writes"
        ));
    }

    let column_names = stmt
        .column_names()
        .iter()
        .map(|name| name.to_string())
        .collect::<Vec<_>>();

    if !column_names.is_empty() {
        let mut rows = Vec::new();
        let mut truncated = false;
        let mut output_bytes = 0usize;
        let mut query_rows = stmt.query([]).context("failed to execute SQL query")?;

        while let Some(row) = query_rows
            .next()
            .context("failed to iterate SQL query rows")?
        {
            if rows.len() >= ADMIN_SQL_MAX_ROWS {
                truncated = true;
                break;
            }

            let mut out_row = Vec::with_capacity(column_names.len());
            for idx in 0..column_names.len() {
                let value = row
                    .get_ref(idx)
                    .with_context(|| format!("failed to read SQL value at column {idx}"))?;
                let rendered = sql_value_to_string(value);
                if rendered.len() > ADMIN_SQL_MAX_CELL_BYTES
                    || output_bytes.saturating_add(rendered.len()) > ADMIN_SQL_MAX_OUTPUT_BYTES
                {
                    truncated = true;
                    break;
                }
                output_bytes += rendered.len();
                out_row.push(rendered);
            }
            if truncated {
                break;
            }
            rows.push(out_row);
        }

        return Ok(AdminSqlExecution {
            statement_type,
            columns: column_names,
            row_count: rows.len(),
            rows,
            affected_rows: 0,
            truncated,
        });
    }

    Err(anyhow!("admin SQL statement did not return rows"))
}

fn sql_value_to_string(value: ValueRef<'_>) -> String {
    match value {
        ValueRef::Null => "NULL".to_string(),
        ValueRef::Integer(v) => v.to_string(),
        ValueRef::Real(v) => v.to_string(),
        ValueRef::Text(v) => String::from_utf8_lossy(v).to_string(),
        ValueRef::Blob(v) => format!("<blob:{} bytes>", v.len()),
    }
}

fn enforce_single_statement(sql: &str) -> Result<()> {
    let statements = sql
        .split(';')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .count();

    if statements == 0 {
        return Err(anyhow!("SQL cannot be empty"));
    }

    if statements > 1 {
        return Err(anyhow!("Only one SQL statement is allowed per execution"));
    }

    Ok(())
}

fn reject_forbidden_sql_keywords(sql: &str) -> Result<()> {
    let keyword = first_sql_keyword(sql).ok_or_else(|| anyhow!("SQL cannot be empty"))?;
    if ADMIN_SQL_FORBIDDEN_KEYWORDS.contains(&keyword.as_str()) {
        return Err(anyhow!(
            "keyword '{keyword}' is not allowed in admin SQL console"
        ));
    }
    Ok(())
}

fn first_sql_keyword(sql: &str) -> Option<String> {
    let mut token = String::new();
    for ch in sql.chars() {
        if ch.is_ascii_alphabetic() || ch == '_' {
            token.push(ch.to_ascii_lowercase());
        } else if !token.is_empty() {
            return Some(token);
        }
    }

    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

fn strip_sql_literals_and_comments(sql: &str) -> String {
    #[derive(Clone, Copy)]
    enum ScanState {
        Normal,
        SingleQuote,
        DoubleQuote,
        LineComment,
        BlockComment,
    }

    let chars = sql.chars().collect::<Vec<_>>();
    let mut out = String::with_capacity(chars.len());
    let mut index = 0usize;
    let mut state = ScanState::Normal;

    while index < chars.len() {
        let ch = chars[index];
        match state {
            ScanState::Normal => {
                if ch == '\'' {
                    out.push(' ');
                    state = ScanState::SingleQuote;
                } else if ch == '"' {
                    out.push(' ');
                    state = ScanState::DoubleQuote;
                } else if ch == '-' && chars.get(index + 1) == Some(&'-') {
                    out.push(' ');
                    out.push(' ');
                    index += 1;
                    state = ScanState::LineComment;
                } else if ch == '/' && chars.get(index + 1) == Some(&'*') {
                    out.push(' ');
                    out.push(' ');
                    index += 1;
                    state = ScanState::BlockComment;
                } else {
                    out.push(ch);
                }
            }
            ScanState::SingleQuote => {
                out.push(' ');
                if ch == '\'' {
                    if chars.get(index + 1) == Some(&'\'') {
                        out.push(' ');
                        index += 1;
                    } else {
                        state = ScanState::Normal;
                    }
                }
            }
            ScanState::DoubleQuote => {
                out.push(' ');
                if ch == '"' {
                    if chars.get(index + 1) == Some(&'"') {
                        out.push(' ');
                        index += 1;
                    } else {
                        state = ScanState::Normal;
                    }
                }
            }
            ScanState::LineComment => {
                if ch == '\n' {
                    out.push('\n');
                    state = ScanState::Normal;
                } else {
                    out.push(' ');
                }
            }
            ScanState::BlockComment => {
                out.push(' ');
                if ch == '*' && chars.get(index + 1) == Some(&'/') {
                    out.push(' ');
                    index += 1;
                    state = ScanState::Normal;
                }
            }
        }

        index += 1;
    }

    out
}

fn open_connection(data_root: &Path) -> Result<Connection> {
    let db_path = data_root.join(MINERALS_DB_FILE);
    let conn = Connection::open(&db_path)
        .with_context(|| format!("failed to open {}", db_path.display()))?;
    conn.busy_timeout(Duration::from_secs(5))
        .context("failed to configure sqlite busy timeout")?;
    conn.execute_batch(
        "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;",
    )
    .context("failed to configure sqlite connection")?;
    Ok(conn)
}

fn initialize_schema(conn: &mut Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS images (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            stored_name TEXT NOT NULL UNIQUE,
            original_name TEXT,
            content_type TEXT,
            metadata_schema_version INTEGER NOT NULL DEFAULT 1,
            embeddings_json TEXT NOT NULL DEFAULT '[]',
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS minerals (
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
            image_id INTEGER,
            metadata_schema_version INTEGER NOT NULL DEFAULT 1,
            embeddings_json TEXT NOT NULL DEFAULT '[]',
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(image_id) REFERENCES images(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS catalog (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            slug TEXT NOT NULL UNIQUE,
            folder_name TEXT NOT NULL,
            source_mineral_id INTEGER NOT NULL,
            metadata_json TEXT NOT NULL DEFAULT '{}',
            image_id INTEGER,
            metadata_schema_version INTEGER NOT NULL DEFAULT 1,
            embeddings_json TEXT NOT NULL DEFAULT '[]',
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(source_mineral_id) REFERENCES minerals(id) ON DELETE RESTRICT,
            FOREIGN KEY(image_id) REFERENCES images(id) ON DELETE SET NULL
        );
        ",
    )
    .context("failed to create sqlite schema")?;

    ensure_column_exists(conn, "minerals", "image_id", "INTEGER")?;
    ensure_column_exists(
        conn,
        "catalog",
        "metadata_json",
        "TEXT NOT NULL DEFAULT '{}'",
    )?;
    ensure_column_exists(conn, "catalog", "image_id", "INTEGER")?;

    conn.execute_batch(
        "
        CREATE INDEX IF NOT EXISTS idx_catalog_source_mineral
            ON catalog(source_mineral_id);

        CREATE INDEX IF NOT EXISTS idx_catalog_image
            ON catalog(image_id);

        CREATE INDEX IF NOT EXISTS idx_minerals_image
            ON minerals(image_id);
        ",
    )
    .context("failed to create sqlite indexes")?;

    Ok(())
}

fn column_exists(conn: &Connection, table_name: &str, column_name: &str) -> Result<bool> {
    let pragma_sql = format!("PRAGMA table_info({table_name})");
    let mut stmt = conn
        .prepare(&pragma_sql)
        .with_context(|| format!("failed to inspect columns for table '{table_name}'"))?;
    let mut rows = stmt
        .query([])
        .with_context(|| format!("failed to query table info for '{table_name}'"))?;

    while let Some(row) = rows
        .next()
        .with_context(|| format!("failed to iterate columns for table '{table_name}'"))?
    {
        let existing: String = row.get(1).context("failed to read PRAGMA column name")?;
        if existing == column_name {
            return Ok(true);
        }
    }

    Ok(false)
}

fn ensure_column_exists(
    conn: &Connection,
    table_name: &str,
    column_name: &str,
    definition: &str,
) -> Result<()> {
    if column_exists(conn, table_name, column_name)? {
        return Ok(());
    }

    conn.execute(
        &format!("ALTER TABLE {table_name} ADD COLUMN {column_name} {definition}"),
        [],
    )
    .with_context(|| {
        format!("failed to add missing column '{column_name}' to table '{table_name}'")
    })?;

    Ok(())
}

fn table_exists(conn: &Connection, table_name: &str) -> Result<bool> {
    let exists: i64 = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            params![table_name],
            |row| row.get(0),
        )
        .context("failed to inspect sqlite schema")?;
    Ok(exists == 1)
}

fn migrate_from_legacy_sql_schema(data_root: &Path, conn: &mut Connection) -> Result<bool> {
    if !table_exists(conn, "catalog_entries")? || !table_exists(conn, "catalog_metadata")? {
        return Ok(false);
    }

    let legacy_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM catalog_entries", [], |row| row.get(0))
        .context("failed to count legacy catalog_entries")?;

    if legacy_count == 0 {
        drop_legacy_schema_tables(conn)?;
        return Ok(false);
    }

    #[derive(Default)]
    struct LegacyGroup {
        folder_name: String,
        localized: HashMap<String, MineralDiskRecord>,
        image_name: Option<String>,
    }

    let mut grouped: HashMap<String, LegacyGroup> = HashMap::new();

    let mut stmt = conn
        .prepare(
            "
            SELECT
              ce.slug,
              ce.folder_name,
              cm.lang_code,
              cm.common_name,
              cm.description,
              cm.mineral_family,
              cm.formula,
              cm.hardness_mohs,
              cm.density_g_cm3,
              cm.crystal_system,
              cm.color,
              cm.streak,
              cm.luster,
              cm.major_elements_pct_json,
              cm.notes,
              (
                SELECT i.stored_name
                FROM catalog_entry_images cei
                JOIN images i ON i.id = cei.image_id
                WHERE cei.catalog_entry_id = ce.id
                ORDER BY cei.sort_order ASC, i.id ASC
                LIMIT 1
              ) AS image_name
            FROM catalog_entries ce
            JOIN catalog_metadata cm ON cm.catalog_entry_id = ce.id
            ORDER BY ce.slug, cm.lang_code
            ",
        )
        .context("failed to prepare legacy schema migration query")?;

    let rows = stmt
        .query_map([], |row| {
            let major_elements_raw: String = row.get(13)?;
            let major_elements_pct: BTreeMap<String, f32> =
                serde_json::from_str(&major_elements_raw).map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(
                        13,
                        rusqlite::types::Type::Text,
                        Box::new(err),
                    )
                })?;

            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                MineralDiskRecord {
                    common_name: row.get(3)?,
                    description: row.get(4)?,
                    mineral_family: row.get(5)?,
                    formula: row.get(6)?,
                    hardness_mohs: row.get(7)?,
                    density_g_cm3: row.get(8)?,
                    crystal_system: row.get(9)?,
                    color: row.get(10)?,
                    streak: row.get(11)?,
                    luster: row.get(12)?,
                    major_elements_pct,
                    notes: row.get(14)?,
                    image_file: row.get::<_, Option<String>>(15)?,
                },
                row.get::<_, Option<String>>(15)?,
            ))
        })
        .context("failed to execute legacy schema migration query")?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("failed to collect legacy schema migration rows")?;

    drop(stmt);

    for row in rows {
        let (slug, folder_name, lang_code, metadata, image_name) = row;

        let entry = grouped.entry(slug).or_default();
        entry.folder_name = folder_name;
        entry.localized.insert(lang_code, metadata);
        if image_name.is_some() {
            entry.image_name = image_name;
        }
    }

    if grouped.is_empty() {
        return Ok(false);
    }

    let tx = conn
        .transaction()
        .context("failed to open transaction for legacy SQL migration")?;
    let mut pending_images = Vec::new();

    for (slug, group) in grouped {
        let image = match group.image_name {
            Some(stored_name) => resolve_shared_image_by_name(data_root, &stored_name)?,
            None => None,
        };

        if let Some(pending_image) = upsert_catalog_bundle(
            &tx,
            data_root,
            &slug,
            &group.folder_name,
            &group.localized,
            image.as_ref(),
        )? {
            pending_images.push(pending_image);
        }
    }

    tx.commit()
        .context("failed to commit legacy SQL migration transaction")?;
    for pending_image in pending_images {
        pending_image.persist();
    }

    drop_legacy_schema_tables(conn)?;

    Ok(true)
}

fn drop_legacy_schema_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        DROP TABLE IF EXISTS catalog_entry_images;
        DROP TABLE IF EXISTS mineral_images;
        DROP TABLE IF EXISTS catalog_entry_minerals;
        DROP TABLE IF EXISTS catalog_metadata;
        DROP TABLE IF EXISTS catalog_entries;
        DROP TABLE IF EXISTS metadata_schemas;
        ",
    )
    .context("failed to drop legacy schema tables")?;

    Ok(())
}

fn migrate_legacy_sources(data_root: &Path, conn: &mut Connection) -> Result<()> {
    if let Some(records) = read_legacy_json_records(data_root)? {
        if !records.is_empty() {
            import_localized_records(data_root, conn, records)?;
            return Ok(());
        }
    }

    let folder_records = migrate_legacy_folder_records(data_root)?;
    if !folder_records.is_empty() {
        import_localized_records(data_root, conn, folder_records)?;
    }

    Ok(())
}

fn read_legacy_json_records(data_root: &Path) -> Result<Option<Vec<LocalizedMineralRecord>>> {
    let legacy_path = data_root.join(LEGACY_JSON_DB_FILE);
    if !legacy_path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(&legacy_path)
        .with_context(|| format!("failed to read {}", legacy_path.display()))?;
    if raw.trim().is_empty() {
        return Ok(Some(Vec::new()));
    }

    let parsed: LegacyJsonDatabase = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse {}", legacy_path.display()))?;
    Ok(Some(parsed.records))
}

fn import_localized_records(
    data_root: &Path,
    conn: &mut Connection,
    records: Vec<LocalizedMineralRecord>,
) -> Result<()> {
    let mut grouped: HashMap<String, (String, HashMap<String, MineralDiskRecord>)> = HashMap::new();
    for row in records {
        if !is_valid_mineral_folder_name(&row.slug) {
            return Err(anyhow!(
                "legacy mineral slug '{}' is not a valid mineral identifier",
                row.slug
            ));
        }
        if !is_valid_mineral_folder_name(&row.folder_name) {
            return Err(anyhow!(
                "legacy mineral folder '{}' is not a valid mineral identifier",
                row.folder_name
            ));
        }
        let entry = grouped
            .entry(row.slug.clone())
            .or_insert_with(|| (row.folder_name.clone(), HashMap::new()));
        if entry.0 != row.folder_name {
            return Err(anyhow!(
                "legacy mineral '{}' references multiple folders",
                row.slug
            ));
        }
        entry.0 = row.folder_name.clone();
        entry.1.insert(row.lang_code.clone(), row.metadata.clone());
    }

    let tx = conn
        .transaction()
        .context("failed to open migration transaction")?;
    let mut pending_images = Vec::new();

    for (slug, (folder_name, localized)) in grouped {
        let image = resolve_legacy_image_for_slug(data_root, &folder_name, &localized)?;
        if let Some(pending_image) = upsert_catalog_bundle(
            &tx,
            data_root,
            &slug,
            &folder_name,
            &localized,
            image.as_ref(),
        )? {
            pending_images.push(pending_image);
        }
    }

    tx.commit()
        .context("failed to commit migration transaction")?;
    for pending_image in pending_images {
        pending_image.persist();
    }
    Ok(())
}

fn upsert_catalog_bundle(
    tx: &Transaction<'_>,
    data_root: &Path,
    slug: &str,
    folder_name: &str,
    localized_records: &HashMap<String, MineralDiskRecord>,
    image: Option<&OwnedImageInput>,
) -> Result<Option<PendingImageFile>> {
    let normalized = normalize_localized_records(localized_records)?;
    let canonical = normalized
        .get(FALLBACK_LANGUAGE)
        .or_else(|| normalized.values().next())
        .ok_or_else(|| anyhow!("missing canonical metadata for '{slug}'"))?;

    let canonical_major_json = major_elements_to_json(&canonical.major_elements_pct)?;

    let source_mineral_id = match tx
        .query_row(
            "SELECT id FROM minerals WHERE slug = ?1",
            params![slug],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .context("failed to check existing mineral")?
    {
        Some(id) => {
            tx.execute(
                "
                UPDATE minerals
                SET
                  common_name = ?1,
                  description = ?2,
                  mineral_family = ?3,
                  formula = ?4,
                  hardness_mohs = ?5,
                  density_g_cm3 = ?6,
                  crystal_system = ?7,
                  color = ?8,
                  streak = ?9,
                  luster = ?10,
                  major_elements_pct_json = ?11,
                  notes = ?12,
                  metadata_schema_version = ?13,
                  updated_at = CURRENT_TIMESTAMP
                WHERE id = ?14
                ",
                params![
                    canonical.common_name,
                    canonical.description,
                    canonical.mineral_family,
                    canonical.formula,
                    canonical.hardness_mohs,
                    canonical.density_g_cm3,
                    canonical.crystal_system,
                    canonical.color,
                    canonical.streak,
                    canonical.luster,
                    canonical_major_json,
                    canonical.notes,
                    METADATA_SCHEMA_VERSION,
                    id,
                ],
            )
            .context("failed to update mineral row")?;
            id
        }
        None => {
            tx.execute(
                "
                INSERT INTO minerals (
                  slug,
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
                  major_elements_pct_json,
                  notes,
                  metadata_schema_version,
                  embeddings_json
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, '[]')
                ",
                params![
                    slug,
                    canonical.common_name,
                    canonical.description,
                    canonical.mineral_family,
                    canonical.formula,
                    canonical.hardness_mohs,
                    canonical.density_g_cm3,
                    canonical.crystal_system,
                    canonical.color,
                    canonical.streak,
                    canonical.luster,
                    canonical_major_json,
                    canonical.notes,
                    METADATA_SCHEMA_VERSION,
                ],
            )
            .context("failed to insert mineral row")?;
            tx.last_insert_rowid()
        }
    };

    let metadata_json = localized_metadata_to_json(&normalized)?;

    let catalog_id = match tx
        .query_row(
            "SELECT id FROM catalog WHERE slug = ?1",
            params![slug],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .context("failed to check existing catalog row")?
    {
        Some(id) => {
            tx.execute(
                "
                UPDATE catalog
                SET
                  folder_name = ?1,
                  source_mineral_id = ?2,
                  metadata_json = ?3,
                  metadata_schema_version = ?4,
                  updated_at = CURRENT_TIMESTAMP
                WHERE id = ?5
                ",
                params![
                    folder_name,
                    source_mineral_id,
                    metadata_json,
                    METADATA_SCHEMA_VERSION,
                    id
                ],
            )
            .context("failed to update catalog row")?;
            id
        }
        None => {
            tx.execute(
                "
                INSERT INTO catalog (
                  slug,
                  folder_name,
                  source_mineral_id,
                  metadata_json,
                  metadata_schema_version,
                  embeddings_json
                )
                VALUES (?1, ?2, ?3, ?4, ?5, '[]')
                ",
                params![
                    slug,
                    folder_name,
                    source_mineral_id,
                    metadata_json,
                    METADATA_SCHEMA_VERSION,
                ],
            )
            .context("failed to insert catalog row")?;
            tx.last_insert_rowid()
        }
    };

    let pending_image = if let Some(image_input) = image {
        let stored_image = store_image_file(
            data_root,
            slug,
            &image_input.ext,
            &image_input.bytes,
            image_input.original_name.as_deref(),
        )?;
        let pending_image = PendingImageFile {
            path: data_root.join(IMAGES_DIR).join(&stored_image.stored_name),
            keep: false,
        };

        tx.execute(
            "
            INSERT INTO images (
              stored_name,
              original_name,
              content_type,
              metadata_schema_version,
              embeddings_json
            )
            VALUES (?1, ?2, ?3, ?4, '[]')
            ",
            params![
                stored_image.stored_name,
                stored_image.original_name,
                stored_image.content_type,
                METADATA_SCHEMA_VERSION,
            ],
        )
        .context("failed to insert image row")?;

        let image_id = tx.last_insert_rowid();

        tx.execute(
            "UPDATE catalog SET image_id = ?1 WHERE id = ?2",
            params![image_id, catalog_id],
        )
        .context("failed to set catalog image reference")?;

        tx.execute(
            "UPDATE minerals SET image_id = ?1 WHERE id = ?2",
            params![image_id, source_mineral_id],
        )
        .context("failed to set mineral image reference")?;
        Some(pending_image)
    } else {
        None
    };

    Ok(pending_image)
}

fn prune_orphan_images(conn: &Connection, data_root: &Path) -> Result<()> {
    let mut stmt = conn
        .prepare(
            "
            SELECT i.id, i.stored_name
            FROM images i
            LEFT JOIN catalog c ON c.image_id = i.id
            LEFT JOIN minerals m ON m.image_id = i.id
            WHERE c.image_id IS NULL AND m.image_id IS NULL
            ",
        )
        .context("failed to prepare orphan image query")?;

    let orphan_rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .context("failed to execute orphan image query")?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("failed to collect orphan image rows")?;

    for (image_id, stored_name) in orphan_rows {
        let image_path = data_root.join(IMAGES_DIR).join(&stored_name);
        if image_path.exists() {
            fs::remove_file(&image_path)
                .with_context(|| format!("failed to delete {}", image_path.display()))?;
        }

        conn.execute("DELETE FROM images WHERE id = ?1", params![image_id])
            .with_context(|| format!("failed to delete orphan image id {image_id}"))?;
    }

    Ok(())
}

fn store_image_file(
    data_root: &Path,
    slug: &str,
    ext: &str,
    bytes: &[u8],
    original_name: Option<&str>,
) -> Result<StoredImage> {
    if !is_valid_mineral_folder_name(slug) {
        return Err(anyhow!(
            "cannot store an image for invalid mineral slug '{slug}'"
        ));
    }
    let normalized_ext = normalize_image_extension(ext);
    let configured_images_root = data_root.join(IMAGES_DIR);
    fs::create_dir_all(&configured_images_root)
        .with_context(|| format!("failed to create {}", configured_images_root.display()))?;
    let images_root =
        ensure_real_directory_within(data_root, &configured_images_root, "images directory")?;

    for attempt in 0..64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let candidate = format!("{slug}.{now:x}.{attempt}.{normalized_ext}");
        let target = images_root.join(&candidate);
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to create image {}", target.display()))
            }
        };
        if let Err(error) = file.write_all(bytes) {
            drop(file);
            let _ = fs::remove_file(&target);
            return Err(error)
                .with_context(|| format!("failed to write image {}", target.display()));
        }

        return Ok(StoredImage {
            stored_name: candidate,
            original_name: original_name
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            content_type: content_type_from_ext(&normalized_ext).to_string(),
        });
    }

    Err(anyhow!(
        "failed to allocate unique image filename for slug '{slug}'"
    ))
}

fn ensure_real_directory_within(
    data_root: &Path,
    directory: &Path,
    label: &str,
) -> Result<std::path::PathBuf> {
    let metadata = fs::symlink_metadata(directory)
        .with_context(|| format!("failed to inspect {label} {}", directory.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(anyhow!(
            "{label} {} must be a real directory, not a symlink",
            directory.display()
        ));
    }
    let canonical_root = fs::canonicalize(data_root)
        .with_context(|| format!("failed to resolve data root {}", data_root.display()))?;
    let canonical_directory = fs::canonicalize(directory)
        .with_context(|| format!("failed to resolve {label} {}", directory.display()))?;
    if !canonical_directory.starts_with(&canonical_root) {
        return Err(anyhow!(
            "{label} {} escapes data root {}",
            canonical_directory.display(),
            canonical_root.display()
        ));
    }
    Ok(canonical_directory)
}

fn resolve_shared_image_by_name(
    data_root: &Path,
    stored_name: &str,
) -> Result<Option<OwnedImageInput>> {
    let image_path = data_root.join(IMAGES_DIR).join(stored_name);
    if !image_path.exists() {
        return Ok(None);
    }

    let ext = image_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("jpg")
        .to_string();

    let bytes = fs::read(&image_path)
        .with_context(|| format!("failed to read shared image {}", image_path.display()))?;

    Ok(Some(OwnedImageInput {
        bytes,
        ext,
        original_name: Some(stored_name.to_string()),
    }))
}

fn resolve_legacy_image_for_slug(
    data_root: &Path,
    folder_name: &str,
    localized: &HashMap<String, MineralDiskRecord>,
) -> Result<Option<OwnedImageInput>> {
    if !is_valid_mineral_folder_name(folder_name) {
        return Err(anyhow!(
            "legacy mineral folder '{folder_name}' is not a valid mineral identifier"
        ));
    }

    let legacy_root = data_root.join(LEGACY_MINERALS_DIR);
    let legacy_root_metadata = match fs::symlink_metadata(&legacy_root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect {}", legacy_root.display()))
        }
    };
    if legacy_root_metadata.file_type().is_symlink() || !legacy_root_metadata.is_dir() {
        return Err(anyhow!(
            "legacy minerals root {} must be a real directory, not a symlink",
            legacy_root.display()
        ));
    }

    let canonical_data_root = fs::canonicalize(data_root)
        .with_context(|| format!("failed to resolve {}", data_root.display()))?;
    let canonical_legacy_root = fs::canonicalize(&legacy_root)
        .with_context(|| format!("failed to resolve {}", legacy_root.display()))?;
    if !canonical_legacy_root.starts_with(&canonical_data_root) {
        return Err(anyhow!(
            "legacy minerals root {} escapes data root {}",
            canonical_legacy_root.display(),
            canonical_data_root.display()
        ));
    }

    let folder_path = legacy_root.join(folder_name);
    let folder_metadata = match fs::symlink_metadata(&folder_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect {}", folder_path.display()))
        }
    };
    if folder_metadata.file_type().is_symlink() || !folder_metadata.is_dir() {
        return Err(anyhow!(
            "legacy mineral folder {} must be a real directory, not a symlink",
            folder_path.display()
        ));
    }
    let canonical_folder = fs::canonicalize(&folder_path)
        .with_context(|| format!("failed to resolve {}", folder_path.display()))?;
    if !canonical_folder.starts_with(&canonical_legacy_root) {
        return Err(anyhow!(
            "legacy mineral folder {} escapes {}",
            canonical_folder.display(),
            canonical_legacy_root.display()
        ));
    }

    let preferred_file = localized
        .get(FALLBACK_LANGUAGE)
        .and_then(|value| value.image_file.as_deref())
        .or_else(|| {
            localized
                .values()
                .find_map(|value| value.image_file.as_deref())
        });

    let candidate_path = if let Some(name) = preferred_file {
        validate_legacy_image_file_name(name)?;
        let preferred_path = folder_path.join(name);
        match fs::symlink_metadata(&preferred_path) {
            Ok(_) => Some(preferred_path),
            Err(error) if error.kind() == ErrorKind::NotFound => None,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to inspect {}", preferred_path.display()))
            }
        }
    } else {
        None
    }
    .or_else(|| {
        fs::read_dir(&folder_path).ok().and_then(|iter| {
            iter.filter_map(std::result::Result::ok)
                .filter(|entry| {
                    entry
                        .file_type()
                        .map(|kind| kind.is_file() && !kind.is_symlink())
                        .unwrap_or(false)
                })
                .find(|entry| {
                    entry
                        .file_name()
                        .to_str()
                        .map(|name| name.starts_with("image."))
                        .unwrap_or(false)
                })
                .map(|entry| entry.path())
        })
    });

    let Some(path) = candidate_path else {
        return Ok(None);
    };

    let path_metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("failed to inspect legacy image {}", path.display()))?;
    if path_metadata.file_type().is_symlink() || !path_metadata.is_file() {
        return Err(anyhow!(
            "legacy image {} must be a regular file, not a symlink",
            path.display()
        ));
    }
    if path_metadata.len() > LEGACY_IMAGE_MAX_BYTES {
        return Err(anyhow!(
            "legacy image {} exceeds the {} byte limit",
            path.display(),
            LEGACY_IMAGE_MAX_BYTES
        ));
    }

    let canonical_path = fs::canonicalize(&path)
        .with_context(|| format!("failed to resolve legacy image {}", path.display()))?;
    if !canonical_path.starts_with(&canonical_folder) {
        return Err(anyhow!(
            "legacy image {} escapes mineral folder {}",
            canonical_path.display(),
            canonical_folder.display()
        ));
    }

    let ext = canonical_path
        .extension()
        .and_then(|value| value.to_str())
        .and_then(supported_image_extension)
        .ok_or_else(|| {
            anyhow!(
                "legacy image {} has an unsupported extension",
                canonical_path.display()
            )
        })?;

    let bytes = fs::read(&canonical_path)
        .with_context(|| format!("failed to read legacy image {}", canonical_path.display()))?;
    if !image_signature_matches(&bytes, ext) {
        return Err(anyhow!(
            "legacy image {} does not match its declared {ext} format",
            canonical_path.display()
        ));
    }

    let original_name = canonical_path
        .file_name()
        .and_then(|value| value.to_str())
        .map(str::to_string);

    Ok(Some(OwnedImageInput {
        bytes,
        ext: ext.to_string(),
        original_name,
    }))
}

fn validate_legacy_image_file_name(name: &str) -> Result<()> {
    let trimmed = name.trim();
    let mut components = Path::new(trimmed).components();
    let single_normal_component =
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none();
    if trimmed.is_empty()
        || trimmed != name
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || !single_normal_component
    {
        return Err(anyhow!(
            "legacy image_file '{name}' must be a single normal filename"
        ));
    }
    if Path::new(trimmed)
        .extension()
        .and_then(|value| value.to_str())
        .and_then(supported_image_extension)
        .is_none()
    {
        return Err(anyhow!(
            "legacy image_file '{name}' has an unsupported extension"
        ));
    }
    Ok(())
}

fn supported_image_extension(ext: &str) -> Option<&'static str> {
    match ext
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => Some("png"),
        "webp" => Some("webp"),
        "gif" => Some("gif"),
        "jpeg" | "jpg" => Some("jpg"),
        _ => None,
    }
}

fn image_signature_matches(bytes: &[u8], ext: &str) -> bool {
    match ext {
        "png" => bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]),
        "jpg" => bytes.starts_with(&[0xff, 0xd8, 0xff]),
        "gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        "webp" => bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP",
        _ => false,
    }
}

fn normalize_localized_records(
    localized_records: &HashMap<String, MineralDiskRecord>,
) -> Result<HashMap<String, MineralDiskRecord>> {
    let mut normalized = localized_records
        .iter()
        .map(|(lang, record)| (lang.trim().to_lowercase(), record.clone()))
        .filter(|(lang, _)| !lang.is_empty())
        .collect::<HashMap<_, _>>();

    if normalized.is_empty() {
        return Err(anyhow!("localized metadata cannot be empty"));
    }

    if !normalized.contains_key(FALLBACK_LANGUAGE) {
        let fallback = normalized
            .values()
            .next()
            .cloned()
            .ok_or_else(|| anyhow!("failed to derive fallback localized metadata"))?;
        normalized.insert(FALLBACK_LANGUAGE.to_string(), fallback);
    }

    Ok(normalized)
}

fn localized_metadata_to_json(values: &HashMap<String, MineralDiskRecord>) -> Result<String> {
    serde_json::to_string(values).context("failed to serialize localized metadata")
}

fn localized_metadata_from_json(raw: &str) -> Result<HashMap<String, MineralDiskRecord>> {
    if raw.trim().is_empty() {
        return Ok(HashMap::new());
    }

    serde_json::from_str(raw).with_context(|| "failed to parse localized metadata JSON")
}

fn major_elements_to_json(values: &BTreeMap<String, f32>) -> Result<String> {
    serde_json::to_string(values).context("failed to serialize major elements")
}

fn major_elements_from_json(raw: &str) -> Result<BTreeMap<String, f32>> {
    serde_json::from_str(raw).with_context(|| "failed to parse major elements JSON")
}

fn migrate_legacy_folder_records(data_root: &Path) -> Result<Vec<LocalizedMineralRecord>> {
    let minerals_root = data_root.join(LEGACY_MINERALS_DIR);
    if !minerals_root.exists() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    for entry in fs::read_dir(&minerals_root)
        .with_context(|| format!("failed to read {}", minerals_root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let folder_name = entry.file_name().to_string_lossy().to_string();
        if !is_valid_mineral_folder_name(&folder_name) {
            continue;
        }

        let localized = read_localized_folder_records(&path)?;
        for (lang_code, metadata) in localized {
            out.push(LocalizedMineralRecord {
                slug: folder_name.clone(),
                folder_name: folder_name.clone(),
                lang_code,
                metadata,
            });
        }
    }

    out.sort_by(|a, b| (&a.slug, &a.lang_code).cmp(&(&b.slug, &b.lang_code)));
    Ok(out)
}

fn read_localized_folder_records(folder: &Path) -> Result<HashMap<String, MineralDiskRecord>> {
    let mut localized = HashMap::new();
    let mut legacy_fallback: Option<MineralDiskRecord> = None;

    for entry in fs::read_dir(folder)
        .with_context(|| format!("failed to read folder {}", folder.display()))?
    {
        let entry = entry?;
        if !entry.path().is_file() {
            continue;
        }

        let file_name = entry.file_name().to_string_lossy().to_string();
        if file_name == "mineral.json" {
            let raw = fs::read_to_string(entry.path())
                .with_context(|| format!("failed to read {}", entry.path().display()))?;
            let record: MineralDiskRecord = serde_json::from_str(&raw)
                .with_context(|| format!("failed to parse {}", entry.path().display()))?;
            legacy_fallback = Some(record);
            continue;
        }

        let Some(lang_code) = language_code_from_file_name(&file_name) else {
            continue;
        };

        let raw = fs::read_to_string(entry.path())
            .with_context(|| format!("failed to read {}", entry.path().display()))?;
        let record: MineralDiskRecord = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse {}", entry.path().display()))?;
        localized.insert(lang_code, record);
    }

    if !localized.contains_key(FALLBACK_LANGUAGE) {
        if let Some(record) = legacy_fallback {
            localized.insert(FALLBACK_LANGUAGE.to_string(), record);
        } else if let Some(record) = localized.values().next().cloned() {
            localized.insert(FALLBACK_LANGUAGE.to_string(), record);
        }
    }

    Ok(localized)
}

fn language_code_from_file_name(file_name: &str) -> Option<String> {
    if !file_name.starts_with("mineral.") || !file_name.ends_with(".json") {
        return None;
    }

    let without_prefix = file_name.strip_prefix("mineral.")?;
    let code = without_prefix.strip_suffix(".json")?.trim();
    if code.is_empty() || code == "json" {
        return None;
    }

    Some(code.to_string())
}

fn normalize_image_extension(ext: &str) -> String {
    let normalized = ext.trim().trim_start_matches('.').to_ascii_lowercase();
    match normalized.as_str() {
        "png" => "png".to_string(),
        "webp" => "webp".to_string(),
        "gif" => "gif".to_string(),
        "jpeg" => "jpg".to_string(),
        "jpg" => "jpg".to_string(),
        _ => "jpg".to_string(),
    }
}

fn content_type_from_ext(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "image/jpeg",
    }
}

pub fn is_valid_mineral_folder_name(name: &str) -> bool {
    let mut parts = name.split('.');
    let prefix = parts.next();
    let family = parts.next();
    let id = parts.next();

    if prefix != Some("mineral") || family.is_none() || id.is_none() || parts.next().is_some() {
        return false;
    }

    let family = family.unwrap_or_default();
    let id = id.unwrap_or_default();
    if family.is_empty()
        || !family
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
        || family.starts_with('-')
        || family.ends_with('-')
        || !id.starts_with("0x")
        || id.len() < 5
    {
        return false;
    }

    id[2..].chars().all(|c| c.is_ascii_hexdigit())
}

pub fn parse_major_elements(raw: &str) -> Result<BTreeMap<String, f32>, String> {
    let mut values = BTreeMap::new();
    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let separator = if line.contains('=') { '=' } else { ':' };
        let mut parts = line.splitn(2, separator);
        let key = parts.next().unwrap_or_default().trim();
        let value = parts.next().unwrap_or_default().trim();

        if key.is_empty() || value.is_empty() {
            return Err("major_elements_pct lines must be like 'Si=46.7'".to_string());
        }
        if key.len() > 16 || !key.chars().all(|ch| ch.is_ascii_alphanumeric()) {
            return Err(format!("invalid element symbol '{key}'"));
        }

        let parsed = value
            .parse::<f32>()
            .map_err(|_| format!("invalid percentage for '{key}'"))?;
        if !parsed.is_finite() || !(0.0..=100.0).contains(&parsed) {
            return Err(format!("percentage for '{key}' must be between 0 and 100"));
        }
        values.insert(key.to_string(), parsed);
        if values.len() > 64 {
            return Err("major_elements_pct contains too many entries".to_string());
        }
    }
    let total: f32 = values.values().sum();
    if total > 100.5 {
        return Err(format!(
            "major element percentages total {total:.2}, above 100"
        ));
    }
    Ok(values)
}

pub fn major_elements_to_text(values: &BTreeMap<String, f32>) -> String {
    values
        .iter()
        .map(|(name, value)| format!("{name}={value:.2}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_record(image_file: Option<&str>) -> MineralDiskRecord {
        MineralDiskRecord {
            common_name: "Quartz".to_string(),
            description: String::new(),
            mineral_family: "silicates".to_string(),
            formula: "SiO2".to_string(),
            hardness_mohs: 7.0,
            density_g_cm3: 2.65,
            crystal_system: "trigonal".to_string(),
            color: "colorless".to_string(),
            streak: "white".to_string(),
            luster: "vitreous".to_string(),
            major_elements_pct: BTreeMap::new(),
            notes: String::new(),
            image_file: image_file.map(str::to_string),
        }
    }

    #[test]
    fn admin_sql_interrupts_queries_that_exceed_the_runtime_limit() {
        let temp = tempfile::tempdir().expect("tempdir");
        init_minerals_database(temp.path()).expect("initialize database");

        let started = Instant::now();
        let error = execute_admin_sql_with_runtime(
            temp.path(),
            "WITH RECURSIVE forever(value) AS (VALUES(1) UNION ALL SELECT value + 1 FROM forever) SELECT max(value) FROM forever",
            Duration::from_millis(10),
        )
        .expect_err("recursive query must be interrupted");

        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(format!("{error:#}")
            .to_ascii_lowercase()
            .contains("interrupt"));
    }

    #[test]
    fn legacy_image_reference_must_be_a_normal_filename() {
        for invalid in [
            "../secret.jpg",
            "..\\secret.jpg",
            "/secret.jpg",
            "image.jpg/child",
            " image.jpg",
            "image.txt",
        ] {
            let error = validate_legacy_image_file_name(invalid)
                .expect_err("traversing or unsupported image name must fail");
            assert!(!error.to_string().is_empty());
        }
        validate_legacy_image_file_name("image.JPEG").expect("supported filename");
    }

    #[test]
    fn mineral_identifier_rejects_path_characters() {
        assert!(is_valid_mineral_folder_name("mineral.iron-oxides.0x1234"));
        assert!(!is_valid_mineral_folder_name("mineral.iron/oxides.0x1234"));
        assert!(!is_valid_mineral_folder_name("mineral.iron\\oxides.0x1234"));
        assert!(!is_valid_mineral_folder_name("mineral.-iron.0x1234"));
    }

    #[test]
    fn legacy_image_resolution_rejects_traversal_before_reading() {
        let temp = tempfile::tempdir().expect("tempdir");
        let folder_name = "mineral.silicates.0x1234";
        let folder = temp.path().join(LEGACY_MINERALS_DIR).join(folder_name);
        fs::create_dir_all(&folder).expect("legacy folder");
        fs::write(temp.path().join("secret.jpg"), [0xff, 0xd8, 0xff]).expect("outside file");
        let localized = HashMap::from([(
            FALLBACK_LANGUAGE.to_string(),
            legacy_record(Some("../../secret.jpg")),
        )]);

        let error = resolve_legacy_image_for_slug(temp.path(), folder_name, &localized)
            .expect_err("traversal must fail");
        assert!(error.to_string().contains("single normal filename"));
    }

    #[test]
    fn valid_legacy_seed_still_migrates_with_a_registered_image() {
        let temp = tempfile::tempdir().expect("tempdir");
        let folder_name = "mineral.silicates.0x1234";
        let folder = temp.path().join(LEGACY_MINERALS_DIR).join(folder_name);
        fs::create_dir_all(&folder).expect("legacy folder");
        fs::write(folder.join("image.jpg"), [0xff, 0xd8, 0xff, 0xd9]).expect("legacy image");
        fs::write(
            folder.join("mineral.en.json"),
            serde_json::to_vec(&legacy_record(Some("image.jpg"))).expect("metadata JSON"),
        )
        .expect("legacy metadata");

        init_minerals_database(temp.path()).expect("migrate valid legacy seed");
        let minerals = load_minerals(temp.path(), FALLBACK_LANGUAGE).expect("load catalog");
        assert_eq!(minerals.len(), 1);
        assert_eq!(minerals[0].slug, folder_name);
        assert!(minerals[0]
            .image_path
            .as_deref()
            .is_some_and(|path| path.starts_with("/media/images/")));
    }

    #[test]
    fn image_storage_rejects_a_traversing_slug() {
        let temp = tempfile::tempdir().expect("tempdir");
        let error = store_image_file(
            temp.path(),
            "../../escape",
            "jpg",
            &[0xff, 0xd8, 0xff],
            Some("image.jpg"),
        )
        .expect_err("invalid slug must fail");
        assert!(error.to_string().contains("invalid mineral slug"));
        assert!(!temp.path().join("escape").exists());
    }

    #[test]
    fn image_directory_must_remain_inside_the_data_root() {
        let data_root = tempfile::tempdir().expect("data root");
        let outside = tempfile::tempdir().expect("outside directory");
        let error =
            ensure_real_directory_within(data_root.path(), outside.path(), "images directory")
                .expect_err("outside image directory must fail");
        assert!(error.to_string().contains("escapes data root"));
    }

    #[cfg(unix)]
    #[test]
    fn initialization_rejects_a_symlinked_database_before_opening_it() {
        use std::os::unix::fs::symlink;

        let data_root = tempfile::tempdir().expect("data root");
        let outside = tempfile::NamedTempFile::new().expect("outside database target");
        fs::write(outside.path(), b"do not modify").expect("marker");
        symlink(outside.path(), data_root.path().join(MINERALS_DB_FILE)).expect("database symlink");

        let error = init_minerals_database(data_root.path())
            .expect_err("symlinked database must be rejected");
        assert!(error.to_string().contains("regular file"));
        assert_eq!(
            fs::read(outside.path()).expect("marker remains"),
            b"do not modify"
        );
    }

    #[cfg(unix)]
    #[test]
    fn initialization_applies_private_unix_permissions() {
        let data_root = tempfile::tempdir().expect("data root");
        init_minerals_database(data_root.path()).expect("initialize database");

        let root_mode = fs::metadata(data_root.path())
            .expect("root metadata")
            .permissions()
            .mode()
            & 0o777;
        let images_mode = fs::metadata(data_root.path().join(IMAGES_DIR))
            .expect("images metadata")
            .permissions()
            .mode()
            & 0o777;
        let database_mode = fs::metadata(data_root.path().join(MINERALS_DB_FILE))
            .expect("database metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(root_mode, 0o700);
        assert_eq!(images_mode, 0o700);
        assert_eq!(database_mode, 0o600);
    }
}
