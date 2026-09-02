INSERT OR IGNORE INTO tags (slug, name, description, color)
VALUES (
    'migrated',
    'Migrated',
    'Content successfully copied by a BOREAL migration and ready for source-removal review and metadata reindexing.',
    '#20c997'
);

INSERT OR IGNORE INTO drive_item_tags (remote_name, item_id, tag_id, applied_at)
SELECT assignments.remote_name, assignments.item_id, migrated.id, assignments.applied_at
FROM drive_item_tags assignments
JOIN tags previous ON previous.id = assignments.tag_id
JOIN tags migrated ON migrated.slug = 'migrated'
WHERE previous.slug = 'migration-complete';

DELETE FROM tags WHERE slug = 'migration-complete';

UPDATE tags
SET name = 'Migrated',
    description = 'Content successfully copied by a BOREAL migration and ready for source-removal review and metadata reindexing.',
    color = '#20c997'
WHERE slug = 'migrated';

DELETE FROM tag_scopes
WHERE tag_id = (SELECT id FROM tags WHERE slug = 'migrated');

INSERT INTO tag_scopes (tag_id, scope)
SELECT id, 'my-drive' FROM tags WHERE slug = 'migrated';

INSERT INTO tag_scopes (tag_id, scope)
SELECT id, 'shared-with-me' FROM tags WHERE slug = 'migrated';
