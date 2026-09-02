ALTER TABLE migration_sources ADD COLUMN status TEXT NOT NULL DEFAULT 'pending';
ALTER TABLE migration_sources ADD COLUMN started_at TEXT;
ALTER TABLE migration_sources ADD COLUMN completed_at TEXT;
ALTER TABLE migration_sources ADD COLUMN error_message TEXT NOT NULL DEFAULT '';
