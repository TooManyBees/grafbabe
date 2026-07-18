mod database;
mod http_handler;
mod models;

use crate::models::Window;
use prometheus_scraper::borrowed::MetricFamily;
use prometheus_scraper::{Format, ParseError, TextFormat, parse_payload};
use rusqlite::Connection;
use std::fs::File;
use std::io::Read;

fn parse_prometheus<'a>(s: &'a str) -> Result<Vec<MetricFamily<'a>>, ParseError> {
    parse_payload(s.as_bytes(), Format::Text(TextFormat::Prometheus)).collect()
}

fn main() {
    let mut f = File::open("./prometheus.txt").unwrap();
    let mut s = String::new();
    f.read_to_string(&mut s).unwrap();
    let metrics = parse_prometheus(&s).unwrap();

    let mut connection = database::get_connection("./pview.db3").unwrap();
    database::init_database(&connection).unwrap();

    database::store_snapshot(&mut connection, &metrics).unwrap();
}
