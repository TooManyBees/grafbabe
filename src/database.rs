use prometheus_scraper::borrowed::MetricFamily;
use prometheus_scraper::owned::MetricType;
use rusqlite::{
    Connection,
    types::{ToSql, ToSqlOutput, Value},
};
use std::borrow::Cow;
use std::collections::HashMap;
use std::rc::Rc;

struct SqlMetricType(MetricType);
impl ToSql for SqlMetricType {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput> {
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

pub fn init_database(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute(
        "CREATE TABLE metrics (
            id INTEGER PRIMARY KEY,
            name VARCHAR(255) NOT NULL,
            kind INTEGER NOT NULL DEFAULT 0,
            help TEXT,
        );

        CREATE UNIQUE INDEX metrics_by_name ON metrics(name);

        CREATE TABLE values (
            metric_id INTEGER NOT NULL,
            value INTEGER NOT NULL,
            labels TEXT,
            timestamp BIGINT NOT NULL,
        );

        CREATE INDEX values_by_metric_id ON values(metric_id);
        CREATE INDEX values_by_timestamp ON values(timestamp);",
        (),
    )?;

    Ok(())
}

fn get_known_metrics<'a>(
    connection: &Connection,
    metrics: &[MetricFamily<'a>],
) -> rusqlite::Result<HashMap<Cow<'a, str>, i64>> {
    let metric_names: Vec<_> = metrics
        .iter()
        .map(|family| Value::from(family.name.clone().into_owned()))
        .collect();
    let mut statement = connection.prepare(
        "SELECT name, id
        FROM metrics
        WHERE name IN rarray(?1);",
    )?;
    let mut rows = statement.query([Rc::new(metric_names)])?;
    let mut known_metrics = HashMap::with_capacity(metrics.len());
    while let Some(row) = rows.next()? {
        let name: String = row.get(0)?;
        let index: i64 = row.get(1)?;
        known_metrics.insert(Cow::from(name), index);
    }

    let mut insert_statement = connection.prepare(
        "INSERT INTO metrics (name, kind, help)
        VALUES (?, ?, ?)
        RETURNING id;",
    )?;
    for metric in metrics {
        if !known_metrics.contains_key(&metric.name) {
            insert_statement.query_one(
                (
                    metric.name.clone(),
                    SqlMetricType(metric.r#type),
                    metric.help.clone(),
                ),
                |row| {
                    known_metrics.insert(metric.name.clone(), row.get(0)?);
                    Ok(())
                },
            )?;
        }
    }

    Ok(known_metrics)
}

pub fn store_snapshot(connection: &Connection, metrics: &[MetricFamily]) -> rusqlite::Result<()> {
    let known_metrics = get_known_metrics(connection, metrics)?;

    Ok(())
}

pub fn prune_old_metrics(connection: &Connection) -> rusqlite::Result<usize> {
    let mut statement = connection.prepare(
        "DELETE FROM values
        WHERE timestamp < (unixepoch('now') - 2592000);",
    )?;

    statement.execute(())
}
