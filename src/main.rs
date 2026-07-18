mod database;
mod http_handler;
mod models;

use prometheus_scraper::borrowed::MetricFamily;
use prometheus_scraper::{Format, ParseError, TextFormat, parse_payload};
use rusqlite::Connection;
use std::fmt;
use std::fs::File;
use std::io::Read;

use std::net::TcpListener;

fn parse_prometheus<'a>(s: &'a str) -> Result<Vec<MetricFamily<'a>>, ParseError> {
    parse_payload(s.as_bytes(), Format::Text(TextFormat::Prometheus)).collect()
}

#[derive(Debug)]
enum SeedError {
    IO(std::io::Error),
    Parse(ParseError),
    DB(rusqlite::Error),
}
impl fmt::Display for SeedError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SeedError::IO(e) => e.fmt(fmt),
            SeedError::Parse(e) => e.fmt(fmt),
            SeedError::DB(e) => e.fmt(fmt),
        }
    }
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

fn serve_http(port: u16, mut connection: Connection) -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let mut buf = [0u8; 1024 * 4];
    for stream in listener.incoming() {
        if let Err(e) = http_handler::handle_http(stream?, &mut buf, &mut connection) {
            println!("Error serving http: {}", e);
        }
    }
    Ok(())
}

fn main() {
    let mut connection = database::get_connection("./pview.db3").unwrap();
    database::init_database(&connection).unwrap();

    match std::env::args().skip(1).next().as_deref() {
        Some("seed") => seed_database(&mut connection).unwrap(),
        Some("serve") => serve_http(3000, connection).unwrap(),
        Some(cmd) => {
            eprintln!("Unrecognized command {}", cmd);
            std::process::exit(1);
        }
        None => serve_http(3000, connection).unwrap(),
    }
}
