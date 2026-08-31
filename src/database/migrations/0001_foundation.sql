CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE scan_runs (
    id INTEGER PRIMARY KEY,
    scan_type TEXT NOT NULL,
    status TEXT NOT NULL,
    started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at TEXT,
    error_message TEXT
);

CREATE INDEX scan_runs_started_at_index
    ON scan_runs (started_at DESC);

CREATE INDEX scan_runs_status_index
    ON scan_runs (status);
