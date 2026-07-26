mod logger;
mod parse_ini;
mod time;
mod version;

use log::Level;
pub use logger::init_logger;
use parse_ini::{ParseError, parse_ini};
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;
pub use version::{version, version_more};

#[derive(Debug)]
pub struct Config {
    pub command: Command,
    pub info_command: Option<InfoCommand>,
    pub listen_addrs: Vec<SocketAddr>,
    pub prometheus_addr: String,
    pub poll_rate_mins: u64,
    pub state_location: PathBuf,
    pub log_level: Level,
    pub log_format: LogFormat,
    pub log_target: LogTarget,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            command: Command::Serve,
            info_command: None,
            listen_addrs: vec![DEFAULT_LISTEN_ADDR],
            prometheus_addr: "http://localhost/metrics".to_string(),
            poll_rate_mins: 1,
            state_location: PathBuf::from("."),
            log_level: Level::Info,
            log_format: LogFormat::Plain,
            log_target: LogTarget::Stderr,
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

    pub fn poll_rate_duration(&self) -> Duration {
        Duration::from_mins(self.poll_rate_mins)
    }
}

#[derive(Clone, Debug, Default)]
pub enum Command {
    #[default]
    Serve,
    #[cfg(feature = "mock_data")]
    ServeMockData(String),
    #[cfg(feature = "mock_data")]
    Seed(String),
}

#[derive(Clone, Copy, Debug)]
pub enum InfoCommand {
    Usage,
    Version,
    VersionMore,
}

#[derive(Copy, Clone, Debug, Default)]
pub enum LogFormat {
    #[default]
    Plain,
    Pretty,
}

#[derive(Copy, Clone, Debug, Default)]
pub enum LogTarget {
    None,
    Stdout,
    #[default]
    Stderr,
}

#[derive(Debug)]
pub enum ConfigError {
    MissingArgument(String),
    #[cfg(feature = "mock_data")]
    MissingCommandArgument(String),
    UnrecognizedArgument(String),
    UnrecognizedCommand(String),
    ParseError(PathBuf, ParseError),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConfigError::MissingArgument(s) => {
                write!(f, "the flag {s} is missing its following argument")
            }
            #[cfg(feature = "mock_data")]
            ConfigError::MissingCommandArgument(s) => {
                write!(f, "the command {s} is missing its following argument")
            }
            ConfigError::UnrecognizedArgument(s) => write!(f, "unrecognized argument {s}"),
            ConfigError::UnrecognizedCommand(s) => write!(f, "unrecognized command {s}"),
            ConfigError::ParseError(p, e) => {
                write!(f, "error parsing config file {}: {e}", p.to_string_lossy())
            }
        }
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

    let mut args = std::env::args().skip(1);

    let mut command = Command::Serve;

    while args.len() > 0 {
        let arg = args.next().unwrap().to_lowercase();
        match arg.as_str() {
            flag @ "-c" | flag @ "--config-file" => match args.next() {
                Some(s) => config_file = Some(PathBuf::from(s)),
                None => return Err(ConfigError::MissingArgument(flag.to_string())),
            },
            "-h" | "--help" => {
                return Ok(Config {
                    info_command: Some(InfoCommand::Usage),
                    ..Default::default()
                });
            }
            "-v" => {
                return Ok(Config {
                    info_command: Some(InfoCommand::Version),
                    ..Default::default()
                });
            }
            "-vv" | "--version" => {
                return Ok(Config {
                    info_command: Some(InfoCommand::VersionMore),
                    ..Default::default()
                });
            }
            command_str if !command_str.starts_with("-") => match command_str {
                #[cfg(feature = "mock_data")]
                "seed" => match args.next() {
                    Some(path) => command = Command::Seed(path),
                    None => {
                        return Err(ConfigError::MissingCommandArgument(command_str.to_string()));
                    }
                },
                #[cfg(feature = "mock_data")]
                "mock" => match args.next() {
                    Some(path) => command = Command::ServeMockData(path),
                    None => {
                        return Err(ConfigError::MissingCommandArgument(command_str.to_string()));
                    }
                },
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
        Ok(Config {
            command,
            ..Default::default()
        })
    }
}

pub fn usage() {
    let name = std::env::args().next().unwrap_or(PROGRAM_NAME.to_string());
    eprintln!(
        "Usage:\t{name} [-ch]

\t-c or --config-file <PATH> (path to config file)
\t-h or --help (you're readin' it)
\t-v (print version string)
\t-vv or --version (print more detailed version)"
    )
}
