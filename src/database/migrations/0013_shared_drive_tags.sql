CREATE TABLE shared_drive_tags (
    drive_id TEXT NOT NULL REFERENCES shared_drives(drive_id) ON DELETE CASCADE,
    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (drive_id, tag_id)
);

CREATE INDEX shared_drive_tags_tag_index
    ON shared_drive_tags (tag_id, drive_id);
