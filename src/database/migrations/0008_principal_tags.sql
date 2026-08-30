CREATE TABLE principal_tags (
    principal_id INTEGER NOT NULL REFERENCES principals(id) ON DELETE CASCADE,
    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (principal_id, tag_id)
);

CREATE INDEX principal_tags_tag_index
    ON principal_tags (tag_id, principal_id);
