UPDATE tags
SET description = 'Person whose Google Drive content may be lost, such as a departing or former user.'
WHERE slug = 'data-loss-risk'
  AND description IN ('', 'Person whose Drive data may be at risk of loss');

UPDATE tags
SET description = 'Content confirmed as migrated or backed up and ready to be deleted manually from Google Drive.'
WHERE slug = 'safe-to-delete'
  AND description IN (
      '',
      'Migration verified; source content is safe to delete manually in Google Drive'
  );

UPDATE tags
SET description = 'Content identified for deletion but not yet confirmed as safe to delete.'
WHERE slug = 'to-delete'
  AND description IN ('', 'Content selected for deletion review');

UPDATE tags
SET description = 'Content selected to be moved to another Drive location or handed off to another owner.'
WHERE slug = 'to-migrate'
  AND description IN ('', 'Content selected for migration');
