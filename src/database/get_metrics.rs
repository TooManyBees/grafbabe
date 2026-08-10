use crate::database::{IndexType, now_ms};
use crate::models::{Metrics, Series, Window};
use rusqlite::{Connection, types::Value};
use std::cmp::Ordering;
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

    let metric_ids: HashMap<(IndexType, Option<IndexType>), (String, Option<String>)> = {
        let mut statement = connection.prepare(
            "SELECT DISTINCT metrics.id AS metric_id, metrics.name AS metric_name, labels.id AS label_id, labels.label AS label
            FROM metric_values
            INNER JOIN metrics ON metric_values.metric_id = metrics.id
            LEFT JOIN labels ON metric_values.label_id = labels.id
            INNER JOIN events ON metric_values.event_id = events.id
            WHERE events.id IN rarray(?1);"
        )?;
        let mut rows = statement.query([event_ids.clone()])?;
        let mut metric_ids: HashMap<(IndexType, Option<IndexType>), (String, Option<String>)> =
            HashMap::new();
        while let Some(row) = rows.next()? {
            let metric_id: IndexType = row.get(0)?;
            let metric_name: String = row.get(1)?;
            let label_id: Option<IndexType> = row.get(2)?;
            let label_name: Option<String> = row.get(3)?;
            metric_ids.insert((metric_id, label_id), (metric_name, label_name));
        }
        metric_ids
    };

    let mut series: Vec<_> = {
        let mut statement = connection.prepare(
            "SELECT metric_id, label_id, value
            FROM events
            LEFT JOIN metric_values ON events.id = metric_values.event_id
            INNER JOIN metrics ON metrics.id = metric_values.metric_id
            WHERE event_id IN rarray(?1)
            ORDER BY timestamp;",
        )?;
        let mut rows = statement.query([event_ids])?;

        let mut events: HashMap<(String, Option<String>), Series> = HashMap::new();

        while let Some(row) = rows.next()? {
            let metric_id: IndexType = row.get(0)?;
            let label_id: Option<IndexType> = row.get(1)?;
            let value: Option<f64> = row.get(2)?;

            let (metric_name, label_name) = metric_ids[&(metric_id, label_id)].clone();

            events
                .entry((metric_name.clone(), label_name.clone()))
                .or_insert_with(|| Series::new(metric_name, label_name, num_events))
                .push(value);
        }

        events.into_values().collect()
    };

    series.sort_by(|a, b| match a.name.cmp(&b.name) {
        Ordering::Equal => a.label.cmp(&b.label),
        ordering => ordering,
    });

    Ok(Metrics { timestamps, series })
}

const ONE_MINUTE_MS: i64 = 1000 * 60;

fn get_event_indices_for_window(
    connection: &Connection,
    num_samples: usize,
    window: Window,
) -> rusqlite::Result<Vec<IndexType>> {
    let (max_id, max_timestamp): (IndexType, i64) =
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

    // seek an additional 1m back to account for the fact that the min timestamp will
    // otherwise land between the oldest event we want, and the one after it
    let min_timestamp = now_ms() - window.as_ms() - ONE_MINUTE_MS;
    if min_timestamp > max_timestamp {
        log::trace!("search window is newer than newest event, returning empty set");
        return Ok(vec![]);
    }
    let min_id: IndexType = connection.query_one(
        "SELECT MIN(id) FROM events WHERE timestamp > ?",
        [min_timestamp],
        |row| row.get(0),
    )?; // TODO: log a reason why it's an error if this query returns null

    let num_samples = if num_samples > window.total_samples() {
        log::trace!("Sample size capped at {}", window.total_samples());
        window.total_samples()
    } else {
        num_samples
    };

    log::trace!(min_id, min_timestamp, max_id, max_timestamp; "Found search window");

    // If there are as many or fewer samples than desired in the window,
    // just return that exact range.
    if max_id - min_id + 1 <= num_samples as IndexType {
        log::trace!(
            "fewer or equal events than desired samples, returning all in {}..={}",
            min_id,
            max_id
        );
        return Ok((min_id..=max_id).collect());
    }

    let ids = tween_ids(min_id, max_id, num_samples);

    Ok(ids)
}

fn tween_ids(min_id: IndexType, max_id: IndexType, num_samples: usize) -> Vec<IndexType> {
    let sample_rate = (max_id - min_id) as f32 / (num_samples - 1) as f32;
    let mut ids = Vec::with_capacity(num_samples);

    ids.push(min_id);
    for offset in (0..num_samples - 1).rev() {
        let id_f = max_id as f32 - offset as f32 * sample_rate;
        let id = id_f.round() as IndexType;
        if id <= 0 {
            break;
        }
        ids.push(id);
    }

    ids.dedup(); // Rounding to integers may produce
    ids
}

#[cfg(test)]
mod test {
    use super::tween_ids;

    #[test]
    fn tween_ids_returns_range_including_min_and_max() {
        let min_id = 5i64;
        let max_id = 10i64;
        let num_samples = 5usize;

        let result = tween_ids(min_id, max_id, num_samples);

        assert_eq!(num_samples, result.len());
        assert_eq!(min_id, result[0], "expected first element to be the min_id");
        assert_eq!(max_id, result[4], "expected last element to be the max_id");

        for window in result.windows(2) {
            let l = window[0];
            let r = window[1];
            assert!(
                l < r,
                "expected returned ids to be in ascending order\n {:?}",
                result
            );
        }
    }
}
