use crate::database::now_ms;
use crate::models::{Metrics, Series, Window};
use rusqlite::{Connection, types::Value};
use std::collections::HashMap;
use std::rc::Rc;

pub fn get_metrics(
    connection: &Connection,
    num_samples: usize,
    window: Window,
) -> rusqlite::Result<Metrics> {
    let event_ids = get_event_indices_for_window(connection, num_samples, window)?;
    if event_ids.is_empty() {
        return Ok(Metrics::default());
    }
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

fn get_event_indices_for_window(
    connection: &Connection,
    num_samples: usize,
    window: Window,
) -> rusqlite::Result<Vec<i64>> {
    let (max_id, max_timestamp): (i64, i64) =
        match connection.query_one("SELECT MAX(id), timestamp FROM events;", (), |row| {
            let id = row.get(0)?;
            let ts = row.get(1)?;
            Ok((id, ts))
        }) {
            Ok(pair) => pair,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                log::warn!("No events in database yet");
                return Ok(vec![]);
            }
            Err(e) => return Err(e),
        };
    let min_timestamp = now_ms() - window.as_ms();
    if min_timestamp > max_timestamp {
        log::warn!("Search window is newer than newest event");
        return Ok(vec![]);
    }
    log::debug!(max_id, max_timestamp, min_timestamp; "Found search window");
    let min_id: i64 = connection.query_one(
        "SELECT MIN(id) FROM events WHERE timestamp >= ?",
        [min_timestamp],
        |row| row.get(0),
    )?; // TODO: log a reason why it's an error if this query returns null

    // If there are as many or fewer samples than desired in the window,
    // just return that exact range.
    if max_id - min_id + 1 <= num_samples as i64 {
        return Ok((min_id..=max_id).collect());
    }

    // FIXME: account for the possibility of a variable sample rate
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
