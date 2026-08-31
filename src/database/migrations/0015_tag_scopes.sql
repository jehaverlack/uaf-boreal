CREATE TABLE tag_scopes (
    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    scope TEXT NOT NULL CHECK (scope IN (
        'directory',
        'my-drive',
        'shared-drives',
        'shared-with-me'
    )),
    PRIMARY KEY (tag_id, scope)
);

INSERT INTO tag_scopes (tag_id, scope) SELECT id, 'directory' FROM tags;
INSERT INTO tag_scopes (tag_id, scope) SELECT id, 'my-drive' FROM tags;
INSERT INTO tag_scopes (tag_id, scope) SELECT id, 'shared-drives' FROM tags;
INSERT INTO tag_scopes (tag_id, scope) SELECT id, 'shared-with-me' FROM tags;

CREATE INDEX idx_tag_scopes_scope ON tag_scopes(scope, tag_id);
