use rusqlite::{Batch, Connection, Transaction, fallible_iterator::FallibleIterator};
use std::error::Error;
use std::{fmt, fs, io::ErrorKind, path::Path};

#[derive(Debug, Eq, PartialEq)]
pub struct Migration {
    pub name: &'static str,
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

pub static MIGRATIONS: &'static [Migration] = &[
    // Migrations refer to SQL files in this directory
    include_migration!("000_init"),
    #[cfg(test)]
    include_migration!("000_unit_test_data"),
];

pub fn migrate(connection: &mut Connection) -> Result<(), MigrationError> {
    if MIGRATIONS.is_empty() {
        return Ok(());
    }

    let last_migration_name = last_migration(connection)?;
    let migrations_to_run = migrations_after(last_migration_name);

    let transaction = connection.transaction()?;
    for migration in migrations_to_run {
        log::info!("Running migration {}", migration.name);
        migration
            .execute(&transaction)
            .map_err(|e| MigrationError::from_migration(&migration, e))?;
    }
    Ok(transaction.commit()?)
}

pub fn auto_migrate<P: AsRef<Path>>(
    connection: &mut Connection,
    database_path: P,
) -> Result<(), Box<dyn Error>> {
    if !is_migrated(&connection)? {
        log::info!("Migrating database");
        let database_path = database_path.as_ref();
        let backup_path = database_path.with_extension("bak.db3");
        log::info!("Copying backup to {}", backup_path.to_string_lossy());
        fs::copy(&database_path, &backup_path)?;
        // The other database files might not exist yet, but should be
        // backed-up.
        if let Err(e) = fs::copy(
            &database_path.with_extension("db3-shm"),
            database_path.with_extension("bak.db3-shm"),
        ) {
            if e.kind() == ErrorKind::NotFound {
                return Err(e.into());
            }
        }
        if let Err(e) = fs::copy(
            &database_path.with_extension("db3-wal"),
            database_path.with_extension("bak.db3-wal"),
        ) {
            if e.kind() == ErrorKind::NotFound {
                return Err(e.into());
            }
        }
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

    if let Some(last_migration_name) = last_migration_name.as_deref() {
        if !MIGRATIONS.iter().any(|m| m.name == last_migration_name) {
            log::warn!(
                "database is migrated to unknown migration: {}",
                last_migration_name
            );
            return Ok(true);
        }
    }

    match (last_migration_name, MIGRATIONS.iter().last().unwrap()) {
        (Some(name), migration) => Ok(name == migration.name),
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

fn migrations_after<N: AsRef<str>>(name: Option<N>) -> &'static [Migration] {
    let name = match name {
        None => return MIGRATIONS,
        Some(n) => n,
    };

    let idx = MIGRATIONS.iter().position(|m| m.name == name.as_ref());

    match idx {
        Some(i) => &MIGRATIONS[i + 1..],
        None => &[],
    }
}

#[derive(Debug)]
pub enum MigrationError {
    Rusqlite(rusqlite::Error),
    Syntax {
        name: &'static str,
        message: String,
        index: (usize, usize),
        queries: &'static str,
    },
    Migration {
        name: &'static str,
        cause: rusqlite::Error,
    },
}

impl fmt::Display for MigrationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MigrationError::Rusqlite(e) => e.fmt(f),
            MigrationError::Syntax {
                name,
                message,
                index: (line, col),
                queries,
            } => {
                write!(
                    f,
                    "{message}, in {name}.sql, line {line}, column {col}:\n{queries}"
                )
            }
            MigrationError::Migration { name, cause } => {
                write!(f, "Error running migration {name}.sql: {cause}")
            }
        }
    }
}

impl MigrationError {
    fn from_migration(migration: &Migration, error: rusqlite::Error) -> MigrationError {
        match error {
            rusqlite::Error::SqlInputError {
                msg, offset, sql, ..
            } => {
                let index = migration.queries.find(&sql).unwrap_or(0) + offset as usize;
                let (line, col) = find_line_col(migration.queries, index);
                MigrationError::Syntax {
                    name: migration.name,
                    message: msg,
                    index: (line, col),
                    queries: migration.queries,
                }
            }
            e => MigrationError::Migration {
                name: migration.name,
                cause: e,
            },
        }
    }
}

impl From<rusqlite::Error> for MigrationError {
    fn from(error: rusqlite::Error) -> MigrationError {
        MigrationError::Rusqlite(error)
    }
}

impl Error for MigrationError {}

fn find_line_col(s: &str, index: usize) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;
    for l in s[0..=index].lines() {
        line += 1;
        col = l.len();
    }
    (line, col)
}

#[cfg(test)]
mod test {
    use super::{MIGRATIONS, is_migrated, last_migration, migrate, migrations_after};
    use rusqlite::Connection;

    #[test]
    fn last_migration_returns_none_on_new_db() {
        let db = in_memory_db();
        let last_migration_name = last_migration(&db);
        assert_eq!(Ok(None), last_migration_name);
    }

    #[test]
    fn last_migration_returns_none_on_legacy_db() {
        let db = in_memory_db();
        pre_migration_schema(&db).expect("couldn't initialize db as pre-migration version did");
        let last_migration_name = last_migration(&db);
        assert_eq!(Ok(None), last_migration_name);
    }

    #[test]
    fn last_migration_returns_name_of_last_migration() {
        let db = in_memory_db();
        let expected_name = "5678_cool_migration";
        db.execute(
            "CREATE TABLE grafbabe_migrations (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL
            ) STRICT;",
            [],
        )
        .expect("couldn't insert migrations table");
        db.execute(
            "INSERT INTO grafbabe_migrations (name)
            VALUES (?1);",
            [expected_name],
        )
        .expect("couldn't insert the name of a migration");

        let last_migration_name = last_migration(&db);

        assert_eq!(Ok(Some(expected_name.to_string())), last_migration_name);
    }

    #[test]
    fn is_migrated_detects_empty_database() {
        let db = in_memory_db();
        assert!(!is_migrated(&db).unwrap());
    }

    #[test]
    fn is_migrated_detects_missing_migrations() {
        let mut db = in_memory_db();
        let mut transaction = db.transaction().unwrap();
        MIGRATIONS[0].execute(&mut transaction).unwrap();
        transaction.commit().unwrap();
        assert!(!is_migrated(&db).unwrap());
    }

    #[test]
    fn is_migrated_detects_up_to_date_database() {
        let mut db = in_memory_db();
        let _ = migrate(&mut db);
        assert!(is_migrated(&db).unwrap());
    }

    #[test]
    fn is_migrated_detects_unknown_migrations() {
        let mut db = in_memory_db();
        let mut transaction = db.transaction().unwrap();
        MIGRATIONS[0].execute(&mut transaction).unwrap();
        transaction.commit().unwrap();

        db.execute(
            "INSERT INTO grafbabe_migrations (name)
            VALUES ('000_init'), ('000_unit_test_data'), ('nonexistant');",
            (),
        )
        .unwrap();
        assert!(is_migrated(&db).unwrap());
    }

    #[test]
    fn migrations_after_skips_migrations_until_match() {
        assert_eq!(&MIGRATIONS[1..], migrations_after(Some("000_init")))
    }

    #[test]
    fn all_the_migrations_work() {
        let mut db = in_memory_db();
        migrate(&mut db).expect("migrations failed");

        let inserted_migration_names =
            read_migration_names(&db).expect("couldn't read migration names");

        let all_migration_names: Vec<String> =
            MIGRATIONS.iter().map(|m| m.name.to_string()).collect();

        assert_eq!(all_migration_names, inserted_migration_names);
    }

    #[test]
    fn all_the_migrations_work_on_legacy_db() {
        let mut db = in_memory_db();
        pre_migration_schema(&db).expect("couldn't initialize db as pre-migration version did");
        migrate(&mut db).expect("migrations failed");

        let inserted_migration_names =
            read_migration_names(&db).expect("couldn't read migration names");

        let all_migration_names: Vec<String> =
            MIGRATIONS.iter().map(|m| m.name.to_string()).collect();

        assert_eq!(all_migration_names, inserted_migration_names);
    }

    fn in_memory_db() -> Connection {
        in_memory_db_inner().expect("couldn't create in-memory db")
    }

    fn in_memory_db_inner() -> rusqlite::Result<Connection> {
        let db = Connection::open_in_memory()?;
        db.pragma_update(None, "foreign_keys", "ON")?;
        rusqlite::vtab::array::load_module(&db)?;
        Ok(db)
    }

    fn read_migration_names(db: &Connection) -> rusqlite::Result<Vec<String>> {
        let mut statement = db.prepare("SELECT name FROM grafbabe_migrations ORDER BY id ASC")?;
        let mut rows = statement.query([])?;
        let mut names: Vec<String> = Vec::with_capacity(MIGRATIONS.len());
        while let Some(row) = rows.next()? {
            names.push(row.get(0)?);
        }
        Ok(names)
    }

    fn pre_migration_schema(db: &Connection) -> rusqlite::Result<()> {
        // Previously the contents of the database/init.rs module
        db.execute(
            "CREATE TABLE IF NOT EXISTS metrics (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                kind INTEGER NOT NULL DEFAULT 0,
                help TEXT
            ) STRICT;",
            (),
        )?;
        db.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS metrics_by_name ON metrics(name);",
            (),
        )?;

        db.execute(
            "CREATE TABLE IF NOT EXISTS labels (
                id INTEGER PRIMARY KEY,
                label TEXT NOT NULL
            ) STRICT;",
            (),
        )?;
        db.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS labels_by_label ON labels(label);",
            (),
        )?;

        db.execute(
            "CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY,
                timestamp INTEGER NOT NULL
            ) STRICT;",
            (),
        )?;
        db.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS events_by_timestamp ON events(timestamp);",
            (),
        )?;

        db.execute(
            "CREATE TABLE IF NOT EXISTS metric_values (
                metric_id INTEGER NOT NULL REFERENCES metrics(id) ON DELETE CASCADE,
                label_id INTEGER REFERENCES labels(id) ON DELETE CASCADE,
                event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
                value INTEGER NOT NULL
            ) STRICT;",
            (),
        )?;
        db.execute(
            "CREATE INDEX IF NOT EXISTS metric_values_by_event_id ON metric_values(event_id);",
            (),
        )?;

        Ok(())
    }
}
