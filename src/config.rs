mod logger;
mod parse_ini;
mod time;

use log::Level;
pub use logger::{init_error_logger, init_logger};
use parse_ini::{ParseError, parse_ini};
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug)]
pub enum ConfigError {
    MissingArgument(String),
    UnrecognizedArgument(String),
    UnrecognizedCommand(String),
    ParseError(PathBuf, ParseError),
    JustPrintUsage(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConfigError::MissingArgument(s) => write!(f, "{s} is missing its following argument"),
            ConfigError::UnrecognizedArgument(s) => write!(f, "unrecognized argument {s}"),
            ConfigError::UnrecognizedCommand(s) => write!(f, "unrecognized command {s}"),
            ConfigError::ParseError(p, e) => {
                write!(f, "error parsing config file {}: {e}", p.to_string_lossy())
            }
            ConfigError::JustPrintUsage(_) => write!(f, ""),
        }
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub enum Command {
    #[default]
    Serve,
    Seed,
}

#[derive(Debug)]
pub struct Config {
    pub command: Command,
    pub listen_addrs: Vec<SocketAddr>,
    pub prometheus_addr: String,
    pub poll_rate: Duration,
    pub state_location: PathBuf,
    pub log_level: Level,
    // pub logging: ,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            command: Command::Serve,
            listen_addrs: vec![DEFAULT_LISTEN_ADDR],
            prometheus_addr: "http://localhost/metrics".to_string(),
            poll_rate: Duration::from_mins(1),
            state_location: PathBuf::from("."),
            log_level: Level::Info,
        }
    }
}

impl Config {
    pub fn database_name(&self) -> PathBuf {
        Path::new(PROGRAM_NAME).with_added_extension("db3")
    }

    pub fn database_path(&self) -> PathBuf {
        self.state_location.join(self.database_name())
    }
}

const PROGRAM_NAME: &'static str = "grafbabe";
const DEFAULT_PORT: u16 = 4242;
const DEFAULT_LISTEN_ADDR: SocketAddr = if cfg!(debug_assertions) {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), DEFAULT_PORT)
} else {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), DEFAULT_PORT)
};

pub fn parse_config() -> Result<Config, ConfigError> {
    let mut config_file: Option<PathBuf> = None;

    let mut args = std::env::args();
    let binary_name = args.next().unwrap_or(PROGRAM_NAME.to_string());

    let mut command = Command::Serve;

    while args.len() > 0 {
        let arg = args.next().unwrap().to_lowercase();
        match arg.as_str() {
            flag @ "-c" | flag @ "--config-file" => match args.next() {
                Some(s) => config_file = Some(PathBuf::from(s)),
                None => return Err(ConfigError::MissingArgument(flag.to_string())),
            },
            "-h" | "--help" => {
                return Err(ConfigError::JustPrintUsage(binary_name));
            }
            command_str if !command_str.starts_with("-") => match command_str {
                "seed" => command = Command::Seed,
                "serve" => command = Command::Serve,
                _ => return Err(ConfigError::UnrecognizedCommand(arg)),
            },
            _ => return Err(ConfigError::UnrecognizedArgument(arg)),
        }
    }

    if let Some(path) = config_file {
        parse_ini(&path)
            .map_err(|e| ConfigError::ParseError(path, e))
            .map(|mut config| {
                config.command = command;
                config
            })
    } else {
        Ok(Config::default())
    }
}

pub fn usage(name: Option<String>) -> String {
    let name = match name {
        Some(n) => n,
        None => match std::env::args().next() {
            Some(name) => name,
            None => PROGRAM_NAME.to_string(),
        },
    };
    format!(
        "Usage:\t{name} [-ch]

\t -c or --config-file <PATH> path to config file
\t\tdefault: /var/lib/{PROGRAM_NAME}/{PROGRAM_NAME}.ini
\t -h or --help (you're readin' it)"
    )
}
