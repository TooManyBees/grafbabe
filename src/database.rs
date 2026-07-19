mod get_metrics;
mod prune_old_metrics;
mod store_snapshot;

pub use get_metrics::get_metrics;
pub use prune_old_metrics::prune_old_metrics;
pub use store_snapshot::store_snapshot;

use prometheus_scraper::owned::MetricType;
use rusqlite::{
    Connection,
    types::{ToSql, ToSqlOutput, Value},
};
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

pub fn get_connection(path: &str) -> rusqlite::Result<Connection> {
    let connection = Connection::open(path)?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    rusqlite::vtab::array::load_module(&connection)?;
    Ok(connection)
}

pub fn init_database(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute(
        "CREATE TABLE IF NOT EXISTS metrics (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            kind INTEGER NOT NULL DEFAULT 0,
            help TEXT
        ) STRICT;",
        (),
    )?;
    connection.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS metrics_by_name ON metrics(name);",
        (),
    )?;

    connection.execute(
        "CREATE TABLE IF NOT EXISTS labels (
            id INTEGER PRIMARY KEY,
            label TEXT NOT NULL
        ) STRICT;",
        (),
    )?;
    connection.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS labels_by_label ON labels(label);",
        (),
    )?;

    connection.execute(
        "CREATE TABLE IF NOT EXISTS events (
            id INTEGER PRIMARY KEY,
            timestamp INTEGER NOT NULL
        ) STRICT;",
        (),
    )?;
    connection.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS events_by_timestamp ON events(timestamp);",
        (),
    )?;

    connection.execute(
        "CREATE TABLE IF NOT EXISTS metric_values (
            metric_id INTEGER NOT NULL REFERENCES metrics(id) ON DELETE CASCADE,
            label_id INTEGER REFERENCES labels(id) ON DELETE CASCADE,
            event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
            value INTEGER NOT NULL
        ) STRICT;",
        (),
    )?;
    connection.execute(
        "CREATE INDEX IF NOT EXISTS metric_values_by_event_id ON metric_values(event_id);",
        (),
    )?;

    Ok(())
}

fn now_ms() -> i64 {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("I'm not interested in running before 1970");
    now.as_millis() as i64
}
