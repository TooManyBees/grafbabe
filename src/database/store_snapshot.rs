use crate::database::{SqlMetricType, now_ms};
use prometheus_scraper::borrowed::{Counter, LabelPair, MetricFamily, MetricValue};
use prometheus_scraper::owned::{Number, UnsignedNumber};
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
