mod database;

use std::fs::File;
use std::io::Read;

use prometheus_scraper::borrowed::MetricFamily;
use prometheus_scraper::{Format, ParseError, TextFormat, parse_payload};

use rusqlite::{Connection};

fn parse_prometheus<'a>(s: &'a str) -> Result<Vec<MetricFamily<'a>>, ParseError> {
    parse_payload(s.as_bytes(), Format::Text(TextFormat::Prometheus)).collect()
}

fn main() {
    let mut f = File::open("./prometheus.txt").unwrap();
    let mut s = String::new();
    f.read_to_string(&mut s).unwrap();
    let metrics = parse_prometheus(&s).unwrap();
    println!("{:?}", metrics);

    let connection = Connection::open("./pview.db3").unwrap();
    rusqlite::vtab::array::load_module(&connection).unwrap();

    let known_metrics = database::store_snapshot(&connection, &metrics).unwrap();
    println!("{:?}", known_metrics);
}
