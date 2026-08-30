CREATE TABLE tags (
    id INTEGER PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE drive_item_tags (
    remote_name TEXT NOT NULL,
    item_id TEXT NOT NULL,
    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (remote_name, item_id, tag_id),
    FOREIGN KEY (remote_name, item_id)
        REFERENCES drive_items (remote_name, item_id)
        ON DELETE CASCADE
);

CREATE INDEX drive_item_tags_tag_index
    ON drive_item_tags (tag_id, remote_name, item_id);

INSERT INTO tags (slug, name, description) VALUES
    ('to-migrate', 'To Migrate', 'Content selected for migration'),
    ('to-delete', 'To Delete', 'Content selected for deletion review'),
    ('to-export', 'To Export', 'Content selected for export');
