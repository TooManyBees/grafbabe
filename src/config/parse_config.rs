use super::parse_ini::parse_ini;
use super::{Command, Config, ConfigError, InfoCommand};
use std::path::PathBuf;

pub fn parse_config() -> Result<Config, ConfigError> {
    let mut config_file: Option<PathBuf> = None;

    let mut args = std::env::args().skip(1).peekable();

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
                "serve" => match args.next_if(|next| !next.starts_with("-")) {
                    Some(other) if other == "live" => command = Command::ServeLive,
                    Some(other) => {
                        return Err(ConfigError::UnrecognizedCommand(format!("serve {other}")));
                    }
                    None => command = Command::Serve,
                },
                _ => return Err(ConfigError::UnrecognizedCommand(arg)),
            },
            _ => return Err(ConfigError::UnrecognizedArgument(arg)),
        }
    }

    if let Some(path) = config_file {
        parse_ini(&path)
            .map_err(|e| ConfigError::ParseError(path, e))
            .map(|mut config| {
                // Remove config file's frontend_dir, unless serving live
                if cfg!(serve_live_in_release) && command != Command::ServeLive {
                    config.frontend_dir = None;
                }
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
