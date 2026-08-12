use rusqlite::{Batch, Connection, Transaction, fallible_iterator::FallibleIterator};
use std::error::Error;
use std::{fmt, fs, io::ErrorKind, path::Path};

#[derive(Debug, Eq, PartialEq)]
pub struct Migration {
    from: &'static str,
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

macro_rules! include_schema {
    ($name:tt) => {
        Migration {
            from: "",
            name: $name,
            queries: include_str!("schema.sql"),
        }
    };
}

macro_rules! include_migration {
    ($from:tt, $to:tt) => {
        Migration {
            from: $from,
            name: $to,
            queries: include_str!(concat!($to, ".sql")),
        }
    };
}

pub static SCHEMA: Migration = include_schema!("001_change_values_to_real");

static MIGRATIONS: &'static [Migration] = &[
    // Migrations refer to SQL files in this directory
    include_migration!("pre_migration", "000_init"),
    include_migration!("000_init", "001_change_values_to_real"),
];

fn migrate(connection: &mut Connection, from_migration: &str) -> Result<(), MigrationError> {
    if MIGRATIONS.is_empty() {
        return Ok(());
    }

    if is_latest_migration(from_migration) {
        return Ok(());
    }

    let migrations_to_run = {
        let idx = MIGRATIONS.iter().position(|m| m.from == from_migration);
        idx.map(|i| &MIGRATIONS[i..]).unwrap_or_default()
    };
    if migrations_to_run.is_empty() {
        return Ok(());
    }

    let transaction = connection.transaction()?;
    for migration in migrations_to_run {
        log::info!("Running migration {}", migration.name);
        migration
            .execute(&transaction)
            .map_err(|e| MigrationError::from_migration(&migration, e))?;
    }
    Ok(transaction.commit()?)
}

pub fn migrate_fresh_db(connection: &mut Connection) -> Result<(), MigrationError> {
    log::info!("Applying database schema {}", SCHEMA.name);
    let transaction = connection.transaction()?;
    SCHEMA
        .execute(&transaction)
        .map_err(|e| MigrationError::from_migration(&SCHEMA, e))?;
    Ok(transaction.commit()?)
}

pub fn auto_migrate<P: AsRef<Path>>(
    connection: &mut Connection,
    database_path: P,
) -> Result<(), Box<dyn Error>> {
    match last_migration(connection)? {
        None => migrate_fresh_db(connection)?,
        Some(name) => {
            log::info!("Migrating database");
            backup_databases(database_path)?;
            migrate(connection, &name)?;
        }
    }

    Ok(())
}

fn backup_databases<P: AsRef<Path>>(database_path: P) -> std::io::Result<()> {
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
            return Err(e);
        }
    }
    if let Err(e) = fs::copy(
        &database_path.with_extension("db3-wal"),
        database_path.with_extension("bak.db3-wal"),
    ) {
        if e.kind() == ErrorKind::NotFound {
            return Err(e);
        }
    }
    Ok(())
}

fn is_latest_migration(migrate_from: &str) -> bool {
    if MIGRATIONS.is_empty() {
        return true;
    }

    if migrate_from != "pre_migration" && !MIGRATIONS.iter().any(|m| m.name == migrate_from) {
        log::warn!(
            "database is migrated to unknown migration: {}",
            migrate_from,
        );
        return true;
    }

    migrate_from == MIGRATIONS[MIGRATIONS.len() - 1].name
}

fn last_migration(connection: &Connection) -> rusqlite::Result<Option<String>> {
    let (migration_aware, has_any_data) = connection.query_one(
        "SELECT EXISTS (
            SELECT name
            FROM sqlite_schema
            WHERE type = 'table' AND name = 'grafbabe_migrations'
        ), EXISTS (
            SELECT name
            FROM sqlite_schema
            WHERE type = 'table' AND name = 'metric_values'
        );",
        (),
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    match (migration_aware, has_any_data) {
        (true, _) => {}
        (false, true) => return Ok(Some("pre_migration".to_string())),
        (false, false) => return Ok(None),
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
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(Some("000_init".to_string())),
        Err(e) => Err(e),
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
    SQLiteVersion {
        name: &'static str,
        min_version: &'static str,
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
            MigrationError::SQLiteVersion { min_version, name } => {
                write!(
                    f,
                    "Migration {name} requires SQLite version {min_version}. Consider compiling with the bundled_sqlite feature."
                )
            }
        }
    }
}

static ALTER_COLUMN: &str = "ALTER COLUMN";

impl MigrationError {
    fn from_migration(migration: &Migration, error: rusqlite::Error) -> MigrationError {
        match error {
            rusqlite::Error::SqlInputError {
                msg, offset, sql, ..
            } => {
                let offset = offset as usize;
                if &sql[offset..offset + ALTER_COLUMN.len()] == ALTER_COLUMN {
                    return MigrationError::SQLiteVersion {
                        name: migration.name,
                        min_version: "3.53.0",
                    };
                }

                let index = migration.queries.find(&sql).unwrap_or(0) + offset;
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
    use super::{
        MIGRATIONS, Migration, SCHEMA, is_latest_migration, last_migration, migrate,
        migrate_fresh_db,
    };
    use rusqlite::Connection;

    #[test]
    fn schema_and_latest_migration_have_the_same_name() {
        assert_eq!(SCHEMA.name, MIGRATIONS.iter().last().unwrap().name);
    }

    #[test]
    fn last_migration_detects_new_db() {
        let db = in_memory_db();
        let last_migration_name = last_migration(&db);
        assert_eq!(Ok(None), last_migration_name);
    }

    #[test]
    fn last_migration_detects_legacy_db() {
        let db = in_memory_db();
        apply_schema_pre_migration(&db).expect("couldn't apply schema");
        let last_migration_name = last_migration(&db);
        assert_eq!(Ok(Some("pre_migration".to_string())), last_migration_name);
    }

    #[test]
    fn last_migration_returns_000_init() {
        let mut db = in_memory_db();
        apply_schema_000_init(&mut db).expect("couldn't apply schema");
        let last_migration_name = last_migration(&db);
        assert_eq!(Ok(Some("000_init".to_string())), last_migration_name);
    }

    #[test]
    fn last_migration_returns_name_of_last_migration() {
        let db = in_memory_db();
        db.execute("CREATE TABLE metric_values (thing ANY);", ())
            .unwrap();
        let expected_name = "5678_cool_migration";
        db.execute(
            "CREATE TABLE grafbabe_migrations (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL
            ) STRICT;",
            (),
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
    fn is_latest_migration_detects_pre_migration_database() {
        assert!(!is_latest_migration("pre_migration"));
    }

    #[test]
    fn is_latest_migration_detects_pending_migrations() {
        assert!(!is_latest_migration("000_init"));
    }

    #[test]
    fn is_latest_migration_detects_up_to_date_migration() {
        assert!(is_latest_migration("001_change_values_to_real"));
    }

    #[test]
    fn is_latest_migration_detects_unknown_migration() {
        assert!(is_latest_migration("3456_unknown_migration"));
    }

    #[test]
    fn migrate_from_fresh_db() {
        let mut db = in_memory_db();
        migrate_fresh_db(&mut db).expect("couldn't perform fresh migration");
        let inserted_migration_names = read_migration_names(&db).unwrap();
        assert_eq!(
            vec![MIGRATIONS[MIGRATIONS.len() - 1].name.to_string()],
            inserted_migration_names
        );
    }

    #[test]
    fn migrate_from_pre_migration() {
        let mut db = in_memory_db();
        apply_schema_pre_migration(&db).expect("couldn't apply schema");

        migrate(&mut db, "pre_migration").expect("migrations failed");

        let inserted_migration_names =
            read_migration_names(&db).expect("couldn't read migration names");

        let all_migration_names: Vec<String> =
            MIGRATIONS.iter().map(|m| m.name.to_string()).collect();

        assert_eq!(all_migration_names, inserted_migration_names);
    }

    #[test]
    fn migrate_from_000_init() {
        let mut db = in_memory_db();
        apply_schema_000_init(&mut db).expect("couldn't apply schema");
        migrate(&mut db, "000_init").expect("migrations failed");

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

    fn apply_schema_pre_migration(db: &Connection) -> rusqlite::Result<()> {
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

    fn apply_schema_000_init(db: &mut Connection) -> rusqlite::Result<()> {
        let schema = Migration {
            from: "",
            name: "000_init",
            queries: "CREATE TABLE IF NOT EXISTS metrics (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                kind INTEGER NOT NULL DEFAULT 0,
                help TEXT
            ) STRICT;

            CREATE UNIQUE INDEX IF NOT EXISTS metrics_by_name ON metrics(name);

            CREATE TABLE IF NOT EXISTS labels (
                id INTEGER PRIMARY KEY,
                label TEXT NOT NULL
            ) STRICT;

            CREATE UNIQUE INDEX IF NOT EXISTS labels_by_label ON labels(label);

            CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY,
                timestamp INTEGER NOT NULL
            ) STRICT;

            CREATE UNIQUE INDEX IF NOT EXISTS events_by_timestamp ON events(timestamp);

            CREATE TABLE IF NOT EXISTS metric_values (
                metric_id INTEGER NOT NULL REFERENCES metrics(id) ON DELETE CASCADE,
                label_id INTEGER REFERENCES labels(id) ON DELETE CASCADE,
                event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
                value INTEGER NOT NULL
            ) STRICT;

            CREATE INDEX IF NOT EXISTS metric_values_by_event_id ON metric_values(event_id);

            CREATE TABLE grafbabe_migrations (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL
            ) STRICT;",
        };

        let transaction = db.transaction()?;
        schema.execute(&transaction)?;
        transaction.commit()
    }
}
