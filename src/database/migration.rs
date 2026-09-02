use rusqlite::{OptionalExtension, params};

use super::{Database, DatabaseError};

#[derive(Debug, Clone)]
pub struct MigrationJob {
    pub id: i64,
    pub source_kind: String,
    pub status: String,
    pub phase: String,
    pub destination_url: String,
    pub destination_drive_name: String,
    pub destination_folder_name: String,
    pub files_total: u64,
    pub folders_total: u64,
    pub bytes_total: u64,
    pub files_copied: u64,
    pub bytes_copied: u64,
    pub exceptions_count: u64,
    pub created_at: String,
    pub started_at: String,
    pub completed_at: String,
    pub error_message: String,
    pub sources: Vec<MigrationSource>,
}

#[derive(Debug, Clone)]
pub struct MigrationSource {
    pub item_id: String,
    pub name: String,
    pub relative_path: String,
    pub is_directory: bool,
    pub files_total: u64,
    pub folders_total: u64,
    pub bytes_total: u64,
}

pub fn create(
    database: &Database,
    source_scope: &str,
    source_kind: &str,
    item_ids: &[String],
) -> Result<i64, DatabaseError> {
    if !matches!(source_kind, "my-drive" | "shared-with-me") || item_ids.is_empty() {
        return Err("Select at least one My Drive or Shared with Me item".into());
    }
    let mut connection = database.connect()?;
    let transaction = connection.transaction()?;
    transaction.execute(
        "INSERT INTO migration_jobs (source_scope, source_kind) VALUES (?1, ?2)",
        params![source_scope, source_kind],
    )?;
    let migration_id = transaction.last_insert_rowid();
    for item_id in item_ids {
        let source = transaction
            .query_row(
                "SELECT item_id, name, relative_path, is_directory
                 FROM drive_items
                 WHERE remote_name = ?1 AND item_id = ?2 AND is_deleted = 0",
                params![source_scope, item_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, bool>(3)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| format!("Unknown or deleted migration source: {item_id}"))?;
        let pattern = format!("{}/%", source.2);
        let (files, folders, bytes): (i64, i64, i64) = transaction.query_row(
            "SELECT
                SUM(CASE WHEN is_directory = 0 THEN 1 ELSE 0 END),
                SUM(CASE WHEN is_directory = 1 THEN 1 ELSE 0 END),
                COALESCE(SUM(CASE WHEN is_directory = 0 THEN COALESCE(size_bytes, 0) ELSE 0 END), 0)
             FROM drive_items
             WHERE remote_name = ?1 AND is_deleted = 0
               AND (item_id = ?2 OR (?3 = 1 AND relative_path LIKE ?4))",
            params![source_scope, item_id, source.3, pattern],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        transaction.execute(
            "INSERT INTO migration_sources
             (migration_id, item_id, name, relative_path, is_directory, files_total, folders_total, bytes_total)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![migration_id, source.0, source.1, source.2, source.3, files, folders, bytes],
        )?;
    }
    transaction.execute(
        "UPDATE migration_jobs SET
            files_total = (SELECT COALESCE(SUM(files_total), 0) FROM migration_sources WHERE migration_id = ?1),
            folders_total = (SELECT COALESCE(SUM(folders_total), 0) FROM migration_sources WHERE migration_id = ?1),
            bytes_total = (SELECT COALESCE(SUM(bytes_total), 0) FROM migration_sources WHERE migration_id = ?1)
         WHERE id = ?1",
        [migration_id],
    )?;
    transaction.commit()?;
    Ok(migration_id)
}

pub fn list(database: &Database) -> Result<Vec<MigrationJob>, DatabaseError> {
    let connection = database.connect()?;
    let mut statement = connection.prepare(
        "SELECT id, source_kind, status, phase, destination_url, destination_drive_name,
                destination_folder_name, files_total, folders_total, bytes_total,
                files_copied, bytes_copied, exceptions_count, created_at,
                COALESCE(started_at, ''), COALESCE(completed_at, ''), error_message
         FROM migration_jobs ORDER BY created_at DESC, id DESC",
    )?;
    let jobs = statement
        .query_map([], job_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(jobs)
}

pub fn get(database: &Database, id: i64) -> Result<Option<MigrationJob>, DatabaseError> {
    let connection = database.connect()?;
    let mut job = connection
        .query_row(
            "SELECT id, source_kind, status, phase, destination_url, destination_drive_name,
                    destination_folder_name, files_total, folders_total, bytes_total,
                    files_copied, bytes_copied, exceptions_count, created_at,
                    COALESCE(started_at, ''), COALESCE(completed_at, ''), error_message
             FROM migration_jobs WHERE id = ?1",
            [id],
            job_from_row,
        )
        .optional()?;
    if let Some(job) = &mut job {
        let mut statement = connection.prepare(
            "SELECT item_id, name, relative_path, is_directory, files_total, folders_total, bytes_total
             FROM migration_sources WHERE migration_id = ?1 ORDER BY name COLLATE NOCASE",
        )?;
        job.sources = statement
            .query_map([id], |row| {
                Ok(MigrationSource {
                    item_id: row.get(0)?,
                    name: row.get(1)?,
                    relative_path: row.get(2)?,
                    is_directory: row.get(3)?,
                    files_total: row.get::<_, i64>(4)? as u64,
                    folders_total: row.get::<_, i64>(5)? as u64,
                    bytes_total: row.get::<_, i64>(6)? as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
    }
    Ok(job)
}

pub fn set_destination(
    database: &Database,
    id: i64,
    url: &str,
    drive_id: &str,
    drive_name: &str,
    folder_id: &str,
    folder_name: &str,
) -> Result<(), DatabaseError> {
    let connection = database.connect()?;
    let changed = connection.execute(
        "UPDATE migration_jobs SET destination_url = ?2, destination_drive_id = ?3,
            destination_drive_name = ?4, destination_folder_id = ?5,
            destination_folder_name = ?6, status = 'ready', phase = 'Ready for copy authorization',
            updated_at = CURRENT_TIMESTAMP, error_message = '' WHERE id = ?1",
        params![id, url, drive_id, drive_name, folder_id, folder_name],
    )?;
    if changed == 0 {
        return Err(format!("Unknown migration: {id}").into());
    }
    Ok(())
}

pub fn resolve_destination(
    database: &Database,
    folder_id: &str,
) -> Result<Option<(String, String, String)>, DatabaseError> {
    let connection = database.connect()?;
    connection
        .query_row(
            "SELECT sd.drive_id, sd.name, sd.name FROM shared_drives sd WHERE sd.drive_id = ?1
         UNION ALL
         SELECT sd.drive_id, sd.name, di.name FROM drive_items di
         JOIN shared_drives sd ON sd.inventory_scope = di.remote_name
         WHERE di.item_id = ?1 AND di.is_directory = 1 AND di.is_deleted = 0 LIMIT 1",
            [folder_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(Into::into)
}

fn job_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MigrationJob> {
    Ok(MigrationJob {
        id: row.get(0)?,
        source_kind: row.get(1)?,
        status: row.get(2)?,
        phase: row.get(3)?,
        destination_url: row.get(4)?,
        destination_drive_name: row.get(5)?,
        destination_folder_name: row.get(6)?,
        files_total: row.get::<_, i64>(7)? as u64,
        folders_total: row.get::<_, i64>(8)? as u64,
        bytes_total: row.get::<_, i64>(9)? as u64,
        files_copied: row.get::<_, i64>(10)? as u64,
        bytes_copied: row.get::<_, i64>(11)? as u64,
        exceptions_count: row.get::<_, i64>(12)? as u64,
        created_at: row.get(13)?,
        started_at: row.get(14)?,
        completed_at: row.get(15)?,
        error_message: row.get(16)?,
        sources: Vec::new(),
    })
}
