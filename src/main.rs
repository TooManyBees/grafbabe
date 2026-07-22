mod config;
mod database;
mod models;
mod seed_database;
mod serve_http;

use crate::config::{Command, ConfigError, init_error_logger, init_logger, parse_config, usage};
use crate::{seed_database::seed_database, serve_http::serve_http};
use prometheus_scraper::{Format, ParseError, TextFormat, borrowed::MetricFamily, parse_payload};
use std::path::absolute;

fn parse_prometheus<'a>(s: &'a str) -> Result<Vec<MetricFamily<'a>>, ParseError> {
    parse_payload(s.as_bytes(), Format::Text(TextFormat::Prometheus)).collect()
}

fn main() {
    let config = match parse_config() {
        Ok(c) => c,
        Err(error) => {
            match error {
                ConfigError::JustPrintUsage(name) => eprintln!("{}", usage(Some(name))),
                ConfigError::ParseError(path, error) => {
                    let absolute_path = absolute(&path).unwrap_or(path.into());
                    let path_str = absolute_path.to_string_lossy();
                    eprintln!("could not parse config at {path_str}: {error}");
                }
                _ => {
                    _ = init_error_logger();
                    log::error!(error:%; "Could not parse arguments");
                    eprintln!("{error}\n\n{}", usage(None));
                }
            }
            std::process::exit(1);
        }
    };

    init_logger(config.log_level, config.log_format);

    log::debug!(
        listen_addrs:? = config.listen_addrs,
        prometheus_addr:% = config.prometheus_addr,
        poll_rate_mins:% = config.poll_rate_mins,
        state_location:% = config.state_location.to_string_lossy(),
        log_level:% = config.log_level,
        log_format:? = config.log_format;
        "Using config",
    );

    let database_path = config.database_path();

    let mut connection = database::get_connection(database_path).unwrap();
    database::init_database(&connection).unwrap();

    match config.command {
        Command::Seed => {
            if let Err(e) = seed_database(config, &mut connection) {
                log::error!("Aborted database seed: {e}");
                std::process::exit(1);
            }
        }
        Command::Serve => {
            if let Err(e) = serve_http(config, connection) {
                log::error!("Aborted main loop: {e}");
                std::process::exit(1);
            }
        }
    }
}
