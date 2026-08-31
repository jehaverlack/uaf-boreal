INSERT INTO settings (key, value) VALUES ('inventory.automatic_updates', 'false')
ON CONFLICT(key) DO UPDATE SET value = 'false', updated_at = CURRENT_TIMESTAMP;

INSERT INTO settings (key, value) VALUES ('inventory.update_when_overdue_at_startup', 'false')
ON CONFLICT(key) DO UPDATE SET value = 'false', updated_at = CURRENT_TIMESTAMP;
