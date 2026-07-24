use crate::config::time::Time;
use crate::config::{Config, LogFormat, LogTarget};
use log::LevelFilter;
use std::io::Write;
use std::time::SystemTime;

pub fn init_logger(config: &Config) {
    use env_logger::{Target, WriteStyle};

    let target = match config.log_target {
        LogTarget::None => return,
        LogTarget::Stdout => Target::Stdout,
        LogTarget::Stderr => Target::Stderr,
    };

    let mut builder = env_logger::builder();
    let logger = builder
        .target(target)
        .filter_level(config.log_level.to_level_filter())
        .filter_module("ureq", LevelFilter::Off)
        .filter_module("rustls", LevelFilter::Off)
        .format_target(false)
        .format(|formatter, record| {
            let now = SystemTime::now();
            let t = Time::from(now);
            let level = record.level();
            let level_style = formatter.default_level_style(level);
            let target = record.target();
            let args = record.args();
            write!(
                formatter,
                "{t} [{level_style}{level:<5}{level_style:#} {target}] {args}"
            )?;

            env_logger::fmt::default_kv_format(formatter, record.key_values())?;
            write!(formatter, "\n")
        });

    match config.log_format {
        LogFormat::Pretty => logger.write_style(WriteStyle::Auto).init(),
        LogFormat::Plain => logger.write_style(WriteStyle::Never).init(),
    }
}
