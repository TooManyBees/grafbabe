use crate::config::Config;
use crate::{ParseError, database, parse_prometheus};
use rusqlite::Connection;
use std::{fmt, fs::File, io::Read};

pub fn seed_database(config: Config, connection: &mut Connection) -> Result<(), SeedError> {
    log::info!("Seeding database");
    // TODO: accept arbitrary text file
    let mut f = File::open("./prometheus.txt").map_err(SeedError::IO)?;
    let mut s = String::new();
    f.read_to_string(&mut s).map_err(SeedError::IO)?;
    let metrics = parse_prometheus(&s).map_err(SeedError::Parse)?;

    // for n in 0..(30 * 24 * 60) {
    //     if n % 100 == 0 {
    //         log::debug!("Seed progress: {n}");
    //     }
    database::store_snapshot(connection, &metrics).map_err(SeedError::DB)?;
    // }

    Ok(())
}

#[derive(Debug)]
pub enum SeedError {
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
