use crate::models::{Metrics, Series, Window};
use prometheus_scraper::borrowed::{Counter, LabelPair, MetricFamily, MetricValue};
use prometheus_scraper::owned::{MetricType, Number, UnsignedNumber};
use rusqlite::{
    Connection,
    types::{ToSql, ToSqlOutput, Value},
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

fn get_known_metrics<'a>(
    connection: &mut Connection,
    metrics: &[MetricFamily<'a>],
) -> rusqlite::Result<HashMap<Cow<'a, str>, i64>> {
    let mut known_metrics = {
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
        known_metrics
    };

    let transaction = connection.transaction()?;

    let mut insert_statement = transaction.prepare(
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

    std::mem::drop(insert_statement);

    transaction.commit()?;

    Ok(known_metrics)
}

fn labels_to_db(labels: &[LabelPair]) -> Option<String> {
    if labels.is_empty() {
        return None;
    }

    let mut parts: Vec<_> = labels
        .iter()
        .map(|l| format!("{}={}", l.name, l.value))
        .collect();
    parts.sort();

    Some(parts.join(","))
}

fn get_known_labels<'a>(
    connection: &mut Connection,
    metrics: &[MetricFamily<'a>],
) -> rusqlite::Result<HashMap<String, i64>> {
    // let mut label_identifiers: HashMap<&'a [LabelPair<'a>], String> = HashMap::new();
    // for family in metrics.iter() {
    //     for metric in family.metric.iter() {
    //         if metric.label.is_empty() {
    //             continue;
    //         }
    //         if &label_identifiers.contains_key(&metric.label) {
    //             label_identifiers.insert(metric_label, labels_to_db(&metric.label));
    //         }
    //     }
    // }

    let label_strings: Vec<_> = metrics
        .iter()
        .flat_map(|family| {
            family
                .metric
                .iter()
                .filter_map(|metric| labels_to_db(&metric.label))
        })
        .collect();
    let label_values = Rc::new(
        label_strings
            .iter()
            .map(|l| Value::from(l.clone()))
            .collect::<Vec<_>>(),
    );

    let mut known_labels = {
        let mut statement = connection.prepare(
            "SELECT label, id
            FROM labels
            WHERE label IN rarray(?1);",
        )?;
        let mut rows = statement.query([label_values])?;
        let mut known_labels = HashMap::with_capacity(label_strings.len());
        while let Some(row) = rows.next()? {
            let label: String = row.get(0)?;
            let id: i64 = row.get(1)?;
            known_labels.insert(label, id);
        }
        known_labels
    };

    let transaction = connection.transaction()?;

    let mut insert_statement = transaction.prepare(
        "INSERT INTO labels (label)
        VALUES (?)
        RETURNING id;",
    )?;
    for label_string in label_strings.into_iter() {
        if !known_labels.contains_key(&label_string) {
            insert_statement.query_one([label_string.clone()], |row| {
                known_labels.insert(label_string, row.get(0)?);
                Ok(())
            })?;
        }
    }

    std::mem::drop(insert_statement);

    transaction.commit()?;

    Ok(known_labels)
}

pub fn store_snapshot(
    connection: &mut Connection,
    snapshot: &[MetricFamily],
) -> rusqlite::Result<()> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("I'm not interested in running before 1970");
    let timestamp = now.as_millis() as i64;

    let known_metrics = get_known_metrics(connection, snapshot)?;
    let known_labels = get_known_labels(connection, snapshot)?;

    let transaction = connection.transaction()?;

    let event_id: i64 = {
        let mut insert_event_statement = transaction.prepare(
            "INSERT INTO events (timestamp)
            VALUES (?)
            RETURNING id;",
        )?;

        insert_event_statement.query_one((timestamp,), |row| row.get(0))?
    };

    let mut insert_statement = transaction.prepare(
        "INSERT INTO metric_values (metric_id, label_id, event_id, value)
        VALUES (?, ?, ?, ?);",
    )?;

    for family in snapshot {
        let metric_id = known_metrics[&family.name];

        for metric in &family.metric {
            let value = match metric.value {
                MetricValue::Counter(Counter {
                    value: UnsignedNumber::Uint(n),
                    ..
                }) => n as i64,
                MetricValue::Counter(Counter {
                    value: UnsignedNumber::Float(f),
                    ..
                }) => f as i64,
                MetricValue::Gauge(Number::Int(n)) => n,
                MetricValue::Gauge(Number::Float(f)) => f as i64,
                MetricValue::Untyped(Number::Int(n)) => n,
                MetricValue::Untyped(Number::Float(f)) => f as i64,
                _ => panic!("Unsupported metric to insert"),
            };

            let label_id = labels_to_db(&metric.label).map(|string| known_labels[&string]);

            insert_statement.execute((metric_id, label_id, event_id, value))?;
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

fn get_event_indices_for_window(
    connection: &Connection,
    num_samples: usize,
    window: Window,
) -> rusqlite::Result<Vec<i64>> {
    let max_id: i64 = connection.query_one("SELECT MAX(id) FROM events;", (), |row| row.get(0))?;

    let sample_rate: f32 = window.total_samples() as f32 / num_samples as f32;

    let mut ids = Vec::with_capacity(num_samples);
    for offset in 0..num_samples {
        let id_f = max_id as f32 - offset as f32 * sample_rate;
        let id = id_f.round() as i64;
        if id <= 0 {
            break;
        }
        ids.push(id);
    }

    ids.dedup(); // Multiple sample points might round to the same integer index
    ids.reverse(); // Sort in ascending order
    Ok(ids)
}

pub fn get_events(
    connection: &Connection,
    num_samples: usize,
    window: Window,
) -> rusqlite::Result<Metrics> {
    let event_ids = get_event_indices_for_window(connection, num_samples, window)?;
    let num_events = event_ids.len();
    let event_ids: Vec<_> = event_ids.into_iter().map(|id| Value::from(id)).collect();
    let event_ids = Rc::new(event_ids);

    let timestamps: Vec<i64> = {
        let mut statement = connection.prepare(
            "SELECT timestamp
            FROM events
            WHERE id IN rarray(?1)
            ORDER BY id;",
        )?;
        let rows = statement.query_map([event_ids.clone()], |row| row.get(0))?;
        let mut ts = Vec::with_capacity(num_events);
        for row in rows {
            ts.push(row?);
        }
        ts
    };

    let metric_ids: HashMap<(i64, Option<i64>), (String, Option<String>)> = {
        let mut statement = connection.prepare(
            "SELECT DISTINCT metrics.id AS metric_id, metrics.name AS metric_name, labels.id AS label_id, labels.label AS label
            FROM metric_values
            INNER JOIN metrics ON metric_values.metric_id = metrics.id
            LEFT JOIN labels ON metric_values.label_id = labels.id
            INNER JOIN events ON metric_values.event_id = events.id
            WHERE events.id IN rarray(?1);"
        )?;
        let mut rows = statement.query([event_ids.clone()])?;
        let mut metric_ids: HashMap<(i64, Option<i64>), (String, Option<String>)> = HashMap::new();
        while let Some(row) = rows.next()? {
            let metric_id: i64 = row.get(0)?;
            let metric_name: String = row.get(1)?;
            let label_id: Option<i64> = row.get(2)?;
            let label_name: Option<String> = row.get(3)?;
            metric_ids.insert((metric_id, label_id), (metric_name, label_name));
        }
        metric_ids
    };

    let series: Vec<_> = {
        let mut statement = connection.prepare(
            "SELECT metric_id, label_id, events.timestamp, value
            FROM events
            LEFT JOIN metric_values ON events.id = metric_values.event_id
            INNER JOIN metrics ON metrics.id = metric_values.metric_id
            WHERE event_id IN rarray(?1)
            ORDER BY timestamp;",
        )?;
        let mut rows = statement.query([event_ids])?;

        let mut events: HashMap<(String, Option<String>), Series> = HashMap::new();

        while let Some(row) = rows.next()? {
            let metric_id: i64 = row.get(0)?;
            let label_id: Option<i64> = row.get(1)?;
            let value: Option<i64> = row.get(2)?;

            let (metric_name, label_name) = metric_ids[&(metric_id, label_id)].clone();

            events
                .entry((metric_name.clone(), label_name.clone()))
                .or_insert_with(|| Series::new(metric_name, label_name, num_events))
                .push(value);
        }

        events.into_values().collect()
    };

    Ok(Metrics { timestamps, series })
}
