ALTER TABLE tags ADD COLUMN color TEXT NOT NULL DEFAULT '#6c757d';

UPDATE tags SET color = '#0d6efd' WHERE slug = 'to-migrate';
UPDATE tags SET color = '#dc3545' WHERE slug = 'to-delete';
UPDATE tags SET color = '#198754' WHERE slug = 'to-export';
