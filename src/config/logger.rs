use crate::config::LogFormat;
use crate::config::time::Time;
use log::{Level, LevelFilter};
use std::io::Write;
use std::time::SystemTime;

pub fn init_logger(level: Level, format: LogFormat) {
    use env_logger::WriteStyle;

    if let LogFormat::None = format {
        return;
    }

    let mut builder = env_logger::builder();
    let logger = builder
        .filter_level(level.to_level_filter())
        .filter_module("ureq", LevelFilter::Off)
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
        })
        .write_style(WriteStyle::Never);

    match format {
        LogFormat::None => {}
        LogFormat::Pretty => {
            logger.write_style(WriteStyle::Auto).init();
        }
        LogFormat::Plain => logger.init(),
    }
}

pub fn init_error_logger() {
    init_logger(Level::Error, LogFormat::Plain)
}
