mod config;
mod logger;
mod parse_config;
mod parse_ini;
mod time;
mod version;

use std::path::PathBuf;

pub use config::*;
pub use logger::init_logger;
pub use parse_config::parse_config;
pub use version::{version, version_more};

pub fn usage() {
    let name = std::env::args_os()
        .next()
        .map(PathBuf::from)
        .and_then(|p| Some(p.file_name()?.to_os_string()))
        .map_or_else(
            || version::NAME.to_string(),
            |osstr| osstr.to_string_lossy().to_string(),
        );

    eprint!("Usage:\t{name} [-chvv]");
    if cfg!(any(serve_live, feature = "mock_data")) {
        eprint!(" [ serve");
        if cfg!(serve_live) {
            eprint!(" | serve live");
        }
        if cfg!(feature = "mock_data") {
            eprint!(" | mock <path> | seed <path>");
        }
        eprintln!(" ]");
    }

    eprintln!(
        "\nFlags:
\t-c or --config-file <PATH> (path to config file)
\t-h or --help (you're readin' it)
\t-v (print version string)
\t-vv or --version (print more detailed version)\n"
    );

    if cfg!(any(serve_live, feature = "mock_data")) {
        eprint!(
            "Commands:
\tserve (runs the program as normal, serving HTTP requests and polling
\t\tthe prometheus endpoint in the background; this is the default)"
        );
        if cfg!(serve_live) {
            eprint!(
                "
\tserve live (like serve, but reads files from the config file's
\t\t`frontend_dir` setting"
            );
            if cfg!(not(serve_live_in_release)) {
                eprint!("; in debug this is identical to `serve`");
            }
            eprint!(")");
        }
        if cfg!(feature = "mock_data") {
            eprint!(
                "
\tmock <path> (like serve, but reads the file at <path> and uses it as
\t\tmock data; the file must be in prometheus format; the program will
\t\tnot poll the prometheus endpoint in the background)
\tseed <path> (writes a single snapshot to the database, using
\t\tdata read from <path>; the file must be in prometheus format)"
            )
        }
        eprintln!("\n");
    }
}
