ALTER TABLE tag_scopes RENAME TO tag_scopes_old;

CREATE TABLE tag_scopes (
    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    scope TEXT NOT NULL CHECK (scope IN (
        'directory', 'my-drive', 'shared-drives', 'shared-with-me',
        'github-repositories', 'keeper-shared-folders'
    )),
    PRIMARY KEY (tag_id, scope)
);
INSERT INTO tag_scopes (tag_id, scope) SELECT tag_id, scope FROM tag_scopes_old;
DROP TABLE tag_scopes_old;
CREATE INDEX idx_tag_scopes_scope ON tag_scopes(scope, tag_id);

CREATE TABLE keeper_shared_folders (
    folder_uid TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    folder_type TEXT NOT NULL DEFAULT '',
    folder_path TEXT NOT NULL DEFAULT '',
    is_accessible INTEGER NOT NULL DEFAULT 1,
    last_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE keeper_shared_folder_access (
    folder_uid TEXT NOT NULL REFERENCES keeper_shared_folders(folder_uid) ON DELETE CASCADE,
    shared_to TEXT NOT NULL,
    permissions TEXT NOT NULL DEFAULT '',
    target_kind TEXT NOT NULL DEFAULT 'user',
    PRIMARY KEY (folder_uid, shared_to, permissions)
);

CREATE TABLE keeper_shared_folder_tags (
    folder_uid TEXT NOT NULL REFERENCES keeper_shared_folders(folder_uid) ON DELETE CASCADE,
    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (folder_uid, tag_id)
);

CREATE INDEX keeper_shared_folders_name ON keeper_shared_folders(name);
CREATE INDEX keeper_shared_folder_access_target ON keeper_shared_folder_access(shared_to);

-- These existing workflow tags directly support offboarding, access review,
-- and responsibility handoff without creating Keeper-specific duplicates.
INSERT OR IGNORE INTO tag_scopes(tag_id, scope)
SELECT id, 'keeper-shared-folders'
FROM tags
WHERE slug IN (
    'needs-review', 'permission-review', 'needs-handoff', 'retain',
    'remove-my-permissions'
);
