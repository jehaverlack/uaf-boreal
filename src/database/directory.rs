use rusqlite::{OptionalExtension, params};

use super::{Database, DatabaseError};

#[derive(Debug, Clone, Default)]
pub struct ImportSummary {
    pub rows_seen: u64,
    pub rows_created: u64,
    pub rows_updated: u64,
    pub rows_rejected: u64,
}

#[derive(Debug, Clone)]
pub struct DirectorySummary {
    pub principals: u64,
    pub organizations: u64,
    pub groups: u64,
    pub former_or_departing: u64,
    pub sources: u64,
}

#[derive(Debug, Clone)]
pub struct PrincipalRow {
    pub id: i64,
    pub display_name: String,
    pub primary_email: String,
    pub principal_type: String,
    pub status: String,
    pub departure_date: String,
    pub organizations: String,
    pub members: u64,
    pub owned_items: u64,
    pub permitted_items: u64,
}

#[derive(Debug, Clone)]
pub struct OrganizationRow {
    pub name: String,
    pub primary_domain: String,
    pub status: String,
    pub members: u64,
}

#[derive(Debug, Clone)]
pub struct RemoteAccountRow {
    pub remote_name: String,
    pub account_email: String,
    pub display_name: String,
    pub last_verified_at: String,
}

#[derive(Debug, Clone)]
pub struct PrincipalAssociationRow {
    pub remote_name: String,
    pub item_id: String,
    pub name: String,
    pub relative_path: String,
    pub relationship: String,
    pub role: String,
    pub owner_email: String,
    pub is_deleted: bool,
}

pub fn summary(database: &Database) -> Result<DirectorySummary, DatabaseError> {
    let connection = database.connect()?;
    connection
        .query_row(
            "SELECT
            (SELECT COUNT(*) FROM principals),
            (SELECT COUNT(*) FROM organizations),
            (SELECT COUNT(*) FROM principals WHERE principal_type = 'google_group'),
            (SELECT COUNT(*) FROM principals WHERE status IN ('former', 'departing')),
            (SELECT COUNT(*) FROM directory_sources WHERE enabled = 1)",
            [],
            |row| {
                Ok(DirectorySummary {
                    principals: row.get::<_, i64>(0)? as u64,
                    organizations: row.get::<_, i64>(1)? as u64,
                    groups: row.get::<_, i64>(2)? as u64,
                    former_or_departing: row.get::<_, i64>(3)? as u64,
                    sources: row.get::<_, i64>(4)? as u64,
                })
            },
        )
        .map_err(Into::into)
}

pub fn list_principals(database: &Database) -> Result<Vec<PrincipalRow>, DatabaseError> {
    list_principals_filtered(database, "", "", "", "", "", "")
}

pub fn list_principals_filtered(
    database: &Database,
    name_filter: &str,
    email_filter: &str,
    type_filter: &str,
    status_filter: &str,
    departure_filter: &str,
    organization_filter: &str,
) -> Result<Vec<PrincipalRow>, DatabaseError> {
    let connection = database.connect()?;
    let (departure_comparison, departure_value) = parse_date_filter(departure_filter)?;
    let mut statement = connection.prepare(
        "WITH identity_emails AS (
            SELECT id AS principal_id, lower(primary_email) AS email
            FROM principals WHERE COALESCE(primary_email, '') <> ''
            UNION
            SELECT principal_id, lower(email) FROM principal_emails
         ),
         owned AS (
            SELECT ie.principal_id, COUNT(*) AS item_count
            FROM identity_emails ie
            JOIN drive_items di ON lower(di.owner_email) = ie.email AND di.is_deleted = 0
            GROUP BY ie.principal_id
         ),
         permitted AS (
            SELECT ie.principal_id,
                   COUNT(DISTINCT dp.remote_name || char(31) || dp.item_id) AS item_count
            FROM identity_emails ie
            JOIN drive_permissions dp ON lower(dp.email_address) = ie.email
            JOIN drive_items di ON di.remote_name = dp.remote_name
                               AND di.item_id = dp.item_id AND di.is_deleted = 0
            GROUP BY ie.principal_id
         ),
         memberships AS (
            SELECT parent_principal_id AS principal_id, COUNT(*) AS member_count
            FROM principal_memberships WHERE status = 'active'
            GROUP BY parent_principal_id
         ),
         principal_organizations AS (
            SELECT om.principal_id, group_concat(o.name, ', ') AS names
            FROM organization_memberships om
            JOIN organizations o ON o.id = om.organization_id
            GROUP BY om.principal_id
         )
         SELECT p.id, COALESCE(p.display_name, ''), COALESCE(p.primary_email, ''),
                p.principal_type, p.status, COALESCE(p.departure_date, ''),
                COALESCE(po.names, ''), COALESCE(m.member_count, 0),
                COALESCE(owned.item_count, 0), COALESCE(permitted.item_count, 0)
         FROM principals p
         LEFT JOIN principal_organizations po ON po.principal_id = p.id
         LEFT JOIN memberships m ON m.principal_id = p.id
         LEFT JOIN owned ON owned.principal_id = p.id
         LEFT JOIN permitted ON permitted.principal_id = p.id
         WHERE (?1 = '' OR instr(lower(COALESCE(p.display_name, '')), lower(?1)) > 0)
           AND (?2 = '' OR instr(lower(COALESCE(p.primary_email, '')), lower(?2)) > 0
                OR EXISTS (
                    SELECT 1 FROM principal_emails pe
                    WHERE pe.principal_id = p.id
                      AND instr(lower(pe.email), lower(?2)) > 0
                ))
           AND (?3 = '' OR instr(lower(p.principal_type), lower(?3)) > 0)
           AND (?4 = '' OR instr(lower(p.status), lower(?4)) > 0)
           AND (?5 = 0 OR
                (?5 = 1 AND COALESCE(p.departure_date, '') > ?6) OR
                (?5 = 2 AND COALESCE(p.departure_date, '') >= ?6) OR
                (?5 = 3 AND COALESCE(p.departure_date, '') < ?6) OR
                (?5 = 4 AND COALESCE(p.departure_date, '') <= ?6) OR
                (?5 = 5 AND COALESCE(p.departure_date, '') = ?6))
           AND (?7 = '' OR instr(lower(COALESCE(po.names, '')), lower(?7)) > 0)
         ORDER BY p.display_name COLLATE NOCASE, p.primary_email COLLATE NOCASE",
    )?;
    let rows = statement.query_map(
        params![
            name_filter.trim(),
            email_filter.trim(),
            type_filter.trim(),
            status_filter.trim(),
            departure_comparison,
            departure_value,
            organization_filter.trim(),
        ],
        |row| {
            Ok(PrincipalRow {
                id: row.get(0)?,
                display_name: row.get(1)?,
                primary_email: row.get(2)?,
                principal_type: row.get(3)?,
                status: row.get(4)?,
                departure_date: row.get(5)?,
                organizations: row.get(6)?,
                members: row.get::<_, i64>(7)? as u64,
                owned_items: row.get::<_, i64>(8)? as u64,
                permitted_items: row.get::<_, i64>(9)? as u64,
            })
        },
    )?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn parse_date_filter(filter: &str) -> Result<(i64, String), DatabaseError> {
    let filter = filter.trim();
    if filter.is_empty() {
        return Ok((0, String::new()));
    }
    let (comparison, value) = if let Some(value) = filter.strip_prefix(">=") {
        (2, value)
    } else if let Some(value) = filter.strip_prefix("<=") {
        (4, value)
    } else if let Some(value) = filter.strip_prefix('>') {
        (1, value)
    } else if let Some(value) = filter.strip_prefix('<') {
        (3, value)
    } else if let Some(value) = filter.strip_prefix('=') {
        (5, value)
    } else {
        (5, filter)
    };
    let value = value.trim();
    let valid = value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value
            .chars()
            .enumerate()
            .all(|(index, character)| index == 4 || index == 7 || character.is_ascii_digit());
    if !valid {
        return Err(format!("Invalid departure-date filter '{filter}'. Try <2026-12-31.").into());
    }
    Ok((comparison, value.to_string()))
}

pub fn get_principal(
    database: &Database,
    principal_id: i64,
) -> Result<Option<PrincipalRow>, DatabaseError> {
    list_principals(database).map(|principals| {
        principals
            .into_iter()
            .find(|principal| principal.id == principal_id)
    })
}

pub fn list_principal_associations(
    database: &Database,
    principal_id: i64,
) -> Result<Vec<PrincipalAssociationRow>, DatabaseError> {
    let connection = database.connect()?;
    let mut statement = connection.prepare(
        "SELECT remote_name, item_id, name, relative_path, relationship, role,
                COALESCE(owner_email, ''), is_deleted
         FROM (
            SELECT di.remote_name, di.item_id, di.name, di.relative_path,
                   'Owner' AS relationship, 'owner' AS role, di.owner_email, di.is_deleted
            FROM drive_items di
            WHERE lower(COALESCE(di.owner_email, '')) IN (
                SELECT lower(primary_email) FROM principals WHERE id = ?1
                UNION SELECT lower(email) FROM principal_emails WHERE principal_id = ?1
            )
            UNION ALL
            SELECT di.remote_name, di.item_id, di.name, di.relative_path,
                   'Permission' AS relationship, COALESCE(dp.role, ''),
                   di.owner_email, di.is_deleted
            FROM drive_permissions dp
            JOIN drive_items di ON di.remote_name = dp.remote_name AND di.item_id = dp.item_id
            WHERE lower(COALESCE(dp.email_address, '')) IN (
                SELECT lower(primary_email) FROM principals WHERE id = ?1
                UNION SELECT lower(email) FROM principal_emails WHERE principal_id = ?1
            )
         )
         ORDER BY is_deleted, relationship, name COLLATE NOCASE, remote_name",
    )?;
    let rows = statement.query_map([principal_id], |row| {
        Ok(PrincipalAssociationRow {
            remote_name: row.get(0)?,
            item_id: row.get(1)?,
            name: row.get(2)?,
            relative_path: row.get(3)?,
            relationship: row.get(4)?,
            role: row.get(5)?,
            owner_email: row.get(6)?,
            is_deleted: row.get(7)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn list_organizations(database: &Database) -> Result<Vec<OrganizationRow>, DatabaseError> {
    let connection = database.connect()?;
    let mut statement = connection.prepare(
        "SELECT o.name, COALESCE(o.primary_domain, ''), o.status,
                COUNT(om.principal_id)
         FROM organizations o
         LEFT JOIN organization_memberships om ON om.organization_id = o.id
         GROUP BY o.id
         ORDER BY o.name COLLATE NOCASE",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(OrganizationRow {
            name: row.get(0)?,
            primary_domain: row.get(1)?,
            status: row.get(2)?,
            members: row.get::<_, i64>(3)? as u64,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn list_remote_accounts(database: &Database) -> Result<Vec<RemoteAccountRow>, DatabaseError> {
    let connection = database.connect()?;
    let mut statement = connection.prepare(
        "SELECT remote_name, COALESCE(account_email, ''), COALESCE(display_name, ''),
                last_verified_at
         FROM remote_accounts ORDER BY remote_name COLLATE NOCASE",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(RemoteAccountRow {
            remote_name: row.get(0)?,
            account_email: row.get(1)?,
            display_name: row.get(2)?,
            last_verified_at: row.get(3)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn remote_account_email(
    database: &Database,
    remote_name: &str,
) -> Result<Option<String>, DatabaseError> {
    let connection = database.connect()?;
    connection
        .query_row(
            "SELECT account_email FROM remote_accounts
             WHERE remote_name = ?1 AND COALESCE(account_email, '') <> ''",
            [remote_name],
            |row| row.get(0),
        )
        .optional()
        .map_err(Into::into)
}

pub fn save_remote_account(
    database: &Database,
    remote_name: &str,
    account_email: Option<&str>,
    display_name: Option<&str>,
    account_id: Option<&str>,
    raw_json: &str,
) -> Result<(), DatabaseError> {
    let connection = database.connect()?;
    connection.execute(
        "INSERT INTO remote_accounts (
            remote_name, account_email, display_name, account_id, raw_json
         ) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(remote_name) DO UPDATE SET
            account_email = excluded.account_email,
            display_name = excluded.display_name,
            account_id = excluded.account_id,
            raw_json = excluded.raw_json,
            last_verified_at = CURRENT_TIMESTAMP",
        params![
            remote_name,
            account_email,
            display_name,
            account_id,
            raw_json
        ],
    )?;
    Ok(())
}

pub fn import_csv(
    database: &Database,
    filename: &str,
    data: &[u8],
) -> Result<ImportSummary, DatabaseError> {
    let text = std::str::from_utf8(data).map_err(|_| "Directory CSV must use UTF-8 encoding")?;
    let rows = parse_csv(text)?;
    if rows.is_empty() {
        return Err("Directory CSV is empty".into());
    }
    let headers: Vec<String> = rows[0]
        .iter()
        .map(|value| normalize_header(value))
        .collect();
    let email_index = header_index(&headers, &["email", "primary_email", "email_address"])
        .ok_or("Directory CSV requires an email column")?;
    let name_index = header_index(&headers, &["name", "display_name"]);
    let type_index = header_index(&headers, &["type", "category", "principal_type"]);
    let status_index = header_index(&headers, &["status", "employment_status"]);
    let departure_index = header_index(&headers, &["departure_date", "end_date"]);
    let organization_index = header_index(&headers, &["organization", "department"]);
    let notes_index = header_index(&headers, &["notes", "note"]);

    let mut connection = database.connect()?;
    let transaction = connection.transaction()?;
    let source_name = format!("CSV: {}", filename.trim());
    transaction.execute(
        "INSERT INTO directory_sources (name, source_type, enabled)
         VALUES (?1, 'csv_upload', 1)
         ON CONFLICT(name) DO UPDATE SET updated_at = CURRENT_TIMESTAMP",
        [&source_name],
    )?;
    let source_id: i64 = transaction.query_row(
        "SELECT id FROM directory_sources WHERE name = ?1",
        [&source_name],
        |row| row.get(0),
    )?;
    transaction.execute(
        "INSERT INTO directory_import_runs (source_id) VALUES (?1)",
        [source_id],
    )?;
    let run_id = transaction.last_insert_rowid();
    let mut summary = ImportSummary::default();

    for row in rows.iter().skip(1) {
        if row.iter().all(|value| value.trim().is_empty()) {
            continue;
        }
        summary.rows_seen += 1;
        let email = cell(row, email_index).trim().to_ascii_lowercase();
        if !valid_email(&email) {
            summary.rows_rejected += 1;
            continue;
        }
        let display_name = name_index
            .map(|index| cell(row, index).trim())
            .unwrap_or("");
        let principal_type = type_index
            .map(|index| cell(row, index).trim())
            .filter(|value| !value.is_empty())
            .map(normalize_value)
            .unwrap_or_else(|| "person".to_string());
        let status = status_index
            .map(|index| cell(row, index).trim())
            .filter(|value| !value.is_empty())
            .map(normalize_value)
            .unwrap_or_else(|| "unknown".to_string());
        let departure = departure_index
            .map(|index| cell(row, index).trim())
            .filter(|value| !value.is_empty());
        let notes = notes_index
            .map(|index| cell(row, index).trim())
            .filter(|value| !value.is_empty());
        let existed: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM principals WHERE lower(primary_email) = lower(?1))",
            [&email],
            |row| row.get(0),
        )?;
        transaction.execute(
            "INSERT INTO principals (
                principal_type, primary_email, display_name, status, departure_date, notes, source
             ) VALUES (?1, ?2, NULLIF(?3, ''), ?4, ?5, ?6, ?7)
             ON CONFLICT(primary_email) DO UPDATE SET
                principal_type = excluded.principal_type,
                display_name = COALESCE(excluded.display_name, principals.display_name),
                status = excluded.status,
                departure_date = excluded.departure_date,
                notes = COALESCE(excluded.notes, principals.notes),
                source = excluded.source,
                updated_at = CURRENT_TIMESTAMP",
            params![
                principal_type,
                email,
                display_name,
                status,
                departure,
                notes,
                source_name
            ],
        )?;
        let principal_id: i64 = transaction.query_row(
            "SELECT id FROM principals WHERE lower(primary_email) = lower(?1)",
            [&email],
            |row| row.get(0),
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO principal_emails (principal_id, email, is_primary)
             VALUES (?1, ?2, 1)",
            params![principal_id, email],
        )?;
        if let Some(organization_name) = organization_index
            .map(|index| cell(row, index).trim())
            .filter(|value| !value.is_empty())
        {
            transaction.execute(
                "INSERT INTO organizations (name) VALUES (?1) ON CONFLICT(name) DO NOTHING",
                [organization_name],
            )?;
            let organization_id: i64 = transaction.query_row(
                "SELECT id FROM organizations WHERE name = ?1 COLLATE NOCASE",
                [organization_name],
                |row| row.get(0),
            )?;
            transaction.execute(
                "INSERT INTO organization_memberships (
                    organization_id, principal_id, status, source
                 ) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(organization_id, principal_id) DO UPDATE SET
                    status = excluded.status, source = excluded.source",
                params![organization_id, principal_id, status, source_name],
            )?;
        }
        if existed {
            summary.rows_updated += 1;
        } else {
            summary.rows_created += 1;
        }
    }
    transaction.execute(
        "UPDATE directory_import_runs SET
            status = 'complete', completed_at = CURRENT_TIMESTAMP,
            rows_seen = ?2, rows_created = ?3, rows_updated = ?4, rows_rejected = ?5
         WHERE id = ?1",
        params![
            run_id,
            summary.rows_seen as i64,
            summary.rows_created as i64,
            summary.rows_updated as i64,
            summary.rows_rejected as i64,
        ],
    )?;
    transaction.execute(
        "UPDATE directory_sources SET
            last_attempt_at = CURRENT_TIMESTAMP, last_success_at = CURRENT_TIMESTAMP,
            last_error = NULL, updated_at = CURRENT_TIMESTAMP
         WHERE id = ?1",
        [source_id],
    )?;
    transaction.commit()?;
    Ok(summary)
}

fn parse_csv(text: &str) -> Result<Vec<Vec<String>>, DatabaseError> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut characters = text.chars().peekable();
    let mut quoted = false;
    while let Some(character) = characters.next() {
        match character {
            '"' if quoted && characters.peek() == Some(&'"') => {
                field.push('"');
                characters.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => row.push(std::mem::take(&mut field)),
            '\n' if !quoted => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            '\r' if !quoted && characters.peek() == Some(&'\n') => {}
            character => field.push(character),
        }
    }
    if quoted {
        return Err("Directory CSV contains an unterminated quoted field".into());
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    Ok(rows)
}

fn normalize_header(value: &str) -> String {
    normalize_value(value.trim_start_matches('\u{feff}'))
}

fn normalize_value(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

fn header_index(headers: &[String], names: &[&str]) -> Option<usize> {
    headers
        .iter()
        .position(|header| names.contains(&header.as_str()))
}

fn cell(row: &[String], index: usize) -> &str {
    row.get(index).map(String::as_str).unwrap_or("")
}

fn valid_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
}

#[cfg(test)]
mod tests {
    use super::parse_csv;

    #[test]
    fn parses_quoted_csv_fields_and_newlines() {
        let rows = parse_csv(
            "email,notes\r\na@example.edu,\"One, two\"\r\nb@example.edu,\"Line 1\nLine 2\"\n",
        )
        .expect("CSV should parse");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1][1], "One, two");
        assert_eq!(rows[2][1], "Line 1\nLine 2");
    }
}
