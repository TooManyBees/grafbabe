use crate::database::{IndexType, now_ms};
use prometheus_scraper::borrowed::{Counter, Info, LabelPair, MetricFamily, MetricValue};
use prometheus_scraper::owned::{MetricType, Number, UnsignedNumber};
use rusqlite::{Connection, types::Value};
use std::borrow::Cow;
use std::collections::HashMap;
use std::rc::Rc;

pub fn store_snapshot(
    connection: &mut Connection,
    snapshot: &[MetricFamily],
) -> rusqlite::Result<()> {
    let timestamp = now_ms();

    let known_metrics = get_known_metrics(connection, snapshot)?;
    let known_labels = get_known_labels(connection, snapshot)?;

    let transaction = connection.transaction()?;

    let event_id: IndexType = {
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
        // A metric would not be in known_metrics if it was skipped by
        // get_known_metrics because it was an unsupported type.
        let metric_id = match known_metrics.get(&family.name) {
            Some(id) => id,
            None => continue,
        };

        for metric in &family.metric {
            let value = match metric_value(&metric.value) {
                Ok(v) => v,
                Err(t) => {
                    log::warn!(metric_type:? = t; "Skipping unsupported metric type");
                    continue;
                }
            };

            let label_id = labels_to_db(&metric.label).map(|string| known_labels[&string]);

            insert_statement.execute((metric_id, label_id, event_id, value))?;
        }
    }

    std::mem::drop(insert_statement);

    transaction.commit()?;

    let num_metrics = snapshot.len();
    let num_datapoints = snapshot
        .iter()
        .fold(0, |sum, family| sum + family.metric.len());

    log::debug!(timestamp, event_id; "Stored snapshot: {} data points for {} metrics", num_datapoints, num_metrics);

    Ok(())
}

/// The metric types supported by prometheus_scraper, which aren't
/// supported by grafbabe.
#[derive(Copy, Clone, Debug)]
pub enum UnsupportedMetricType {
    Summary,
    Histogram,
    GaugeHistogram,
    NativeHistogram,
    HybridHistogram,
    StateSet,
}

pub fn metric_type(t: MetricType) -> Result<i64, UnsupportedMetricType> {
    match t {
        MetricType::Counter => Ok(0),
        MetricType::Gauge => Ok(1),
        MetricType::Summary => Ok(2),
        MetricType::Untyped => Ok(3),
        MetricType::Histogram => Err(UnsupportedMetricType::Histogram), // 4
        MetricType::GaugeHistogram => Err(UnsupportedMetricType::GaugeHistogram), // 5
        MetricType::NativeHistogram => Err(UnsupportedMetricType::NativeHistogram), // 6
        MetricType::HybridHistogram => Err(UnsupportedMetricType::HybridHistogram), // 7
        MetricType::StateSet => Err(UnsupportedMetricType::StateSet),   // 8
        MetricType::Info => Ok(9),
    }
}

pub fn metric_value(v: &MetricValue) -> Result<f64, UnsupportedMetricType> {
    match v {
        MetricValue::Counter(Counter {
            value: UnsignedNumber::Uint(n),
            ..
        }) => Ok(*n as f64),
        MetricValue::Counter(Counter {
            value: UnsignedNumber::Float(f),
            ..
        }) => Ok(*f),
        MetricValue::Gauge(Number::Int(n)) => Ok(*n as f64),
        MetricValue::Gauge(Number::Float(f)) => Ok(*f),
        MetricValue::Untyped(Number::Int(n)) => Ok(*n as f64),
        MetricValue::Untyped(Number::Float(f)) => Ok(*f),
        MetricValue::Summary(_) => Err(UnsupportedMetricType::Summary),
        MetricValue::Histogram(_) => Err(UnsupportedMetricType::Histogram),
        MetricValue::GaugeHistogram(_) => Err(UnsupportedMetricType::GaugeHistogram),
        MetricValue::NativeHistogram(_) => Err(UnsupportedMetricType::NativeHistogram),
        MetricValue::HybridHistogram { .. } => Err(UnsupportedMetricType::HybridHistogram),
        MetricValue::StateSet(_) => Err(UnsupportedMetricType::StateSet),
        MetricValue::Info(_) => Ok(1f64),
    }
}

fn get_known_metrics<'a>(
    connection: &mut Connection,
    metrics: &[MetricFamily<'a>],
) -> rusqlite::Result<HashMap<Cow<'a, str>, IndexType>> {
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
            let index: IndexType = row.get(1)?;
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
            let metric_type_value = match metric_type(metric.r#type) {
                Ok(v) => v,
                Err(t) => {
                    log::warn!(metric_type:? = t; "Skipping unsupported metric type");
                    continue;
                }
            };

            insert_statement.query_one(
                (metric.name.clone(), metric_type_value, metric.help.clone()),
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

pub fn labels_to_db(labels: &[LabelPair]) -> Option<String> {
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
) -> rusqlite::Result<HashMap<String, IndexType>> {
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
                .filter_map(|metric| match &metric.value {
                    MetricValue::Info(Info { labels }) => labels_to_db(labels),
                    _ => labels_to_db(&metric.label),
                })
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
            let id: IndexType = row.get(1)?;
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
