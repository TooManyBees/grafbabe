use crate::database::store_snapshot::{SimplifiedValue, labels_to_db, metric_value};
use crate::models::{Metrics, Series, Window};
use prometheus_scraper::borrowed::MetricFamily;
use std::time::{Duration, SystemTime};

pub fn get_mock_data(mock_data: &[MetricFamily], num_samples: usize, window: Window) -> Metrics {
    let now = SystemTime::now();
    let num_samples = num_samples.min(window.total_samples());
    let time_slice = window.duration() / num_samples as u32;
    let timestamps: Vec<_> = (0..num_samples as u32)
        .map(|n| now - time_slice * (num_samples as u32 - n))
        .map(|time| time.duration_since(SystemTime::UNIX_EPOCH).unwrap())
        .map(|duration| duration.as_millis() as i64)
        .rev()
        .collect();

    let fraction = window
        .duration()
        .div_duration_f64(Duration::from_hours(24 * 30));

    let series = mock_data
        .iter()
        .flat_map(|family| {
            family
                .metric
                .iter()
                .filter(|metric| match metric_value(&metric.value) {
                    Ok(_) => true,
                    Err(t) => {
                        log::warn!(metric_type:? = t; "Skipping unsupported metric type");
                        false
                    }
                })
                .flat_map(|metric| {
                    let name = family.name.to_string();
                    let label = labels_to_db(&metric.label);
                    let iter: Box<dyn Iterator<Item = Series>> = match metric_value(&metric.value) {
                        Ok(SimplifiedValue::Single(f)) => {
                            let values = extrapolate_from_value(fraction, f, num_samples);
                            let series = Series {
                                name,
                                label,
                                values,
                            };
                            Box::new(std::iter::once(series))
                        }
                        Ok(SimplifiedValue::Histogram(buckets)) => {
                            Box::new(buckets.map(move |(value, le)| {
                                let values = extrapolate_from_value(fraction, value, num_samples);
                                let le_label = format!("le={le}");
                                Series {
                                    name: name.clone(),
                                    label: label
                                        .as_ref()
                                        .map(|l| format!("{l},{le_label}"))
                                        .or(Some(le_label)),
                                    values,
                                }
                            }))
                        }
                        Err(_) => unreachable!("already filtered out unsupported value types"),
                    };
                    iter
                })
        })
        .collect();

    Metrics { timestamps, series }
}

fn extrapolate_from_value(fraction: f64, value: f64, num_samples: usize) -> Vec<Option<f64>> {
    let step_amount = (fraction * value) / num_samples as f64;

    (0..num_samples as u64)
        .map(|n| value - step_amount * (n as f64))
        .map(Some)
        .collect()
}
