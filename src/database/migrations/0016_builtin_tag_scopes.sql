INSERT OR IGNORE INTO tags (slug, name, description, color) VALUES
    (
        'data-loss-risk',
        'Data Loss Risk',
        'Person whose Drive data may be at risk of loss',
        '#fd7e14'
    );

UPDATE tags
SET slug = 'safe-to-delete',
    name = 'Safe to Delete',
    description = 'Migration verified; source content is safe to delete manually in Google Drive',
    color = '#198754'
WHERE slug = 'safe-for-removal'
  AND NOT EXISTS (SELECT 1 FROM tags WHERE slug = 'safe-to-delete');

INSERT OR IGNORE INTO tags (slug, name, description, color) VALUES
    (
        'safe-to-delete',
        'Safe to Delete',
        'Migration verified; source content is safe to delete manually in Google Drive',
        '#198754'
    ),
    (
        'to-delete',
        'To Delete',
        'Content selected for deletion review',
        '#dc3545'
    ),
    (
        'to-migrate',
        'To Migrate',
        'Content selected for migration',
        '#0d6efd'
    );

DELETE FROM tag_scopes
WHERE tag_id IN (
    SELECT id FROM tags
    WHERE slug IN ('data-loss-risk', 'safe-to-delete', 'to-delete', 'to-migrate')
);

INSERT INTO tag_scopes (tag_id, scope)
SELECT id, 'directory' FROM tags WHERE slug = 'data-loss-risk';

INSERT INTO tag_scopes (tag_id, scope)
SELECT id, scope
FROM tags
CROSS JOIN (
    SELECT 'my-drive' AS scope
    UNION ALL SELECT 'shared-drives'
    UNION ALL SELECT 'shared-with-me'
)
WHERE slug IN ('safe-to-delete', 'to-delete');

INSERT INTO tag_scopes (tag_id, scope)
SELECT id, scope
FROM tags
CROSS JOIN (
    SELECT 'my-drive' AS scope
    UNION ALL SELECT 'shared-with-me'
)
WHERE slug = 'to-migrate';
