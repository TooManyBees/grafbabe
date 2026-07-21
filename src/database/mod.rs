mod get_metrics;
mod init;
mod prune_old_metrics;
mod store_snapshot;

pub use get_metrics::get_metrics;
pub use init::init_database;
pub use prune_old_metrics::prune_old_metrics;
pub use store_snapshot::store_snapshot;

use prometheus_scraper::owned::MetricType;
use rusqlite::{
    Connection,
    types::{ToSql, ToSqlOutput, Value},
};
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

pub fn get_connection<P: AsRef<Path>>(path: P) -> rusqlite::Result<Connection> {
    let connection = Connection::open(&path)?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    rusqlite::vtab::array::load_module(&connection)?;
    Ok(connection)
}

fn now_ms() -> i64 {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("I'm not interested in running before 1970");
    now.as_millis() as i64
}
