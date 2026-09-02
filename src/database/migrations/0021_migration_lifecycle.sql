ALTER TABLE migration_jobs ADD COLUMN archived_at TEXT;
CREATE INDEX idx_migration_jobs_archived ON migration_jobs(archived_at, created_at DESC);
