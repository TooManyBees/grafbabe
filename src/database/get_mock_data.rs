use crate::database::store_snapshot::{labels_to_db, metric_value};
use crate::models::{Metrics, Series, Window};
use prometheus_scraper::borrowed::MetricFamily;
use std::time::SystemTime;

pub fn get_mock_data(mock_data: &[MetricFamily], num_samples: usize, window: Window) -> Metrics {
    let now = SystemTime::now();
    let time_slice = window.duration() / num_samples as u32;
    let timestamps: Vec<_> = (0..num_samples as u32)
        .map(|n| now - time_slice * (num_samples as u32 - n))
        .map(|time| time.duration_since(SystemTime::UNIX_EPOCH).unwrap())
        .map(|duration| duration.as_millis() as i64)
        .collect();

    let series = mock_data
        .iter()
        .flat_map(|family| {
            family.metric.iter().map(|metric| {
                let value = match metric_value(&metric.value) {
                    Some(v) => v,
                    None => panic!("Unsupported metric value"),
                };

                let values = (0..num_samples as i64)
                    .map(|n| value * num_samples as i64 / (num_samples as i64 - n))
                    .map(Some)
                    .collect();

                Series {
                    name: family.name.to_string(),
                    label: labels_to_db(&metric.label),
                    values,
                }
            })
        })
        .collect();

    Metrics { timestamps, series }
}
