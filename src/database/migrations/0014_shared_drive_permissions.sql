CREATE TABLE shared_drive_permissions (
    drive_id TEXT NOT NULL REFERENCES shared_drives(drive_id) ON DELETE CASCADE,
    permission_key TEXT NOT NULL,
    permission_id TEXT,
    permission_type TEXT,
    role TEXT,
    email_address TEXT,
    display_name TEXT,
    domain TEXT,
    raw_json TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (drive_id, permission_key)
);

CREATE INDEX shared_drive_permissions_email_index
    ON shared_drive_permissions (email_address);
