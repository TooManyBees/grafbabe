mod config;
mod database;
mod models;
mod serve_http;

use crate::config::{
    Command, ConfigError, InfoCommand, init_logger, parse_config, usage, version, version_more,
};
#[cfg(feature = "mock_data")]
use crate::database::seed_database;
use crate::serve_http::serve_http;
#[cfg(feature = "mock_data")]
use crate::serve_http::serve_mock_http;
use log::kv::Value;
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
                ConfigError::ParseError(path, error) => {
                    let absolute_path = absolute(&path).unwrap_or(path.into());
                    let path_str = absolute_path.to_string_lossy();
                    eprintln!("could not parse config at {path_str}: {error}");
                }
                _ => {
                    eprintln!("Could not parse arguments: {error}\n");
                    usage();
                }
            }
            std::process::exit(1);
        }
    };

    if let Some(info_command) = config.info_command {
        match info_command {
            InfoCommand::Usage => usage(),
            InfoCommand::Version => version(),
            InfoCommand::VersionMore => version_more(),
        }
        std::process::exit(0);
    }

    init_logger(&config);

    log::debug!(
        listen_addrs:? = config.listen_addrs,
        prometheus_addr:% = config.prometheus_addr,
        frontend_dir = config.frontend_dir.as_deref().map(Value::from).unwrap_or(Value::null()),
        poll_rate_mins:% = config.poll_rate_mins,
        state_location:% = config.state_location.to_string_lossy(),
        log_level:% = config.log_level,
        log_format:? = config.log_format;
        "Using config",
    );

    let database_path = config.database_path();

    let connection = database::get_connection(database_path).unwrap();
    database::init_database(&connection).unwrap();

    match config.command {
        #[cfg(feature = "mock_data")]
        Command::Seed(mock_data_path) => {
            if let Err(e) = seed_database(connection, &mock_data_path) {
                log::error!("Aborted database seed: {e}");
                std::process::exit(1);
            }
        }
        #[cfg(feature = "mock_data")]
        Command::ServeMockData(ref mock_data_path) => {
            let mock_data_path = mock_data_path.clone();
            if let Err(e) = serve_mock_http(config, connection, &mock_data_path) {
                log::error!("Aborted main loop: {e}");
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
