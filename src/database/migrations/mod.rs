use rusqlite::{Batch, Connection, Transaction, fallible_iterator::FallibleIterator};
use std::error::Error;
use std::{fmt, fs, path::Path};

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

pub fn migrate(connection: &mut Connection) -> Result<(), MigrationError> {
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
    use super::{MIGRATIONS, last_migration, migrate};
    use rusqlite::Connection;

    #[test]
    fn last_migration_returns_none_on_new_db() {
        let db = in_memory_db();
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
    fn all_the_migrations_work() {
        let mut db = in_memory_db();
        migrate(&mut db).expect("migrations failed");

        let inserted_migration_names = {
            let mut statement = db
                .prepare("SELECT name FROM grafbabe_migrations ORDER BY id ASC")
                .expect("couldn't prepare statement");
            let mut rows = statement.query([]).expect("couldn't query db");
            let mut names: Vec<String> = Vec::with_capacity(MIGRATIONS.len());
            while let Some(row) = rows.next().expect("couldn't read next row") {
                names.push(row.get(0).expect("couldn't scan name from row"));
            }
            names
        };

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
}
