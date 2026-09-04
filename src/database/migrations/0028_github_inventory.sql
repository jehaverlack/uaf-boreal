ALTER TABLE tag_scopes RENAME TO tag_scopes_old;

CREATE TABLE tag_scopes (
    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    scope TEXT NOT NULL CHECK (scope IN (
        'directory', 'my-drive', 'shared-drives', 'shared-with-me', 'github-repositories'
    )),
    PRIMARY KEY (tag_id, scope)
);
INSERT INTO tag_scopes (tag_id, scope) SELECT tag_id, scope FROM tag_scopes_old;
DROP TABLE tag_scopes_old;
CREATE INDEX idx_tag_scopes_scope ON tag_scopes(scope, tag_id);

CREATE TABLE github_organizations (
    organization_id INTEGER PRIMARY KEY,
    login TEXT NOT NULL,
    html_url TEXT NOT NULL,
    is_accessible INTEGER NOT NULL DEFAULT 1,
    last_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE github_repositories (
    repository_id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    full_name TEXT NOT NULL,
    html_url TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    owner_id INTEGER NOT NULL,
    owner_login TEXT NOT NULL,
    owner_url TEXT NOT NULL,
    owner_kind TEXT NOT NULL,
    visibility TEXT NOT NULL,
    archived INTEGER NOT NULL DEFAULT 0,
    disabled INTEGER NOT NULL DEFAULT 0,
    fork INTEGER NOT NULL DEFAULT 0,
    is_template INTEGER NOT NULL DEFAULT 0,
    default_branch TEXT NOT NULL DEFAULT '',
    language TEXT NOT NULL DEFAULT '',
    topics TEXT NOT NULL DEFAULT '',
    size_kb INTEGER NOT NULL DEFAULT 0,
    open_issues INTEGER NOT NULL DEFAULT 0,
    effective_permission TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL DEFAULT '',
    pushed_at TEXT NOT NULL DEFAULT '',
    is_accessible INTEGER NOT NULL DEFAULT 1,
    last_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX github_repositories_owner_name ON github_repositories(owner_login, name);
CREATE INDEX github_repositories_pushed ON github_repositories(pushed_at);

CREATE TABLE github_repository_tags (
    repository_id INTEGER NOT NULL REFERENCES github_repositories(repository_id) ON DELETE CASCADE,
    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY(repository_id, tag_id)
);
