CREATE TABLE shared_drives (
    drive_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    inventory_scope TEXT NOT NULL UNIQUE,
    is_accessible INTEGER NOT NULL DEFAULT 1,
    first_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_scanned_at TEXT,
    last_error TEXT,
    files_scanned INTEGER NOT NULL DEFAULT 0,
    folders_scanned INTEGER NOT NULL DEFAULT 0,
    permissions_scanned INTEGER NOT NULL DEFAULT 0,
    bytes_discovered INTEGER NOT NULL DEFAULT 0,
    deleted_items INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX shared_drives_accessible_name_index
    ON shared_drives (is_accessible, name COLLATE NOCASE);
