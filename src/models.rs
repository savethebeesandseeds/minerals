use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Context, Result};
use rusqlite::{params, types::ValueRef, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

const MINERALS_DB_FILE: &str = "minerals.db";
const LEGACY_JSON_DB_FILE: &str = "minerals.db.json";
const LEGACY_MINERALS_DIR: &str = "minerals";
const IMAGES_DIR: &str = "images";
const FALLBACK_LANGUAGE: &str = "en";
const METADATA_SCHEMA_VERSION: i64 = 1;
const ADMIN_SQL_MAX_ROWS: usize = 500;
const ADMIN_SQL_MAX_LENGTH: usize = 100_000;
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
    fs::create_dir_all(data_root.join(IMAGES_DIR))
        .with_context(|| format!("failed to create {}", data_root.join(IMAGES_DIR).display()))?;

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

    Ok(())
}

pub fn load_minerals(data_root: &Path, lang_code: &str) -> Result<Vec<Mineral>> {
    let conn = open_connection(data_root)?;
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
            image_path: selected_image.map(|value| format!("/data/{IMAGES_DIR}/{value}")),
        });
    }

    minerals.sort_by(|a, b| a.common_name.cmp(&b.common_name));
    Ok(minerals)
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
        upsert_catalog_bundle(
            &tx,
            data_root,
            slug,
            folder_name,
            &normalized,
            Some(&image_input),
        )?;
        tx.commit().context("failed to commit save transaction")?;
    }

    prune_orphan_images(&conn, data_root)?;
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

    prune_orphan_images(&conn, data_root)?;
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
    let mut stmt = conn
        .prepare(trimmed)
        .context("failed to prepare SQL statement")?;

    let column_names = stmt
        .column_names()
        .iter()
        .map(|name| name.to_string())
        .collect::<Vec<_>>();

    if !column_names.is_empty() {
        let mut rows = Vec::new();
        let mut truncated = false;
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
                out_row.push(sql_value_to_string(value));
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

    let affected_rows = stmt
        .execute([])
        .context("failed to execute SQL statement")?;
    drop(stmt);

    prune_orphan_images(&conn, data_root)?;

    Ok(AdminSqlExecution {
        statement_type,
        columns: Vec::new(),
        rows: Vec::new(),
        row_count: 0,
        affected_rows,
        truncated: false,
    })
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
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .context("failed to enable sqlite foreign keys")?;
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

    for (slug, group) in grouped {
        let image = match group.image_name {
            Some(stored_name) => resolve_shared_image_by_name(data_root, &stored_name)?,
            None => None,
        };

        upsert_catalog_bundle(
            &tx,
            data_root,
            &slug,
            &group.folder_name,
            &group.localized,
            image.as_ref(),
        )?;
    }

    tx.commit()
        .context("failed to commit legacy SQL migration transaction")?;

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
        let entry = grouped
            .entry(row.slug.clone())
            .or_insert_with(|| (row.folder_name.clone(), HashMap::new()));
        entry.0 = row.folder_name.clone();
        entry.1.insert(row.lang_code.clone(), row.metadata.clone());
    }

    let tx = conn
        .transaction()
        .context("failed to open migration transaction")?;

    for (slug, (folder_name, localized)) in grouped {
        let image = resolve_legacy_image_for_slug(data_root, &folder_name, &localized)?;
        upsert_catalog_bundle(
            &tx,
            data_root,
            &slug,
            &folder_name,
            &localized,
            image.as_ref(),
        )?;
    }

    tx.commit()
        .context("failed to commit migration transaction")?;
    Ok(())
}

fn upsert_catalog_bundle(
    tx: &Transaction<'_>,
    data_root: &Path,
    slug: &str,
    folder_name: &str,
    localized_records: &HashMap<String, MineralDiskRecord>,
    image: Option<&OwnedImageInput>,
) -> Result<()> {
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

    if let Some(image_input) = image {
        let stored_image = store_image_file(
            data_root,
            slug,
            &image_input.ext,
            &image_input.bytes,
            image_input.original_name.as_deref(),
        )?;

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
    }

    Ok(())
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
        conn.execute("DELETE FROM images WHERE id = ?1", params![image_id])
            .with_context(|| format!("failed to delete orphan image id {image_id}"))?;

        let image_path = data_root.join(IMAGES_DIR).join(&stored_name);
        if image_path.exists() {
            fs::remove_file(&image_path)
                .with_context(|| format!("failed to delete {}", image_path.display()))?;
        }
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
    let normalized_ext = normalize_image_extension(ext);
    let images_root = data_root.join(IMAGES_DIR);
    fs::create_dir_all(&images_root)
        .with_context(|| format!("failed to create {}", images_root.display()))?;

    for attempt in 0..64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let candidate = format!("{slug}.{now:x}.{attempt}.{normalized_ext}");
        let target = images_root.join(&candidate);
        if target.exists() {
            continue;
        }

        fs::write(&target, bytes)
            .with_context(|| format!("failed to write image {}", target.display()))?;

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
    let folder_path = data_root.join(LEGACY_MINERALS_DIR).join(folder_name);
    if !folder_path.exists() {
        return Ok(None);
    }

    let preferred_file = localized
        .get(FALLBACK_LANGUAGE)
        .and_then(|value| value.image_file.clone())
        .or_else(|| {
            localized
                .values()
                .find_map(|value| value.image_file.clone())
        });

    let candidate_path = preferred_file
        .as_ref()
        .map(|name| folder_path.join(name))
        .filter(|path| path.exists())
        .or_else(|| {
            fs::read_dir(&folder_path).ok().and_then(|iter| {
                iter.filter_map(std::result::Result::ok)
                    .map(|entry| entry.path())
                    .find(|path| {
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .map(|name| name.starts_with("image."))
                            .unwrap_or(false)
                    })
            })
        });

    let Some(path) = candidate_path else {
        return Ok(None);
    };

    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("jpg")
        .to_string();

    let bytes = fs::read(&path)
        .with_context(|| format!("failed to read legacy image {}", path.display()))?;

    let original_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .map(str::to_string);

    Ok(Some(OwnedImageInput {
        bytes,
        ext,
        original_name,
    }))
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
    if family.is_empty() || !id.starts_with("0x") || id.len() < 5 {
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

        let parsed = value
            .parse::<f32>()
            .map_err(|_| format!("invalid percentage for '{key}'"))?;
        values.insert(key.to_string(), parsed);
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
