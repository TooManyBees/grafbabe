use prometheus_scraper::borrowed::{MetricFamily, MetricValue, Counter};
use prometheus_scraper::owned::{MetricType, Number, UnsignedNumber};
use rusqlite::{
    Connection,
    types::{ToSql, ToSqlOutput, Value, Null},
};
use std::borrow::Cow;
use std::collections::HashMap;
use std::rc::Rc;
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
    connection.execute("CREATE UNIQUE INDEX IF NOT EXISTS metrics_by_name ON metrics(name);", ())?;

    connection.execute(
        "CREATE TABLE IF NOT EXISTS events (
            id INTEGER PRIMARY KEY,
            timestamp INTEGER NOT NULL
        ) STRICT;",
        (),
    )?;

    connection.execute(
        "CREATE TABLE IF NOT EXISTS metric_values (
            metric_id INTEGER NOT NULL REFERENCES metrics(id) ON DELETE CASCADE,
            event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
            value INTEGER NOT NULL,
            labels TEXT
        ) STRICT;",
        (),
    )?;
    connection.execute("CREATE INDEX IF NOT EXISTS metric_values_by_event_id ON metric_values(event_id);", ())?;

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

pub fn store_snapshot(connection: &mut Connection, snapshot: &[MetricFamily]) -> rusqlite::Result<()> {
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).expect("I'm not interested in running before 1970");
    let timestamp = now.as_millis() as i64;

    let known_metrics = get_known_metrics(connection, snapshot)?;

    let transaction = connection.transaction()?;

    let event_id: i64 = {
        let mut insert_event_statement = transaction.prepare(
            "INSERT INTO events (timestamp)
            VALUES (?)
            RETURNING id;"
        )?;

        insert_event_statement.query_one((timestamp,), |row| row.get(0))?
    };

    let mut insert_statement = transaction.prepare(
        "INSERT INTO metric_values (metric_id, event_id, value, labels)
        VALUES (?, ?, ?, ?);"
    )?;

    for family in snapshot {
        let metric_id = known_metrics[&family.name];

        for metric in &family.metric {
            let value = match metric.value {
                MetricValue::Counter(Counter { value: UnsignedNumber::Uint(n), .. }) => n as i64,
                MetricValue::Counter(Counter { value: UnsignedNumber::Float(f), .. }) => f as i64,
                MetricValue::Gauge(Number::Int(n)) => n,
                MetricValue::Gauge(Number::Float(f)) => f as i64,
                MetricValue::Untyped(Number::Int(n)) => n,
                MetricValue::Untyped(Number::Float(f)) => f as i64,
                _ => panic!("Unsupported metric to insert"),
            };

            insert_statement.execute((metric_id, value, Null, timestamp))?;
        }
    }

    std::mem::drop(insert_statement);

    transaction.commit()
}

pub fn prune_old_metrics(connection: &Connection) -> rusqlite::Result<usize> {
    let mut statement = connection.prepare(
        "DELETE FROM events
        WHERE timestamp < (unixepoch('now') - ?);",
    )?;

    const ONE_MONTH_MILLIS: i64 = 1000 * 60 * 60 * 24 * 30;

    statement.execute((ONE_MONTH_MILLIS,))
}
