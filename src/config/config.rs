use super::parse_ini::ParseError;
use super::version;
use log::Level;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const DEFAULT_PORT: u16 = 4242;
const DEFAULT_LISTEN_ADDR: SocketAddr = if cfg!(debug_assertions) {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), DEFAULT_PORT)
} else {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), DEFAULT_PORT)
};

#[derive(Debug)]
pub struct Config {
    pub command: Command,
    pub info_command: Option<InfoCommand>,
    pub listen_addrs: Vec<SocketAddr>,
    pub frontend_dir: Option<String>,
    pub prometheus_addr: String,
    pub poll_rate_mins: u64,
    pub state_location: PathBuf,
    pub database_name: String,
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
            frontend_dir: if cfg!(debug_assertions) {
                Some("frontend".to_string())
            } else {
                None
            },
            prometheus_addr: "http://localhost/metrics".to_string(),
            poll_rate_mins: 1,
            state_location: PathBuf::from("."),
            database_name: version::NAME.into(),
            log_level: Level::Info,
            log_format: LogFormat::Plain,
            log_target: LogTarget::Stderr,
        }
    }
}

impl Config {
    pub fn database_name(&self) -> PathBuf {
        Path::new(&self.database_name).with_added_extension("db3")
    }

    pub fn database_path(&self) -> PathBuf {
        self.state_location.join(self.database_name())
    }

    pub fn poll_rate_duration(&self) -> Duration {
        Duration::from_mins(self.poll_rate_mins)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum Command {
    #[default]
    Serve,
    ServeLive,
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
    #[cfg(feature = "systemd_journal")]
    SystemdJournal,
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
                write!(f, "the flag {s:?} is missing its following argument")
            }
            #[cfg(feature = "mock_data")]
            ConfigError::MissingCommandArgument(s) => {
                write!(f, "the command {s:?} is missing its following argument")
            }
            ConfigError::UnrecognizedArgument(s) => write!(f, "unrecognized argument {s:?}"),
            ConfigError::UnrecognizedCommand(s) => write!(f, "unrecognized command {s:?}"),
            ConfigError::ParseError(p, e) => {
                write!(f, "error parsing config file {}: {e}", p.to_string_lossy())
            }
        }
    }
}
