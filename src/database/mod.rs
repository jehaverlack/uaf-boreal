mod migrations;
pub mod inventory;
pub mod settings;

use std::{
    error::Error,
    fs,
    path::{
        Path,
        PathBuf,
    },
    time::Duration,
};

use rusqlite::Connection;
use rusqlite::params;

use crate::bootstrap::Runtime;

pub type DatabaseError =
    Box<dyn Error + Send + Sync>;

const DATABASE_FILE_NAME: &str =
    "boreal.sqlite";

#[derive(Debug, Clone)]
pub struct Database {
    path: PathBuf,
}

impl Database {
    /// Initialize BOREAL's SQLite database and apply all pending migrations.
    pub fn initialize(
        runtime: &Runtime,
    ) -> Result<Self, DatabaseError> {
        let path = path(
            runtime,
        )?;

        if let Some(
            parent,
        ) = path.parent()
        {
            fs::create_dir_all(
                parent,
            )?;
        }

        let database = Self {
            path,
        };

        let mut connection = database.connect()?;

        migrations::apply(
            &mut connection,
        )?;

        Ok(
            database,
        )
    }

    /// Open a configured connection to the database.
    ///
    /// Connections are intentionally short-lived so future background jobs can
    /// perform database work without sharing a non-Sync SQLite connection.
    pub fn connect(
        &self,
    ) -> Result<Connection, DatabaseError> {
        let connection = Connection::open(
            &self.path,
        )?;

        configure_connection(
            &connection,
        )?;

        Ok(
            connection,
        )
    }

    pub fn path(
        &self,
    ) -> &Path {
        &self.path
    }

    pub fn start_scan_run(
        &self,
        scan_type: &str,
    ) -> Result<i64, DatabaseError> {
        let connection = self.connect()?;

        connection.execute(
            "INSERT INTO scan_runs (
                scan_type,
                status
            ) VALUES (?1, 'running')",
            [scan_type],
        )?;

        Ok(
            connection.last_insert_rowid(),
        )
    }

    pub fn fail_scan_run(
        &self,
        id: i64,
        error: &str,
    ) -> Result<(), DatabaseError> {
        let connection = self.connect()?;

        connection.execute(
            "UPDATE scan_runs
             SET status = 'error',
                 completed_at = CURRENT_TIMESTAMP,
                 error_message = ?2
             WHERE id = ?1",
            params![
                id,
                error,
            ],
        )?;

        Ok(
            (),
        )
    }
}

fn path(
    runtime: &Runtime,
) -> Result<PathBuf, DatabaseError> {
    let sqlite_dir = runtime
        .directories
        .get("SQLITE")
        .ok_or(
            "BOREAL SQLITE directory is not configured",
        )?;

    Ok(
        sqlite_dir.join(
            DATABASE_FILE_NAME,
        ),
    )
}

fn configure_connection(
    connection: &Connection,
) -> Result<(), DatabaseError> {
    connection.busy_timeout(
        Duration::from_secs(
            5,
        ),
    )?;

    connection.execute_batch(
        "
        PRAGMA foreign_keys = ON;
        PRAGMA journal_mode = WAL;
        ",
    )?;

    Ok(
        (),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::rclone::inventory::DriveItem;

    use super::*;

    fn runtime(
        root: &Path,
    ) -> Runtime {
        let mut directories =
            BTreeMap::new();

        directories.insert(
            "SQLITE".to_string(),
            root.join(
                "data/sqlite",
            ),
        );

        Runtime {
            boreal_home: root.to_path_buf(),
            boreal: serde_json::json!({}),
            directories,
        }
    }

    fn temporary_directory() -> PathBuf {
        let unique = format!(
            "boreal-database-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(
                    std::time::UNIX_EPOCH,
                )
                .expect(
                    "system clock should be valid",
                )
                .as_nanos(),
        );

        std::env::temp_dir().join(
            unique,
        )
    }

    #[test]
    fn initializes_and_reopens_database() {
        let root = temporary_directory();
        let runtime = runtime(
            &root,
        );

        let first = Database::initialize(
            &runtime,
        )
        .expect(
            "database should initialize",
        );

        assert!(
            first.path().is_file(),
        );

        let second = Database::initialize(
            &runtime,
        )
        .expect(
            "database should reopen",
        );

        let connection = second.connect()
            .expect(
                "database should connect",
            );

        let migration_count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .expect(
            "migration count should be readable",
        );

        assert_eq!(
            migration_count,
            3,
        );

        fs::remove_dir_all(
            root,
        )
        .expect(
            "temporary database directory should be removable",
        );
    }

    #[test]
    fn creates_foundation_tables() {
        let root = temporary_directory();
        let database = Database::initialize(
            &runtime(&root),
        )
        .expect(
            "database should initialize",
        );
        let connection = database.connect()
            .expect(
                "database should connect",
            );

        for table in [
            "schema_migrations",
            "settings",
            "scan_runs",
            "drive_items",
            "drive_permissions",
        ] {
            let exists: bool = connection.query_row(
                "SELECT EXISTS(
                    SELECT 1
                    FROM sqlite_master
                    WHERE type = 'table' AND name = ?1
                )",
                [table],
                |row| row.get(0),
            )
            .expect(
                "table lookup should succeed",
            );

            assert!(
                exists,
                "missing table: {table}",
            );
        }

        drop(
            connection,
        );

        fs::remove_dir_all(
            root,
        )
        .expect(
            "temporary database directory should be removable",
        );
    }

    #[test]
    fn persists_inventory_settings() {
        let root = temporary_directory();
        let database = Database::initialize(
            &runtime(&root),
        )
        .expect(
            "database should initialize",
        );
        let expected = settings::InventorySettings {
            automatic_updates: false,
            refresh_interval_hours: 12,
            full_reconciliation_days: 14,
            update_when_overdue_at_startup: false,
            permission_scanning: true,
        };

        settings::save(
            &database,
            &expected,
        )
        .expect(
            "settings should save",
        );

        let actual = settings::load(
            &database,
        )
        .expect(
            "settings should load",
        );

        assert_eq!(
            actual.automatic_updates,
            expected.automatic_updates,
        );
        assert_eq!(
            actual.refresh_interval_hours,
            expected.refresh_interval_hours,
        );
        assert_eq!(
            actual.full_reconciliation_days,
            expected.full_reconciliation_days,
        );
        assert_eq!(
            actual.update_when_overdue_at_startup,
            expected.update_when_overdue_at_startup,
        );
        assert_eq!(
            actual.permission_scanning,
            expected.permission_scanning,
        );

        fs::remove_dir_all(
            root,
        )
        .expect(
            "temporary database directory should be removable",
        );
    }

    #[test]
    fn inventory_upserts_and_soft_deletes_missing_items() {
        let root = temporary_directory();
        let database = Database::initialize(&runtime(&root))
            .expect("database should initialize");
        let mut metadata = BTreeMap::new();
        metadata.insert("owner".to_string(), "owner@example.edu".to_string());
        metadata.insert(
            "permissions".to_string(),
            r#"[{"id":"permission-1","type":"user","role":"reader","emailAddress":"reader@example.edu"}]"#
                .to_string(),
        );
        let item = DriveItem {
            id: "drive-id-1".to_string(),
            name: "Report.pdf".to_string(),
            path: "Reports/Report.pdf".to_string(),
            is_dir: false,
            size: 42,
            mime_type: "application/pdf".to_string(),
            mod_time: "2026-08-29T12:00:00Z".to_string(),
            metadata,
        };
        let folder = DriveItem {
            id: "folder-id-1".to_string(),
            name: "Reports".to_string(),
            path: "Reports".to_string(),
            is_dir: true,
            size: -1,
            mime_type: "inode/directory".to_string(),
            mod_time: "2026-08-29T12:00:00Z".to_string(),
            metadata: BTreeMap::new(),
        };

        let first_scan = database.start_scan_run("my-drive").expect("scan should start");
        let first_summary = inventory::synchronize_my_drive(
            &database,
            first_scan,
            &[folder, item.clone(), item],
            true,
        ).expect("inventory should synchronize");
        assert_eq!(first_summary.files_scanned, 1);
        assert_eq!(first_summary.folders_scanned, 1);
        assert_eq!(first_summary.bytes_discovered, 42);
        assert_eq!(first_summary.permissions_scanned, 1);
        let root_items = inventory::list_my_drive_directory(&database, None, "", "name", false)
            .expect("explorer root should be readable");
        assert_eq!(root_items.len(), 1);
        assert_eq!(root_items[0].size_bytes, Some(42));
        assert_eq!(
            inventory::list_my_drive_directory(&database, None, "report", "size", true)
                .expect("filtered explorer root should be readable")
                .len(),
            1,
        );
        assert!(
            inventory::list_my_drive_directory(&database, None, "missing", "name", false)
                .expect("empty explorer search should be readable")
                .is_empty(),
        );
        let second_scan = database.start_scan_run("my-drive").expect("scan should start");
        let summary = inventory::synchronize_my_drive(&database, second_scan, &[], true)
            .expect("empty authoritative inventory should synchronize");

        assert_eq!(summary.deleted_items, 2);
        assert_eq!(summary.files_scanned, 0);
        assert_eq!(summary.bytes_discovered, 0);
        assert!(
            inventory::list_my_drive_directory(&database, Some("Reports"), "", "name", false)
                .expect("explorer directory should be readable")
                .is_empty(),
            "soft-deleted items must not appear in the explorer",
        );
        let connection = database.connect().expect("database should connect");
        let values: (i64, i64, String) = connection.query_row(
            "SELECT i.is_deleted, i.size_bytes, p.email_address
             FROM drive_items i JOIN drive_permissions p
               ON p.remote_name = i.remote_name AND p.item_id = i.item_id
             WHERE i.item_id = 'drive-id-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).expect("soft-deleted item and permission should remain queryable");
        assert_eq!(values, (1, 42, "reader@example.edu".to_string()));

        drop(connection);
        fs::remove_dir_all(root).expect("temporary database directory should be removable");
    }
}
