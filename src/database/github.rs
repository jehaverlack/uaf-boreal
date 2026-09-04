use rusqlite::{OptionalExtension, params};

use crate::github::client::Repository;

use super::{Database, DatabaseError, inventory::Tag};

#[derive(Debug, Clone)]
pub struct RepositoryRow {
    pub repository_id: i64,
    pub name: String,
    pub html_url: String,
    pub description: String,
    pub owner_login: String,
    pub owner_kind: String,
    pub visibility: String,
    pub archived: bool,
    pub fork: bool,
    pub language: String,
    pub size_kb: u64,
    pub effective_permission: String,
    pub pushed_at: String,
    pub tags: Vec<Tag>,
}

pub fn synchronize(database: &Database, repositories: &[Repository]) -> Result<(), DatabaseError> {
    let mut connection = database.connect()?;
    let transaction = connection.transaction()?;
    transaction.execute("UPDATE github_repositories SET is_accessible = 0", [])?;
    transaction.execute("UPDATE github_organizations SET is_accessible = 0", [])?;
    for repository in repositories {
        let visibility = if repository.visibility.is_empty() {
            if repository.private {
                "private"
            } else {
                "public"
            }
        } else {
            &repository.visibility
        };
        if repository.owner.kind.eq_ignore_ascii_case("organization") {
            transaction.execute(
                "INSERT INTO github_organizations(organization_id,login,html_url,is_accessible,last_seen_at)
                 VALUES(?1,?2,?3,1,CURRENT_TIMESTAMP)
                 ON CONFLICT(organization_id) DO UPDATE SET login=excluded.login,
                    html_url=excluded.html_url,is_accessible=1,last_seen_at=CURRENT_TIMESTAMP",
                params![repository.owner.id, repository.owner.login, repository.owner.html_url],
            )?;
        }
        transaction.execute(
            "INSERT INTO github_repositories
             (repository_id, name, full_name, html_url, description, owner_id, owner_login,
              owner_url, owner_kind, visibility, archived, disabled, fork, is_template,
              default_branch, language, topics, size_kb, open_issues, effective_permission,
              created_at, updated_at, pushed_at, is_accessible, last_seen_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,
                     ?18,?19,?20,?21,?22,?23,1,CURRENT_TIMESTAMP)
             ON CONFLICT(repository_id) DO UPDATE SET
              name=excluded.name, full_name=excluded.full_name, html_url=excluded.html_url,
              description=excluded.description, owner_id=excluded.owner_id,
              owner_login=excluded.owner_login, owner_url=excluded.owner_url,
              owner_kind=excluded.owner_kind, visibility=excluded.visibility,
              archived=excluded.archived, disabled=excluded.disabled, fork=excluded.fork,
              is_template=excluded.is_template, default_branch=excluded.default_branch,
              language=excluded.language, topics=excluded.topics, size_kb=excluded.size_kb,
              open_issues=excluded.open_issues, effective_permission=excluded.effective_permission,
              created_at=excluded.created_at, updated_at=excluded.updated_at,
              pushed_at=excluded.pushed_at, is_accessible=1, last_seen_at=CURRENT_TIMESTAMP",
            params![
                repository.id,
                repository.name,
                repository.full_name,
                repository.html_url,
                repository.description.as_deref().unwrap_or(""),
                repository.owner.id,
                repository.owner.login,
                repository.owner.html_url,
                repository.owner.kind,
                visibility,
                repository.archived,
                repository.disabled,
                repository.fork,
                repository.is_template,
                repository.default_branch,
                repository.language.as_deref().unwrap_or(""),
                repository.topics.join(", "),
                repository.size as i64,
                repository.open_issues_count as i64,
                repository.effective_permission(),
                repository.created_at,
                repository.updated_at,
                repository.pushed_at,
            ],
        )?;
    }
    super::settings::set_in_transaction(
        &transaction,
        "github.last_sync_at",
        &current_timestamp(&transaction)?,
    )?;
    transaction.commit()?;
    Ok(())
}

fn current_timestamp(transaction: &rusqlite::Transaction<'_>) -> Result<String, rusqlite::Error> {
    transaction.query_row("SELECT CURRENT_TIMESTAMP", [], |row| row.get(0))
}

pub fn list(
    database: &Database,
    search: &str,
    owner: &str,
    visibility: &str,
    permission: &str,
    tag: &str,
    include_inaccessible: bool,
    sort: &str,
    descending: bool,
) -> Result<Vec<RepositoryRow>, DatabaseError> {
    let (exclude_tag, tag_slug) = tag
        .strip_prefix('!')
        .map_or((false, tag), |value| (true, value));
    let order = match sort {
        "owner" => "owner_login",
        "visibility" => "visibility",
        "size" => "size_kb",
        "permission" => "effective_permission",
        "pushed" => "pushed_at",
        _ => "full_name",
    };
    let direction = if descending { "DESC" } else { "ASC" };
    let sql = format!(
        "SELECT repository_id,name,full_name,html_url,description,owner_login,owner_kind,
                visibility,archived,fork,language,size_kb,effective_permission,pushed_at
         FROM github_repositories r
         WHERE (?1 OR is_accessible=1)
           AND (?2='' OR full_name LIKE '%'||?2||'%' OR description LIKE '%'||?2||'%' OR topics LIKE '%'||?2||'%')
           AND (?3='' OR owner_login LIKE '%'||?3||'%')
           AND (?4='' OR visibility=?4)
           AND (?5='' OR effective_permission=?5)
           AND (?6='' OR
                (?8=1 AND ((?7=0 AND NOT EXISTS(SELECT 1 FROM github_repository_tags rt WHERE rt.repository_id=r.repository_id))
                         OR (?7=1 AND EXISTS(SELECT 1 FROM github_repository_tags rt WHERE rt.repository_id=r.repository_id))))
                OR (?8=0 AND ((?7=0 AND EXISTS(SELECT 1 FROM github_repository_tags rt JOIN tags t ON t.id=rt.tag_id WHERE rt.repository_id=r.repository_id AND t.slug=?6))
                           OR (?7=1 AND NOT EXISTS(SELECT 1 FROM github_repository_tags rt JOIN tags t ON t.id=rt.tag_id WHERE rt.repository_id=r.repository_id AND t.slug=?6)))))
         ORDER BY {order} {direction}, repository_id {direction}"
    );
    let connection = database.connect()?;
    let mut statement = connection.prepare(&sql)?;
    let mut rows = statement
        .query_map(
            params![
                include_inaccessible,
                search.trim(),
                owner.trim(),
                visibility,
                permission,
                tag_slug,
                exclude_tag,
                tag_slug == super::inventory::UNTAGGED_TAG_FILTER
            ],
            |row| {
                Ok(RepositoryRow {
                    repository_id: row.get(0)?,
                    name: row.get(1)?,
                    html_url: row.get(3)?,
                    description: row.get(4)?,
                    owner_login: row.get(5)?,
                    owner_kind: row.get(6)?,
                    visibility: row.get(7)?,
                    archived: row.get(8)?,
                    fork: row.get(9)?,
                    language: row.get(10)?,
                    size_kb: row.get::<_, i64>(11)? as u64,
                    effective_permission: row.get(12)?,
                    pushed_at: row.get(13)?,
                    tags: Vec::new(),
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let mut tag_statement = connection.prepare(
        "SELECT t.slug,t.name,t.description,t.color,
                0,0,0,0,1 FROM github_repository_tags rt JOIN tags t ON t.id=rt.tag_id
         WHERE rt.repository_id=?1 ORDER BY t.name COLLATE NOCASE",
    )?;
    for repository in &mut rows {
        repository.tags = tag_statement
            .query_map([repository.repository_id], |row| {
                Ok(Tag {
                    slug: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    color: row.get(3)?,
                    directory: row.get(4)?,
                    my_drive: row.get(5)?,
                    shared_drives: row.get(6)?,
                    shared_with_me: row.get(7)?,
                    github_repositories: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
    }
    Ok(rows)
}

pub fn change_tags(
    database: &Database,
    repository_ids: &[i64],
    slug: &str,
    remove: bool,
) -> Result<usize, DatabaseError> {
    if repository_ids.is_empty() {
        return Err("Select at least one GitHub repository".into());
    }
    let mut connection = database.connect()?;
    let transaction = connection.transaction()?;
    let tag_id: i64 = transaction.query_row(
        if remove { "SELECT id FROM tags WHERE slug=?1" } else {
            "SELECT t.id FROM tags t JOIN tag_scopes s ON s.tag_id=t.id WHERE t.slug=?1 AND s.scope='github-repositories'"
        }, [slug], |row| row.get(0)
    ).optional()?.ok_or_else(|| format!("Tag '{slug}' is not available for GitHub repositories"))?;
    let mut changed = 0;
    for id in repository_ids {
        changed += if remove {
            transaction.execute(
                "DELETE FROM github_repository_tags WHERE repository_id=?1 AND tag_id=?2",
                params![id, tag_id],
            )?
        } else {
            transaction.execute("INSERT OR IGNORE INTO github_repository_tags(repository_id,tag_id) SELECT repository_id,?2 FROM github_repositories WHERE repository_id=?1", params![id,tag_id])?
        };
    }
    transaction.commit()?;
    Ok(changed)
}
