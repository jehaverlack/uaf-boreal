INSERT OR IGNORE INTO tags (slug, name, description, color) VALUES (
    'remove-my-permissions',
    'Remove My Permissions',
    'Content where the current user''s access should be removed when leaving an organization or role.',
    '#b45309'
);

DELETE FROM tag_scopes
WHERE tag_id = (SELECT id FROM tags WHERE slug = 'remove-my-permissions');

INSERT INTO tag_scopes (tag_id, scope)
SELECT id, scope
FROM tags
CROSS JOIN (
    SELECT 'my-drive' AS scope
    UNION ALL SELECT 'shared-drives'
    UNION ALL SELECT 'shared-with-me'
)
WHERE slug = 'remove-my-permissions';
