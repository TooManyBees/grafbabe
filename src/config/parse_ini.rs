use super::{Config, DEFAULT_PORT, LogFormat, LogTarget};
use http::uri::Uri;
use log::Level;
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
    Invalid {
        key: &'static str,
        value: String,
    },
    RequiresFeature {
        key: &'static str,
        value: String,
        feature: &'static str,
    },
    NotValidDir {
        key: &'static str,
        value: String,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseError::File(e) => write!(f, "could not open config file: {e}"),
            ParseError::Malformed(s) => write!(f, "malformed line in config: {s}"),
            ParseError::Invalid { key, value } => {
                write!(f, "{value:?} is not a valid value for {key}")
            }
            ParseError::RequiresFeature {
                key,
                value,
                feature,
            } => {
                write!(
                    f,
                    "the value of {key} ({value:?}) requires the feature {feature:?} to be enabled"
                )
            }
            ParseError::NotValidDir { key, value } => {
                write!(f, "could not read from directory {value:?} (value of {key}")
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
                config.prometheus_addr = parse_prometheus_addr(value)?;
            }
            "frontend_dir" => {
                if cfg!(debug_assertions) {
                    config.frontend_dir = Some(parse_frontend_dir(value)?);
                }
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
            "log_target" => {
                config.log_target = parse_log_target(value)?;
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

fn parse_prometheus_addr(uri: &str) -> Result<String, ParseError> {
    let uri = if uri.contains("://") || uri.starts_with("//") {
        uri.to_string()
    } else {
        format!("http://{uri}")
    };

    let parsed = uri.parse::<Uri>().map_err(|_| ParseError::Invalid {
        key: "prometheus_addr",
        value: uri.to_string(),
    })?;

    match parsed.scheme_str() {
        Some("http") => {}
        Some("https") => {
            if !cfg!(feature = "tls") {
                return Err(ParseError::RequiresFeature {
                    key: "prometheus_addr",
                    value: uri.to_string(),
                    feature: "tls",
                });
            }
        }
        Some(_) | None => {
            return Err(ParseError::Invalid {
                key: "prometheus_addr",
                value: uri.to_string(),
            });
        }
    }

    Ok(uri)
}

fn parse_frontend_dir(location: &str) -> Result<String, ParseError> {
    let parse_error = Err(ParseError::NotValidDir {
        key: "frontend_dir",
        value: location.to_string(),
    });
    if !Path::new(location).exists() {
        return parse_error;
    }
    let metadata = match std::fs::metadata(location) {
        Ok(metadata) => metadata,
        Err(_) => return parse_error,
    };
    if metadata.file_type().is_dir() {
        return Ok(location.into());
    } else {
        return parse_error;
    }
}

fn parse_poll_rate(value: &str) -> Result<u64, ParseError> {
    let parse_error = Err(ParseError::Invalid {
        key: "poll_rate",
        value: value.to_string(),
    });

    let boundary =
        match value.as_bytes().windows(2).enumerate().find(|(_, slice)| {
            char::from(slice[0]).is_digit(10) && !char::from(slice[1]).is_digit(10)
        }) {
            Some((idx, _)) => idx + 1,
            None => return parse_error,
        };

    let (numeric, unit) = match value.split_at_checked(boundary) {
        Some(pair) => pair,
        None => return parse_error,
    };

    let number = match u64::from_str(numeric) {
        Ok(n) => n,
        Err(_) => return parse_error,
    };

    if number == 0 {
        return parse_error;
    }

    let duration = match unit {
        "m" => number,
        "h" => number * 60,
        "d" => number * 60 * 24,
        _ => return parse_error,
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
        "plain" => Ok(LogFormat::Plain),
        "pretty" => Ok(LogFormat::Pretty),
        _ => Err(ParseError::Invalid {
            key: "log_format",
            value: format.to_string(),
        }),
    }
}

fn parse_log_target(target: &str) -> Result<LogTarget, ParseError> {
    match target.to_ascii_lowercase().as_str() {
        "none" => Ok(LogTarget::None),
        "stdout" => Ok(LogTarget::Stdout),
        "stderr" => Ok(LogTarget::Stderr),
        _ => Err(ParseError::Invalid {
            key: "log_target",
            value: target.to_string(),
        }),
    }
}
