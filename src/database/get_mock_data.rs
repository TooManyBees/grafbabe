use crate::database::store_snapshot::{labels_to_db, metric_value};
use crate::models::{Metrics, Series, Window};
use prometheus_scraper::borrowed::MetricFamily;
use std::time::{Duration, SystemTime};

pub fn get_mock_data(mock_data: &[MetricFamily], num_samples: usize, window: Window) -> Metrics {
    let now = SystemTime::now();
    let time_slice = window.duration() / num_samples as u32;
    let timestamps: Vec<_> = (0..num_samples as u32)
        .map(|n| now - time_slice * (num_samples as u32 - n))
        .map(|time| time.duration_since(SystemTime::UNIX_EPOCH).unwrap())
        .map(|duration| duration.as_millis() as i64)
        .rev()
        .collect();

    let fraction = window.duration().div_duration_f32(Duration::from_hours(24 * 30));

    let series = mock_data
        .iter()
        .flat_map(|family| {
            family.metric.iter().filter_map(|metric| {
                let value = match metric_value(&metric.value) {
                    Ok(v) => v,
                    Err(t) => {
                        log::warn!(metric_type:? = t; "Skipping unsupported metric type");
                        return None;
                    }
                };

                let step_amount = (fraction * value as f32) / num_samples as f32;

                let values = (0..num_samples as i64)
                    .map(|n| (value as f32 - step_amount * (n as f32)) as i64)
                    .map(Some)
                    .collect();

                Some(Series {
                    name: family.name.to_string(),
                    label: labels_to_db(&metric.label),
                    values,
                })
            })
        })
        .collect();

    Metrics { timestamps, series }
}
