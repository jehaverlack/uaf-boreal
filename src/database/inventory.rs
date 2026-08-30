use std::collections::{HashMap, HashSet};

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
    pub tags: Vec<Tag>,
    pub permissions: Vec<String>,
    pub is_deleted: bool,
}

#[derive(Debug, Clone)]
pub struct Tag {
    pub slug: String,
    pub name: String,
    pub color: String,
}

pub fn list_my_drive_directory(
    database: &Database,
    parent_path: Option<&str>,
    search: &str,
    tag_filter: &str,
    type_filter: &str,
    size_filter: &str,
    modified_filter: &str,
    owner_filter: &str,
    exclude_owner: bool,
    permission_filter: &str,
    include_deleted: bool,
    sort: &str,
    descending: bool,
) -> Result<Vec<DriveExplorerItem>, DatabaseError> {
    let connection = database.connect()?;
    let (size_comparison, size_bytes) = parse_size_filter(size_filter)?;
    let (modified_comparison, modified_value) = parse_modified_filter(modified_filter)?;
    let remote = RemoteKind::MyDriveRo.name();
    let sort_expression = match sort {
        "type" => "CASE WHEN is_directory THEN 'folder' ELSE COALESCE(mime_type, '') END COLLATE NOCASE",
        "size" => "COALESCE(CASE WHEN is_directory THEN cumulative_size_bytes ELSE size_bytes END, -1)",
        "modified" => "COALESCE(modified_at, '')",
        "owner" => "COALESCE(owner_email, '') COLLATE NOCASE",
        _ => "name COLLATE NOCASE",
    };
    let direction = if descending { "DESC" } else { "ASC" };
    let directory_grouping = if sort == "name" { "is_directory DESC," } else { "" };
    let sql = format!(
        "SELECT item_id, name, relative_path, is_directory, mime_type,
                CASE WHEN is_directory THEN cumulative_size_bytes ELSE size_bytes END,
                modified_at, owner_email,
                (SELECT group_concat(t.name || char(30) || t.color, char(31))
                 FROM drive_item_tags dit JOIN tags t ON t.id = dit.tag_id
                 WHERE dit.remote_name = drive_items.remote_name
                   AND dit.item_id = drive_items.item_id),
                (SELECT group_concat(label, char(31)) FROM (
                    SELECT DISTINCT COALESCE(
                        NULLIF(p.email_address, ''), NULLIF(p.domain, ''),
                        NULLIF(p.display_name, ''), NULLIF(p.permission_type, ''), 'Unknown'
                    ) AS label
                    FROM drive_permissions p
                    WHERE p.remote_name = drive_items.remote_name
                      AND p.item_id = drive_items.item_id
                      AND COALESCE(p.email_address, '') <> COALESCE(drive_items.owner_email, '')
                    ORDER BY label COLLATE NOCASE
                )), is_deleted
         FROM drive_items
         WHERE remote_name = ?1
           AND (?13 = 1 OR is_deleted = 0)
           AND ((?2 IS NULL AND parent_path IS NULL) OR parent_path = ?2)
           AND (?3 = '' OR instr(lower(name), lower(?3)) > 0)
           AND (?4 = '' OR EXISTS (
                SELECT 1 FROM drive_item_tags dit JOIN tags t ON t.id = dit.tag_id
                WHERE dit.remote_name = drive_items.remote_name
                  AND dit.item_id = drive_items.item_id AND t.slug = ?4
           ))
           AND (?5 = '' OR instr(lower(
                CASE WHEN is_directory THEN 'folder' ELSE COALESCE(mime_type, '') END
           ), lower(?5)) > 0)
           AND (?6 = 0 OR
                (?6 = 1 AND COALESCE(CASE WHEN is_directory THEN cumulative_size_bytes ELSE size_bytes END, 0) > ?7) OR
                (?6 = 2 AND COALESCE(CASE WHEN is_directory THEN cumulative_size_bytes ELSE size_bytes END, 0) >= ?7) OR
                (?6 = 3 AND COALESCE(CASE WHEN is_directory THEN cumulative_size_bytes ELSE size_bytes END, 0) < ?7) OR
                (?6 = 4 AND COALESCE(CASE WHEN is_directory THEN cumulative_size_bytes ELSE size_bytes END, 0) <= ?7) OR
                (?6 = 5 AND COALESCE(CASE WHEN is_directory THEN cumulative_size_bytes ELSE size_bytes END, 0) = ?7))
           AND (?8 = 0 OR
                (?8 = 1 AND substr(COALESCE(modified_at, ''), 1, 10) > ?9) OR
                (?8 = 2 AND substr(COALESCE(modified_at, ''), 1, 10) >= ?9) OR
                (?8 = 3 AND substr(COALESCE(modified_at, ''), 1, 10) < ?9) OR
                (?8 = 4 AND substr(COALESCE(modified_at, ''), 1, 10) <= ?9) OR
                (?8 = 5 AND substr(COALESCE(modified_at, ''), 1, length(?9)) = ?9))
           AND (?10 = '' OR
                (?11 = 0 AND instr(lower(COALESCE(owner_email, '')), lower(?10)) > 0) OR
                (?11 = 1 AND instr(lower(COALESCE(owner_email, '')), lower(?10)) = 0))
           AND (?12 = '' OR EXISTS (
                SELECT 1 FROM drive_permissions permission_filter
                WHERE permission_filter.remote_name = drive_items.remote_name
                  AND permission_filter.item_id = drive_items.item_id
                  AND instr(lower(
                      COALESCE(permission_filter.email_address, '') || ' ' ||
                      COALESCE(permission_filter.domain, '') || ' ' ||
                      COALESCE(permission_filter.display_name, '') || ' ' ||
                      COALESCE(permission_filter.permission_type, '')
                  ), lower(?12)) > 0
           ))
         ORDER BY {directory_grouping} {sort_expression} {direction}, name COLLATE NOCASE, item_id"
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(
        params![
            remote, parent_path, search.trim(), tag_filter, type_filter.trim(),
            size_comparison, size_bytes, modified_comparison, modified_value,
            owner_filter.trim(), exclude_owner, permission_filter.trim(), include_deleted,
        ],
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
                tags: row.get::<_, Option<String>>(8)?
                    .map(|tags| tags.split('\u{1f}').filter_map(|tag| {
                        let (name, color) = tag.split_once('\u{1e}')?;
                        Some(Tag { slug: String::new(), name: name.to_string(), color: color.to_string() })
                    }).collect())
                    .unwrap_or_default(),
                permissions: row.get::<_, Option<String>>(9)?
                    .map(|permissions| permissions.split('\u{1f}').map(str::to_string).collect())
                    .unwrap_or_default(),
                is_deleted: row.get(10)?,
            })
        },
    )?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn comparison_prefix(value: &str) -> (i64, &str) {
    for (prefix, comparison) in [(">=", 2), ("<=", 4), (">", 1), ("<", 3), ("=", 5)] {
        if let Some(rest) = value.strip_prefix(prefix) {
            return (comparison, rest.trim());
        }
    }
    (5, value.trim())
}

fn parse_size_filter(filter: &str) -> Result<(i64, i64), DatabaseError> {
    let filter = filter.trim();
    if filter.is_empty() {
        return Ok((0, 0));
    }
    let (mut comparison, value) = comparison_prefix(filter);
    if !matches!(filter.chars().next(), Some('>' | '<' | '=')) {
        comparison = 2; // A bare size remains a convenient minimum-size filter.
    }
    let split = value.find(|character: char| !character.is_ascii_digit() && character != '.')
        .unwrap_or(value.len());
    let number = value[..split].trim().parse::<f64>()
        .map_err(|_| format!("Invalid size filter '{filter}'. Try >5GB or <=250MB."))?;
    let unit = value[split..].trim().to_ascii_uppercase();
    let multiplier = match unit.as_str() {
        "" | "MB" => 1_000_000_f64,
        "B" => 1_f64,
        "KB" => 1_000_f64,
        "GB" => 1_000_000_000_f64,
        "TB" => 1_000_000_000_000_f64,
        "KIB" => 1_024_f64,
        "MIB" => 1_048_576_f64,
        "GIB" => 1_073_741_824_f64,
        "TIB" => 1_099_511_627_776_f64,
        _ => return Err(format!("Unknown size unit in '{filter}'. Use B, KB, MB, GB, or TB.").into()),
    };
    let bytes = number * multiplier;
    if !bytes.is_finite() || bytes < 0.0 || bytes > i64::MAX as f64 {
        return Err(format!("Size filter '{filter}' is out of range.").into());
    }
    Ok((comparison, bytes.round() as i64))
}

fn parse_modified_filter(filter: &str) -> Result<(i64, String), DatabaseError> {
    let filter = filter.trim();
    if filter.is_empty() {
        return Ok((0, String::new()));
    }
    let (comparison, value) = comparison_prefix(filter);
    let valid = value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value.chars().enumerate().all(|(index, character)| {
            index == 4 || index == 7 || character.is_ascii_digit()
        });
    if !valid {
        return Err(format!("Invalid modified-date filter '{filter}'. Try >2026-01-01.").into());
    }
    Ok((comparison, value.to_string()))
}

pub fn list_tags(database: &Database) -> Result<Vec<Tag>, DatabaseError> {
    let connection = database.connect()?;
    let mut statement = connection.prepare(
        "SELECT slug, name, color FROM tags ORDER BY name COLLATE NOCASE",
    )?;
    let rows = statement.query_map([], |row| Ok(Tag {
        slug: row.get(0)?,
        name: row.get(1)?,
        color: row.get(2)?,
    }))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn create_tag(
    database: &Database,
    name: &str,
    color: &str,
) -> Result<(), DatabaseError> {
    validate_tag(name, color)?;
    let slug = tag_slug(name);
    if slug.is_empty() {
        return Err("Tag name must contain letters or numbers".into());
    }
    let connection = database.connect()?;
    connection.execute(
        "INSERT INTO tags (slug, name, color) VALUES (?1, ?2, ?3)",
        params![slug, name.trim(), color],
    )?;
    Ok(())
}

pub fn update_tag(
    database: &Database,
    slug: &str,
    name: &str,
    color: &str,
) -> Result<(), DatabaseError> {
    validate_tag(name, color)?;
    let connection = database.connect()?;
    let updated = connection.execute(
        "UPDATE tags SET name = ?2, color = ?3 WHERE slug = ?1",
        params![slug, name.trim(), color],
    )?;
    if updated == 0 {
        return Err(format!("Unknown tag: {slug}").into());
    }
    Ok(())
}

fn validate_tag(name: &str, color: &str) -> Result<(), DatabaseError> {
    if name.trim().is_empty() || name.trim().len() > 40 {
        return Err("Tag name must be between 1 and 40 characters".into());
    }
    if color.len() != 7 || !color.starts_with('#')
        || !color[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("Tag color must use #RRGGBB format".into());
    }
    Ok(())
}

fn tag_slug(name: &str) -> String {
    name.trim().to_ascii_lowercase().chars().fold(String::new(), |mut slug, character| {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
        slug
    }).trim_end_matches('-').to_string()
}

pub fn apply_tag_recursively(
    database: &Database,
    item_ids: &[String],
    tag_slug: &str,
) -> Result<usize, DatabaseError> {
    if item_ids.is_empty() {
        return Err("Select at least one My Drive item".into());
    }
    let mut connection = database.connect()?;
    let transaction = connection.transaction()?;
    let tag_id: i64 = transaction.query_row(
        "SELECT id FROM tags WHERE slug = ?1",
        [tag_slug],
        |row| row.get(0),
    ).optional()?.ok_or_else(|| format!("Unknown tag: {tag_slug}"))?;
    let remote = RemoteKind::MyDriveRo.name();
    let mut applied = 0;

    for item_id in item_ids {
        applied += transaction.execute(
            "INSERT OR IGNORE INTO drive_item_tags (remote_name, item_id, tag_id)
             SELECT descendant.remote_name, descendant.item_id, ?3
             FROM drive_items selected
             JOIN drive_items descendant
               ON descendant.remote_name = selected.remote_name
              AND descendant.is_deleted = 0
              AND (
                   descendant.item_id = selected.item_id
                   OR (selected.is_directory = 1 AND
                       substr(descendant.relative_path, 1, length(selected.relative_path) + 1)
                           = selected.relative_path || '/')
              )
             WHERE selected.remote_name = ?1
               AND selected.item_id = ?2
               AND selected.is_deleted = 0",
            params![remote, item_id, tag_id],
        )?;
    }
    transaction.commit()?;
    Ok(applied)
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
    let mut folder_sizes: HashMap<&str, u64> = HashMap::new();

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
            let file_size = size.unwrap_or(0) as u64;
            let mut ancestor = item.path.rsplit_once('/').map(|(parent, _)| parent);
            while let Some(folder_path) = ancestor {
                let total = folder_sizes.entry(folder_path).or_default();
                *total = total.saturating_add(file_size);
                ancestor = folder_path.rsplit_once('/').map(|(parent, _)| parent);
            }
        }
    }

    transaction.execute(
        "UPDATE drive_items
         SET cumulative_size_bytes = 0
         WHERE remote_name = ?1 AND is_directory = 1 AND last_seen_scan_id = ?2",
        params![remote, scan_id],
    )?;
    for (folder_path, size) in folder_sizes {
        transaction.execute(
            "UPDATE drive_items
             SET cumulative_size_bytes = ?3
             WHERE remote_name = ?1 AND relative_path = ?2 AND is_directory = 1",
            params![remote, folder_path, size as i64],
        )?;
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
