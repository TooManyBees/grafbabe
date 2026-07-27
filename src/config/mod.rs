mod config;
mod logger;
mod parse_config;
mod parse_ini;
mod time;
mod version;

pub use config::*;
pub use logger::init_logger;
pub use parse_config::parse_config;
pub use version::{version, version_more};

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
