use super::config::{Config, LogFormat, LogTarget};
use super::time::Time;
use env_logger::fmt::{Formatter, Target, WriteStyle};
use log::{LevelFilter, kv};
use std::time::SystemTime;
use std::{io, io::Write};

pub fn init_logger(config: &Config) {
    let target = match config.log_target {
        LogTarget::None => return,
        LogTarget::Stdout => Target::Stdout,
        LogTarget::Stderr => Target::Stderr,
    };
    let log_format = config.log_format;

    let mut builder = env_logger::builder();
    let logger = builder
        .target(target)
        .filter_level(config.log_level.to_level_filter())
        .filter_module("ureq", LevelFilter::Off)
        .filter_module("rustls", LevelFilter::Off)
        .filter_module("mio", LevelFilter::Off)
        .format(move |formatter, record| {
            let now = SystemTime::now();
            let t = Time::from(now);
            let level = record.level();
            let target = record.target();
            let args = record.args();
            #[cfg(feature = "color")]
            let level_style = formatter.default_level_style(level);
            #[cfg(feature = "color")]
            write!(
                formatter,
                "{t} [{level_style}{level:<5}{level_style:#} {target}] {args}"
            )?;
            #[cfg(not(feature = "color"))]
            write!(formatter, "{t} [{level:<5} {target}] {args}")?;

            record
                .key_values()
                .visit(&mut Visitor {
                    format: log_format,
                    formatter,
                })
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            write!(formatter, "\n")
        });

    match config.log_format {
        LogFormat::Pretty => logger.write_style(WriteStyle::Auto).init(),
        LogFormat::Plain => logger.write_style(WriteStyle::Never).init(),
    }
}

static NEEDS_ESCAPE_PRETTY: &'static [char] = &[' ', '"', '\\'];
static NEEDS_ESCAPE_PLAIN: &'static [char] = &[' ', '"', '\\', '\n'];

struct Visitor<'a> {
    format: LogFormat,
    formatter: &'a mut Formatter,
}

impl<'kvs> kv::VisitSource<'kvs> for Visitor<'_> {
    fn visit_pair(&mut self, key: kv::Key<'_>, value: kv::Value<'kvs>) -> Result<(), kv::Error> {
        match self.format {
            LogFormat::Plain => write_plain(self.formatter, key, value)?,
            LogFormat::Pretty => write_pretty(self.formatter, key, value)?,
        }
        Ok(())
    }
}

fn write_plain(w: &mut Formatter, key: kv::Key<'_>, value: kv::Value) -> std::io::Result<()> {
    write!(w, " {key}=")?;
    let value_str = value.to_string();
    if value_str.chars().any(|c| NEEDS_ESCAPE_PLAIN.contains(&c)) {
        let escaped_value = value_str.escape_debug();
        write!(w, "\"{escaped_value}\"")
    } else {
        write!(w, "{value}")
    }
}

fn write_pretty(w: &mut Formatter, key: kv::Key<'_>, value: kv::Value) -> std::io::Result<()> {
    write!(w, "\n\t{key}: ")?;
    let value_str = value.to_string();
    if value_str.chars().any(|c| NEEDS_ESCAPE_PRETTY.contains(&c)) {
        let escaped_value = value_str.escape_debug();
        write!(w, "\"{escaped_value}\"")
    } else {
        write!(w, "{value}")
    }
}
