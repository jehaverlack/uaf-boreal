CREATE TABLE organizations (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    primary_domain TEXT COLLATE NOCASE,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE principals (
    id INTEGER PRIMARY KEY,
    principal_type TEXT NOT NULL DEFAULT 'person',
    primary_email TEXT UNIQUE COLLATE NOCASE,
    display_name TEXT,
    status TEXT NOT NULL DEFAULT 'unknown',
    departure_date TEXT,
    risk_level TEXT NOT NULL DEFAULT 'normal',
    notes TEXT,
    source TEXT NOT NULL DEFAULT 'manual',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX principals_type_status_index
    ON principals (principal_type, status);
CREATE INDEX principals_departure_index
    ON principals (departure_date);

CREATE TABLE principal_emails (
    principal_id INTEGER NOT NULL REFERENCES principals(id) ON DELETE CASCADE,
    email TEXT NOT NULL UNIQUE COLLATE NOCASE,
    is_primary INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (principal_id, email)
);

CREATE TABLE organization_memberships (
    organization_id INTEGER NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    principal_id INTEGER NOT NULL REFERENCES principals(id) ON DELETE CASCADE,
    membership_role TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    effective_from TEXT,
    effective_until TEXT,
    source TEXT NOT NULL DEFAULT 'manual',
    PRIMARY KEY (organization_id, principal_id)
);

CREATE TABLE principal_memberships (
    parent_principal_id INTEGER NOT NULL REFERENCES principals(id) ON DELETE CASCADE,
    member_principal_id INTEGER NOT NULL REFERENCES principals(id) ON DELETE CASCADE,
    membership_role TEXT NOT NULL DEFAULT 'member',
    status TEXT NOT NULL DEFAULT 'active',
    effective_from TEXT,
    effective_until TEXT,
    source TEXT NOT NULL DEFAULT 'manual',
    PRIMARY KEY (parent_principal_id, member_principal_id),
    CHECK (parent_principal_id <> member_principal_id)
);

CREATE TABLE directory_sources (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    source_type TEXT NOT NULL,
    source_location TEXT,
    organization_id INTEGER REFERENCES organizations(id) ON DELETE SET NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    refresh_on_metadata_update INTEGER NOT NULL DEFAULT 0,
    missing_row_policy TEXT NOT NULL DEFAULT 'unchanged',
    last_attempt_at TEXT,
    last_success_at TEXT,
    last_error TEXT,
    content_hash TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE directory_import_runs (
    id INTEGER PRIMARY KEY,
    source_id INTEGER NOT NULL REFERENCES directory_sources(id) ON DELETE CASCADE,
    started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at TEXT,
    status TEXT NOT NULL DEFAULT 'running',
    rows_seen INTEGER NOT NULL DEFAULT 0,
    rows_created INTEGER NOT NULL DEFAULT 0,
    rows_updated INTEGER NOT NULL DEFAULT 0,
    rows_rejected INTEGER NOT NULL DEFAULT 0,
    error_message TEXT
);

CREATE TABLE remote_accounts (
    remote_name TEXT PRIMARY KEY,
    account_email TEXT COLLATE NOCASE,
    display_name TEXT,
    account_id TEXT,
    raw_json TEXT NOT NULL DEFAULT '{}',
    last_verified_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
