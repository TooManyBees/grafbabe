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

enum SeedError {
    IO(std::io::Error),
    Parse(ParseError),
    DB(rusqlite::Error),
}
fn seed_database(connection: &mut Connection) -> Result<(), SeedError> {
    let mut f = File::open("./prometheus.txt").map_err(SeedError::IO)?;
    let mut s = String::new();
    f.read_to_string(&mut s).map_err(SeedError::IO)?;
    let metrics = parse_prometheus(&s).map_err(SeedError::Parse)?;

    // for n in 0..(30 * 24 * 60) {
    //     if n % 100 == 0 {
    //         println!("{n}");
    //     }
        database::store_snapshot(connection, &metrics).map_err(SeedError::DB)?;
    // }

    Ok(())
}

fn main() {
    let mut connection = database::get_connection("./pview.db3").unwrap();
    database::init_database(&connection).unwrap();

    seed_database(&mut connection).unwrap();
}
