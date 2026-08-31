INSERT INTO settings (key, value) VALUES ('directory.setup_skipped', 'false')
ON CONFLICT(key) DO NOTHING;
