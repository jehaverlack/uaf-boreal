use rusqlite::{
    params,
    Connection,
};

use super::DatabaseError;

struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "foundation",
        sql: include_str!(
            "migrations/0001_foundation.sql"
        ),
    },
    Migration {
        version: 2,
        name: "drive_inventory",
        sql: include_str!(
            "migrations/0002_drive_inventory.sql"
        ),
    },
    Migration {
        version: 3,
        name: "folder_sizes",
        sql: include_str!(
            "migrations/0003_folder_sizes.sql"
        ),
    },
    Migration {
        version: 4,
        name: "tags",
        sql: include_str!(
            "migrations/0004_tags.sql"
        ),
    },
    Migration {
        version: 5,
        name: "tag_colors",
        sql: include_str!(
            "migrations/0005_tag_colors.sql"
        ),
    },
    Migration {
        version: 6,
        name: "identity_directory",
        sql: include_str!(
            "migrations/0006_identity_directory.sql"
        ),
    },
    Migration {
        version: 7,
        name: "identity_lookup_indexes",
        sql: include_str!(
            "migrations/0007_identity_lookup_indexes.sql"
        ),
    },
];

pub fn apply(
    connection: &mut Connection,
) -> Result<(), DatabaseError> {
    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        ",
    )?;

    for migration in MIGRATIONS {
        let already_applied: bool = connection.query_row(
            "SELECT EXISTS(
                SELECT 1
                FROM schema_migrations
                WHERE version = ?1
            )",
            [migration.version],
            |row| row.get(0),
        )?;

        if already_applied {
            continue;
        }

        let transaction = connection.transaction()?;

        transaction.execute_batch(
            migration.sql,
        )?;

        transaction.execute(
            "INSERT INTO schema_migrations (
                version,
                name
            ) VALUES (?1, ?2)",
            params![
                migration.version,
                migration.name,
            ],
        )?;

        transaction.commit()?;

        println!(
            "Applied database migration {}: {}",
            migration.version,
            migration.name,
        );
    }

    Ok(
        (),
    )
}
