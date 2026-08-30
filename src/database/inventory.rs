use std::collections::HashSet;

use rusqlite::{params, OptionalExtension};
use serde_json::Value;

use crate::rclone::{inventory::DriveItem, remotes::RemoteKind};

use super::{Database, DatabaseError};

#[derive(Debug, Clone, Default)]
pub struct InventorySummary {
    pub completed_at: String,
    pub files_scanned: u64,
    pub folders_scanned: u64,
    pub permissions_scanned: u64,
    pub bytes_discovered: u64,
    pub deleted_items: u64,
}

#[derive(Debug, Clone)]
pub struct DriveExplorerItem {
    pub item_id: String,
    pub name: String,
    pub relative_path: String,
    pub is_directory: bool,
    pub mime_type: Option<String>,
    pub size_bytes: Option<u64>,
    pub modified_at: Option<String>,
    pub owner_email: Option<String>,
}

pub fn list_my_drive_directory(
    database: &Database,
    parent_path: Option<&str>,
) -> Result<Vec<DriveExplorerItem>, DatabaseError> {
    let connection = database.connect()?;
    let remote = RemoteKind::MyDriveRo.name();
    let mut statement = connection.prepare(
        "SELECT item_id, name, relative_path, is_directory, mime_type,
                size_bytes, modified_at, owner_email
         FROM drive_items
         WHERE remote_name = ?1
           AND is_deleted = 0
           AND ((?2 IS NULL AND parent_path IS NULL) OR parent_path = ?2)
         ORDER BY is_directory DESC, name COLLATE NOCASE, item_id",
    )?;
    let rows = statement.query_map(
        params![remote, parent_path],
        |row| {
            let size: Option<i64> = row.get(5)?;
            Ok(DriveExplorerItem {
                item_id: row.get(0)?,
                name: row.get(1)?,
                relative_path: row.get(2)?,
                is_directory: row.get(3)?,
                mime_type: row.get(4)?,
                size_bytes: size.map(|value| value as u64),
                modified_at: row.get(6)?,
                owner_email: row.get(7)?,
            })
        },
    )?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn synchronize_my_drive(
    database: &Database,
    scan_id: i64,
    items: &[DriveItem],
    include_permissions: bool,
) -> Result<InventorySummary, DatabaseError> {
    let mut connection = database.connect()?;
    let transaction = connection.transaction()?;
    let remote = RemoteKind::MyDriveRo.name();
    let mut summary = InventorySummary::default();
    let mut seen_item_ids = HashSet::new();

    for item in items {
        // Rclone may expose the same Drive object through multiple paths,
        // particularly shortcuts and legacy multi-parent objects. Drive ID is
        // authoritative, so count and store each object only once per scan.
        if !seen_item_ids.insert(item.id.as_str()) {
            continue;
        }

        let size = (item.size >= 0).then_some(item.size);
        let owner = item.metadata.get("owner").map(String::as_str);
        let created = item.metadata.get("btime").map(String::as_str);
        let parent_path = item.path.rsplit_once('/').map(|(parent, _)| parent);
        let metadata_json = serde_json::to_string(&item.metadata)?;

        transaction.execute(
            "INSERT INTO drive_items (
                remote_name, item_id, name, relative_path, parent_path,
                is_directory, mime_type, size_bytes, modified_at, created_at,
                owner_email, metadata_json, last_seen_scan_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(remote_name, item_id) DO UPDATE SET
                name = excluded.name,
                relative_path = excluded.relative_path,
                parent_path = excluded.parent_path,
                is_directory = excluded.is_directory,
                mime_type = excluded.mime_type,
                size_bytes = excluded.size_bytes,
                modified_at = excluded.modified_at,
                created_at = excluded.created_at,
                owner_email = excluded.owner_email,
                metadata_json = excluded.metadata_json,
                last_seen_at = CURRENT_TIMESTAMP,
                last_seen_scan_id = excluded.last_seen_scan_id,
                is_deleted = 0,
                deleted_at = NULL",
            params![
                remote, item.id, item.name, item.path, parent_path,
                item.is_dir, empty_as_none(&item.mime_type), size,
                empty_as_none(&item.mod_time), created, owner, metadata_json, scan_id,
            ],
        )?;

        if include_permissions {
            transaction.execute(
                "DELETE FROM drive_permissions WHERE remote_name = ?1 AND item_id = ?2",
                params![remote, item.id],
            )?;

            for permission in permissions(item)? {
                let key = permission_key(&permission);
                transaction.execute(
                "INSERT INTO drive_permissions (
                    remote_name, item_id, permission_key, permission_id,
                    permission_type, role, email_address, display_name,
                    domain, raw_json, last_seen_scan_id
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    remote, item.id, key,
                    field(&permission, "id"), field(&permission, "type"),
                    field(&permission, "role"), field(&permission, "emailAddress"),
                    field(&permission, "displayName"), field(&permission, "domain"),
                    permission.to_string(), scan_id,
                ],
                )?;
                summary.permissions_scanned += 1;
            }
        }

        if item.is_dir {
            summary.folders_scanned += 1;
        } else {
            summary.files_scanned += 1;
            summary.bytes_discovered = summary.bytes_discovered.saturating_add(size.unwrap_or(0) as u64);
        }
    }

    summary.deleted_items = transaction.execute(
        "UPDATE drive_items
         SET is_deleted = 1,
             deleted_at = COALESCE(deleted_at, CURRENT_TIMESTAMP)
         WHERE remote_name = ?1
           AND last_seen_scan_id <> ?2
           AND is_deleted = 0",
        params![remote, scan_id],
    )? as u64;

    transaction.execute(
        "UPDATE scan_runs SET
            status = 'complete', completed_at = CURRENT_TIMESTAMP, error_message = NULL,
            files_scanned = ?2, folders_scanned = ?3, permissions_scanned = ?4,
            bytes_discovered = ?5, deleted_items = ?6
         WHERE id = ?1",
        params![
            scan_id, summary.files_scanned as i64, summary.folders_scanned as i64,
            summary.permissions_scanned as i64, summary.bytes_discovered as i64,
            summary.deleted_items as i64,
        ],
    )?;
    summary.completed_at = transaction.query_row(
        "SELECT completed_at FROM scan_runs WHERE id = ?1",
        [scan_id],
        |row| row.get(0),
    )?;

    transaction.commit()?;
    Ok(summary)
}

pub fn latest_summary(database: &Database) -> Result<Option<InventorySummary>, DatabaseError> {
    let connection = database.connect()?;
    connection.query_row(
        "SELECT completed_at, files_scanned, folders_scanned,
                permissions_scanned, bytes_discovered, deleted_items
         FROM scan_runs
         WHERE scan_type = 'my-drive' AND status = 'complete'
         ORDER BY id DESC LIMIT 1",
        [],
        |row| Ok(InventorySummary {
            completed_at: row.get(0)?,
            files_scanned: row.get::<_, i64>(1)? as u64,
            folders_scanned: row.get::<_, i64>(2)? as u64,
            permissions_scanned: row.get::<_, i64>(3)? as u64,
            bytes_discovered: row.get::<_, i64>(4)? as u64,
            deleted_items: row.get::<_, i64>(5)? as u64,
        }),
    ).optional().map_err(Into::into)
}

fn permissions(item: &DriveItem) -> Result<Vec<Value>, DatabaseError> {
    let Some(raw) = item.metadata.get("permissions") else { return Ok(Vec::new()); };
    let value: Value = serde_json::from_str(raw)
        .map_err(|error| format!("Invalid permissions metadata in the rclone response: {error}"))?;
    Ok(match value {
        Value::Array(values) => values,
        Value::Null => Vec::new(),
        value => vec![value],
    })
}

fn field<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value.get(name).and_then(Value::as_str)
}

fn permission_key(value: &Value) -> String {
    if let Some(id) = field(value, "id") { return format!("id:{id}"); }
    ["type", "role", "emailAddress", "domain"].iter()
        .filter_map(|name| field(value, name).map(|part| format!("{name}:{part}")))
        .collect::<Vec<_>>()
        .join("|")
        .pipe_nonempty()
        .unwrap_or_else(|| value.to_string())
}

trait Nonempty { fn pipe_nonempty(self) -> Option<Self> where Self: Sized; }
impl Nonempty for String {
    fn pipe_nonempty(self) -> Option<Self> { if self.is_empty() { None } else { Some(self) } }
}

fn empty_as_none(value: &str) -> Option<&str> {
    (!value.is_empty()).then_some(value)
}
