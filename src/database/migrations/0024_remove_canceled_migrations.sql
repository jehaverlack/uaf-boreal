DELETE FROM migration_jobs WHERE status = 'canceled' AND started_at IS NULL;
