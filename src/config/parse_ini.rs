use crate::config::{Config, DEFAULT_PORT, LogFormat};
use log::Level;
use regex::Regex;
use std::{
    fmt,
    fs::File,
    io::Read,
    net::{IpAddr, SocketAddr},
    path::{Path, absolute},
    str::FromStr,
};

#[derive(Debug)]
pub enum ParseError {
    File(std::io::Error),
    Malformed(String),
    Invalid { key: &'static str, value: String },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseError::File(e) => write!(f, "could not open config file: {e}"),
            ParseError::Malformed(s) => write!(f, "malformed line in config: {s}"),
            ParseError::Invalid { key, value } => {
                write!(f, "{value} is not a valid value for {key}")
            }
        }
    }
}

pub fn parse_ini(path: &Path) -> Result<Config, ParseError> {
    let mut f = File::open(&path).map_err(ParseError::File)?;
    let mut s = String::new();
    f.read_to_string(&mut s).map_err(ParseError::File)?;

    let mut config: Config = Default::default();

    for line in s.lines() {
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        let (key, value) = match line.split_once('=') {
            Some((key, value)) => (key.trim(), value.trim()),
            None => return Err(ParseError::Malformed(line.to_string())),
        };

        if key.is_empty() || value.is_empty() {
            return Err(ParseError::Malformed(line.to_string()));
        }

        match key {
            "listen_addrs" => {
                config.listen_addrs = parse_listen_addrs(value)?;
            }
            "prometheus_addr" => {
                config.prometheus_addr = value.into(); // FIXME
            }
            "poll_rate" => {
                config.poll_rate_mins = parse_poll_rate(value)?;
            }
            "state_location" => {
                config.state_location = value.into();
            }
            "log_level" => {
                config.log_level = parse_log_level(value)?;
            }
            "log_format" => {
                config.log_format = parse_log_format(value)?;
            }
            _ => {
                let absolute_path = absolute(&path).unwrap_or(path.into());
                let path_str = absolute_path.to_string_lossy();
                log::warn!("Ignoring unknown config key in {path_str}: {key}");
            }
        }
    }

    Ok(config)
}

fn parse_listen_addrs(addrs: &str) -> Result<Vec<SocketAddr>, ParseError> {
    addrs
        .split_ascii_whitespace()
        .try_fold(Vec::with_capacity(2), |mut vec, addr| {
            let a = parse_listen_addr(addr)?;
            vec.push(a);
            Ok(vec)
        })
}

fn parse_listen_addr(addr: &str) -> Result<SocketAddr, ParseError> {
    addr.parse::<SocketAddr>()
        .or_else(|_| {
            addr.parse::<IpAddr>()
                .map(|ip| SocketAddr::new(ip, DEFAULT_PORT))
        })
        .map_err(|_| ParseError::Invalid {
            key: "listen_addrs",
            value: addr.to_string(),
        })
}

fn parse_uri(uri: &str) -> Result<&str, ParseError> {
    todo!()
}

fn parse_poll_rate(value: &str) -> Result<u64, ParseError> {
    let regex = Regex::new(r"\A(\d+)(m|h|d)\z").unwrap();
    let caps = match regex.captures(value) {
        Some(caps) => caps,
        None => {
            return Err(ParseError::Invalid {
                key: "poll_rate",
                value: value.to_string(),
            });
        }
    };

    let number = match caps
        .get(0)
        .map(|m| m.as_str())
        .and_then(|s| u64::from_str(s).ok())
    {
        Some(n) => n,
        None => {
            return Err(ParseError::Invalid {
                key: "poll_rate",
                value: value.to_string(),
            });
        }
    };

    let duration = match caps.get(1).map(|m| m.as_str()) {
        Some("m") => number,
        Some("h") => number * 60,
        Some("d") => number * 60 * 24,
        _ => {
            return Err(ParseError::Invalid {
                key: "poll_rate",
                value: value.to_string(),
            });
        }
    };

    Ok(duration)
}

fn parse_log_level(level: &str) -> Result<Level, ParseError> {
    match level.to_ascii_lowercase().as_str() {
        "error" => Ok(Level::Error),
        "warn" => Ok(Level::Warn),
        "info" => Ok(Level::Info),
        "debug" => Ok(Level::Debug),
        "trace" => Ok(Level::Trace),
        _ => Err(ParseError::Invalid {
            key: "log_level",
            value: level.to_string(),
        }),
    }
}

fn parse_log_format(format: &str) -> Result<LogFormat, ParseError> {
    match format.to_ascii_lowercase().as_str() {
        "none" => Ok(LogFormat::None),
        "plain" => Ok(LogFormat::Plain),
        "pretty" => Ok(LogFormat::Pretty),
        _ => Err(ParseError::Invalid {
            key: "log_format",
            value: format.to_string(),
        }),
    }
}
