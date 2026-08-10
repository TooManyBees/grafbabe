mod get_metrics;
#[cfg(feature = "mock_data")]
mod get_mock_data;
mod migrations;
mod prune_old_metrics;
#[cfg(feature = "mock_data")]
mod seed_database;
mod store_snapshot;

pub use get_metrics::get_metrics;
#[cfg(feature = "mock_data")]
pub use get_mock_data::get_mock_data;
pub use migrations::auto_migrate;
use migrations::migrate;
pub use migrations::MIGRATIONS;
pub use prune_old_metrics::prune_old_metrics;
#[cfg(feature = "mock_data")]
pub use seed_database::seed_database;
pub use store_snapshot::store_snapshot;

use prometheus_scraper::owned::MetricType;
use rusqlite::{
    Connection, ErrorCode, OpenFlags,
    types::{ToSql, ToSqlOutput, Value},
};
use std::error::Error;
use std::path::Path;
use std::time::SystemTime;

struct SqlMetricType(MetricType);
impl ToSql for SqlMetricType {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        let value: i64 = match self.0 {
            MetricType::Counter => 0,
            MetricType::Gauge => 1,
            MetricType::Summary => 2,
            MetricType::Untyped => 3,
            MetricType::Histogram => 4,
            MetricType::GaugeHistogram => 5,
            MetricType::NativeHistogram => 6,
            MetricType::HybridHistogram => 7,
            MetricType::StateSet => 8,
            MetricType::Info => 9,
        };
        Ok(ToSqlOutput::Owned(Value::Integer(value)))
    }
}

pub fn get_connection<P: AsRef<Path>>(path: P) -> Result<Connection, Box<dyn Error>> {
    let mut connection = open_or_create(&path)?;
    connection
        .as_ref()
        .pragma_update(None, "journal_mode", "WAL")?;
    connection
        .as_ref()
        .pragma_update(None, "foreign_keys", "ON")?;
    rusqlite::vtab::array::load_module(connection.as_ref())?;
    if let OpenResult::New(c) = &mut connection {
        log::debug!("Initializing new database");
        migrate(c)?;
    }
    Ok(connection.unwrap())
}

enum OpenResult {
    New(Connection),
    Existing(Connection),
}

impl OpenResult {
    fn as_ref(&self) -> &Connection {
        match self {
            OpenResult::New(c) => &c,
            OpenResult::Existing(c) => &c,
        }
    }

    fn unwrap(self) -> Connection {
        match self {
            OpenResult::New(c) => c,
            OpenResult::Existing(c) => c,
        }
    }
}

fn open_or_create<P: AsRef<Path>>(path: P) -> rusqlite::Result<OpenResult> {
    match Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => return Ok(OpenResult::Existing(c)),
        Err(e) => {
            if let Some(ErrorCode::CannotOpen) = e.sqlite_error_code() {
                // Recover from not being able to open database from lack of
                // SQLITE_OPEN_CREATE
            } else {
                return Err(e);
            }
        }
    };

    log::info!(database:% = path.as_ref().display(); "Creating new database");

    let connection = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    Ok(OpenResult::New(connection))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_else(|e| e.duration().as_millis() as i64 * -1)
}
