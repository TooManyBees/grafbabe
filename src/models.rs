use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Series {
    pub name: String,
    pub label: Option<String>,
    // Optional to accomodate for the fact that a metric might not exist for every time slice
    pub values: Vec<Option<i64>>,
}

impl Series {
    pub fn new(name: String, label: Option<String>, capacity: usize) -> Self {
        Series {
            name,
            label,
            values: Vec::with_capacity(capacity),
        }
    }

    pub fn push(&mut self, value: Option<i64>) {
        self.values.push(value)
    }
}

#[derive(Debug, Serialize)]
pub struct Metrics {
    pub timestamps: Vec<i64>,
    pub series: Vec<Series>,
}

const EVENT_DURATION_MINS: usize = 1;
const TOTAL_SAMPLES_15M: usize = 15 / EVENT_DURATION_MINS;
const TOTAL_SAMPLES_30M: usize = 30 / EVENT_DURATION_MINS;
const TOTAL_SAMPLES_1H: usize = 60 / EVENT_DURATION_MINS;
const TOTAL_SAMPLES_4H: usize = TOTAL_SAMPLES_1H * 4;
const TOTAL_SAMPLES_12H: usize = TOTAL_SAMPLES_1H * 12;
const TOTAL_SAMPLES_DAY: usize = TOTAL_SAMPLES_1H * 24;
const TOTAL_SAMPLES_WEEK: usize = TOTAL_SAMPLES_DAY * 7;
const TOTAL_SAMPLES_MONTH: usize = TOTAL_SAMPLES_DAY * 30;

#[derive(Copy, Clone, Debug)]
pub enum Window {
    QuarterHour,
    HalfHour,
    Hour,
    Hour4,
    Hour12,
    Day,
    Week,
    Month,
}

impl Window {
    pub fn total_samples(self) -> usize {
        match self {
            Window::QuarterHour => TOTAL_SAMPLES_15M,
            Window::HalfHour => TOTAL_SAMPLES_30M,
            Window::Hour => TOTAL_SAMPLES_1H,
            Window::Hour4 => TOTAL_SAMPLES_4H,
            Window::Hour12 => TOTAL_SAMPLES_12H,
            Window::Day => TOTAL_SAMPLES_DAY,
            Window::Week => TOTAL_SAMPLES_WEEK,
            Window::Month => TOTAL_SAMPLES_MONTH,
        }
    }
}
