INSERT OR IGNORE INTO tags (slug, name, description, color) VALUES
    (
        'access-review',
        'Access Review',
        'Person, group, or account whose existing Google Drive access should be reviewed.',
        '#6f42c1'
    ),
    (
        'needs-review',
        'Needs Review',
        'Content requiring investigation before making a migration, deletion, handoff, or retention decision.',
        '#ffc107'
    ),
    (
        'permission-review',
        'Permission Review',
        'Content whose sharing permissions, managers, or access should be reviewed.',
        '#6610f2'
    ),
    (
        'needs-handoff',
        'Needs Handoff',
        'Content that should be transferred, documented, or assigned to another responsible person.',
        '#0dcaf0'
    ),
    (
        'retain',
        'Keep',
        'Content intentionally marked by the user to remain in its current location.',
        '#00d149'
    ),
    (
        'migration-complete',
        'Migration Complete',
        'Content the user has marked as successfully migrated or handed off.',
        '#20c997'
    );

UPDATE tags
SET description = 'Content the user has reviewed and marked as ready for manual deletion from Google Drive.'
WHERE slug = 'safe-to-delete'
  AND description IN (
      '',
      'Migration verified; source content is safe to delete manually in Google Drive',
      'Content confirmed as migrated or backed up and ready to be deleted manually from Google Drive.'
  );

DELETE FROM tag_scopes
WHERE tag_id IN (
    SELECT id FROM tags
    WHERE slug IN (
        'data-loss-risk',
        'access-review',
        'needs-review',
        'permission-review',
        'needs-handoff',
        'retain',
        'to-migrate',
        'migration-complete',
        'to-delete',
        'safe-to-delete'
    )
);

INSERT INTO tag_scopes (tag_id, scope)
SELECT id, 'directory'
FROM tags
WHERE slug IN ('data-loss-risk', 'access-review');

INSERT INTO tag_scopes (tag_id, scope)
SELECT id, scope
FROM tags
CROSS JOIN (
    SELECT 'my-drive' AS scope
    UNION ALL SELECT 'shared-drives'
    UNION ALL SELECT 'shared-with-me'
)
WHERE slug IN (
    'needs-review',
    'permission-review',
    'needs-handoff',
    'retain',
    'migration-complete',
    'to-delete',
    'safe-to-delete'
);

INSERT INTO tag_scopes (tag_id, scope)
SELECT id, scope
FROM tags
CROSS JOIN (
    SELECT 'my-drive' AS scope
    UNION ALL SELECT 'shared-with-me'
)
WHERE slug = 'to-migrate';
