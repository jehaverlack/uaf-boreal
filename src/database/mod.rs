mod migrations;
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

    pub fn complete_scan_run(
        &self,
        id: i64,
    ) -> Result<String, DatabaseError> {
        let connection = self.connect()?;

        connection.execute(
            "UPDATE scan_runs
             SET status = 'complete',
                 completed_at = CURRENT_TIMESTAMP,
                 error_message = NULL
             WHERE id = ?1",
            [id],
        )?;

        let completed_at = connection.query_row(
            "SELECT completed_at
             FROM scan_runs
             WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;

        Ok(
            completed_at,
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
            1,
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
}
