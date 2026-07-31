use rusqlite::{Batch, Connection, Transaction, fallible_iterator::FallibleIterator};
use std::error::Error;
use std::{fs, path::Path};

pub struct Migration {
    name: &'static str,
    queries: &'static str,
}

impl Migration {
    pub fn execute(&self, transaction: &Transaction) -> rusqlite::Result<()> {
        let mut batch = Batch::new(&transaction, self.queries);
        while let Some(mut statement) = batch.next()? {
            statement.execute([])?;
        }

        transaction.execute(
            "INSERT INTO grafbabe_migrations (name) VALUES (?1);",
            [self.name],
        )?;

        Ok(())
    }
}

macro_rules! include_migration {
    ($name:tt) => {
        Migration {
            name: $name,
            queries: include_str!(concat!($name, ".sql")),
        }
    };
}

static MIGRATIONS: &'static [Migration] = &[
    // Migrations refer to SQL files in this directory
    include_migration!("000_init"),
];

pub fn migrate(connection: &mut Connection) -> rusqlite::Result<()> {
    if MIGRATIONS.is_empty() {
        return Ok(());
    }

    let last_migration_name = last_migration(connection)?;

    // Skip all migrations up until the last migration, then skip that one too
    let migrations_to_run = MIGRATIONS
        .iter()
        .skip_while(|m| {
            last_migration_name
                .as_ref()
                .map(|name| name != m.name)
                .unwrap_or(false)
        })
        .skip_while(|m| {
            last_migration_name
                .as_ref()
                .map(|name| name == m.name)
                .unwrap_or(false)
        });

    let transaction = connection.transaction()?;
    for migration in migrations_to_run {
        log::info!("Running migration {}", migration.name);
        migration.execute(&transaction)?;
    }
    transaction.commit()
}

pub fn auto_migrate<P: AsRef<Path>>(
    connection: &mut Connection,
    database_path: P,
) -> Result<(), Box<dyn Error>> {
    if !is_migrated(&connection)? {
        log::info!("Migrating database");
        let backup_path = database_path.as_ref().with_extension("db3.bak");
        log::info!("Copying backup to {}", backup_path.to_string_lossy());
        fs::copy(&database_path, &backup_path)?;
        migrate(connection)?;
    }

    Ok(())
}

fn is_migrated(connection: &Connection) -> rusqlite::Result<bool> {
    if MIGRATIONS.is_empty() {
        return Ok(true);
    }

    let last_migration_name = last_migration(connection)?;

    match last_migration_name {
        Some(ref name) => log::debug!("last migration found: {}", name),
        None => log::debug!("no migrations run yet"),
    }

    match (last_migration_name, MIGRATIONS.iter().last()) {
        (Some(name), Some(migration)) => Ok(name == migration.name),
        (Some(_name), None) => {
            // TODO warn about a newer migration than the binary knows about?
            Ok(true)
        }
        (None, _) => Ok(false),
    }
}

fn last_migration(connection: &Connection) -> rusqlite::Result<Option<String>> {
    match connection.query_one(
        "SELECT EXISTS (
            SELECT name
            FROM sqlite_schema
            WHERE type = 'table' AND name = 'grafbabe_migrations'
        );",
        (),
        |row| row.get(0),
    )? {
        true => {}
        false => return Ok(None),
    }

    match connection.query_one(
        "SELECT name
         FROM grafbabe_migrations
         ORDER BY id DESC
         LIMIT 1",
        (),
        |row| row.get(0),
    ) {
        Ok(name) => Ok(Some(name)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}
