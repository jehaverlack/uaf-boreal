CREATE TABLE migration_jobs (
    id INTEGER PRIMARY KEY,
    source_scope TEXT NOT NULL,
    source_kind TEXT NOT NULL CHECK (source_kind IN ('my-drive', 'shared-with-me')),
    status TEXT NOT NULL DEFAULT 'draft',
    phase TEXT NOT NULL DEFAULT 'Select destination',
    destination_url TEXT NOT NULL DEFAULT '',
    destination_drive_id TEXT NOT NULL DEFAULT '',
    destination_drive_name TEXT NOT NULL DEFAULT '',
    destination_folder_id TEXT NOT NULL DEFAULT '',
    destination_folder_name TEXT NOT NULL DEFAULT '',
    files_total INTEGER NOT NULL DEFAULT 0,
    folders_total INTEGER NOT NULL DEFAULT 0,
    bytes_total INTEGER NOT NULL DEFAULT 0,
    files_copied INTEGER NOT NULL DEFAULT 0,
    bytes_copied INTEGER NOT NULL DEFAULT 0,
    exceptions_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    started_at TEXT,
    completed_at TEXT,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    error_message TEXT NOT NULL DEFAULT ''
);

CREATE TABLE migration_sources (
    migration_id INTEGER NOT NULL REFERENCES migration_jobs(id) ON DELETE CASCADE,
    item_id TEXT NOT NULL,
    name TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    is_directory INTEGER NOT NULL,
    files_total INTEGER NOT NULL DEFAULT 0,
    folders_total INTEGER NOT NULL DEFAULT 0,
    bytes_total INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (migration_id, item_id)
);

CREATE INDEX idx_migration_jobs_status ON migration_jobs(status, updated_at DESC);
CREATE INDEX idx_migration_sources_job ON migration_sources(migration_id);
