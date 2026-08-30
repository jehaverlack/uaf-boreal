CREATE TABLE drive_items (
    remote_name TEXT NOT NULL,
    item_id TEXT NOT NULL,
    name TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    parent_path TEXT,
    is_directory INTEGER NOT NULL,
    mime_type TEXT,
    size_bytes INTEGER,
    modified_at TEXT,
    created_at TEXT,
    owner_email TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    first_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen_scan_id INTEGER NOT NULL REFERENCES scan_runs(id),
    is_deleted INTEGER NOT NULL DEFAULT 0,
    deleted_at TEXT,
    PRIMARY KEY (remote_name, item_id)
);

CREATE INDEX drive_items_path_index
    ON drive_items (remote_name, relative_path);
CREATE INDEX drive_items_owner_index
    ON drive_items (owner_email);
CREATE INDEX drive_items_size_index
    ON drive_items (size_bytes DESC);
CREATE INDEX drive_items_deleted_index
    ON drive_items (remote_name, is_deleted);

CREATE TABLE drive_permissions (
    remote_name TEXT NOT NULL,
    item_id TEXT NOT NULL,
    permission_key TEXT NOT NULL,
    permission_id TEXT,
    permission_type TEXT,
    role TEXT,
    email_address TEXT,
    display_name TEXT,
    domain TEXT,
    raw_json TEXT NOT NULL,
    last_seen_scan_id INTEGER NOT NULL REFERENCES scan_runs(id),
    PRIMARY KEY (remote_name, item_id, permission_key),
    FOREIGN KEY (remote_name, item_id)
        REFERENCES drive_items (remote_name, item_id)
        ON DELETE CASCADE
);

CREATE INDEX drive_permissions_email_index
    ON drive_permissions (email_address);
CREATE INDEX drive_permissions_domain_index
    ON drive_permissions (domain);

ALTER TABLE scan_runs ADD COLUMN files_scanned INTEGER NOT NULL DEFAULT 0;
ALTER TABLE scan_runs ADD COLUMN folders_scanned INTEGER NOT NULL DEFAULT 0;
ALTER TABLE scan_runs ADD COLUMN permissions_scanned INTEGER NOT NULL DEFAULT 0;
ALTER TABLE scan_runs ADD COLUMN bytes_discovered INTEGER NOT NULL DEFAULT 0;
ALTER TABLE scan_runs ADD COLUMN deleted_items INTEGER NOT NULL DEFAULT 0;
