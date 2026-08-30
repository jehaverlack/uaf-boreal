use rusqlite::{
    params,
    OptionalExtension,
    Transaction,
};

use super::{
    Database,
    DatabaseError,
};

#[derive(Debug, Clone)]
pub struct InventorySettings {
    pub automatic_updates: bool,
    pub refresh_interval_hours: u32,
    pub full_reconciliation_days: u32,
    pub update_when_overdue_at_startup: bool,
    pub permission_scanning: bool,
    pub directory_sheet_enabled: bool,
    pub directory_sheet_url: String,
}

impl Default for InventorySettings {
    fn default() -> Self {
        Self {
            automatic_updates: true,
            refresh_interval_hours: 24,
            full_reconciliation_days: 7,
            update_when_overdue_at_startup: true,
            permission_scanning: true,
            directory_sheet_enabled: false,
            directory_sheet_url: String::new(),
        }
    }
}

impl InventorySettings {
    pub fn validate(
        &self,
    ) -> Result<(), DatabaseError> {
        if !(1..=720).contains(
            &self.refresh_interval_hours,
        ) {
            return Err(
                "Refresh interval must be between 1 and 720 hours"
                    .into(),
            );
        }

        if !(1..=365).contains(
            &self.full_reconciliation_days,
        ) {
            return Err(
                "Full reconciliation interval must be between 1 and 365 days"
                    .into(),
            );
        }

        if self.directory_sheet_enabled && self.directory_sheet_url.trim().is_empty() {
            return Err("A Google Sheets URL is required when linked directory import is enabled".into());
        }

        Ok(
            (),
        )
    }
}

pub fn load(
    database: &Database,
) -> Result<InventorySettings, DatabaseError> {
    let connection = database.connect()?;
    let defaults = InventorySettings::default();

    Ok(
        InventorySettings {
            automatic_updates: get_bool(
                &connection,
                "inventory.automatic_updates",
                defaults.automatic_updates,
            )?,
            refresh_interval_hours: get_u32(
                &connection,
                "inventory.refresh_interval_hours",
                defaults.refresh_interval_hours,
            )?,
            full_reconciliation_days: get_u32(
                &connection,
                "inventory.full_reconciliation_days",
                defaults.full_reconciliation_days,
            )?,
            update_when_overdue_at_startup: get_bool(
                &connection,
                "inventory.update_when_overdue_at_startup",
                defaults.update_when_overdue_at_startup,
            )?,
            permission_scanning: get_bool(
                &connection,
                "inventory.permission_scanning",
                defaults.permission_scanning,
            )?,
            directory_sheet_enabled: get_bool(
                &connection,
                "directory.sheet_enabled",
                defaults.directory_sheet_enabled,
            )?,
            directory_sheet_url: get(
                &connection,
                "directory.sheet_url",
            )?.unwrap_or(defaults.directory_sheet_url),
        },
    )
}

pub fn save(
    database: &Database,
    settings: &InventorySettings,
) -> Result<(), DatabaseError> {
    settings.validate()?;

    let mut connection = database.connect()?;
    let transaction = connection.transaction()?;

    set(
        &transaction,
        "inventory.automatic_updates",
        bool_value(settings.automatic_updates),
    )?;
    set(
        &transaction,
        "inventory.refresh_interval_hours",
        &settings.refresh_interval_hours.to_string(),
    )?;
    set(
        &transaction,
        "inventory.full_reconciliation_days",
        &settings.full_reconciliation_days.to_string(),
    )?;
    set(
        &transaction,
        "inventory.update_when_overdue_at_startup",
        bool_value(settings.update_when_overdue_at_startup),
    )?;
    set(
        &transaction,
        "inventory.permission_scanning",
        bool_value(settings.permission_scanning),
    )?;
    set(
        &transaction,
        "directory.sheet_enabled",
        bool_value(settings.directory_sheet_enabled),
    )?;
    set(
        &transaction,
        "directory.sheet_url",
        settings.directory_sheet_url.trim(),
    )?;
    transaction.execute(
        "INSERT INTO directory_sources (
            name, source_type, source_location, enabled, refresh_on_metadata_update
         ) VALUES ('Linked Google Sheet directory', 'google_sheet', ?1, ?2, ?2)
         ON CONFLICT(name) DO UPDATE SET
            source_location = excluded.source_location,
            enabled = excluded.enabled,
            refresh_on_metadata_update = excluded.refresh_on_metadata_update,
            updated_at = CURRENT_TIMESTAMP",
        params![
            settings.directory_sheet_url.trim(),
            settings.directory_sheet_enabled,
        ],
    )?;

    transaction.commit()?;

    Ok(
        (),
    )
}

fn get_bool(
    connection: &rusqlite::Connection,
    key: &str,
    default: bool,
) -> Result<bool, DatabaseError> {
    let value = get(
        connection,
        key,
    )?;

    match value.as_deref() {
        None => Ok(default),
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        Some(value) => Err(
            format!(
                "Invalid boolean setting {key}: {value}"
            )
            .into(),
        ),
    }
}

fn get_u32(
    connection: &rusqlite::Connection,
    key: &str,
    default: u32,
) -> Result<u32, DatabaseError> {
    match get(
        connection,
        key,
    )? {
        None => Ok(default),
        Some(value) => value.parse::<u32>()
            .map_err(
                |error| format!(
                    "Invalid numeric setting {key}: {error}"
                ).into(),
            ),
    }
}

fn get(
    connection: &rusqlite::Connection,
    key: &str,
) -> Result<Option<String>, DatabaseError> {
    Ok(
        connection.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .optional()?,
    )
}

fn set(
    transaction: &Transaction<'_>,
    key: &str,
    value: &str,
) -> Result<(), DatabaseError> {
    transaction.execute(
        "INSERT INTO settings (
            key,
            value,
            updated_at
        ) VALUES (?1, ?2, CURRENT_TIMESTAMP)
        ON CONFLICT(key) DO UPDATE SET
            value = excluded.value,
            updated_at = CURRENT_TIMESTAMP",
        params![
            key,
            value,
        ],
    )?;

    Ok(
        (),
    )
}

fn bool_value(
    value: bool,
) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}
