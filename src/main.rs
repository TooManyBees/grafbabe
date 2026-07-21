mod config;
mod database;
mod http_handler;
mod models;

use crate::config::{
    Command, Config, ConfigError, init_error_logger, init_logger, parse_config, usage,
};
use mio::{Events, Interest, Poll, Token, Waker, net::TcpListener};
use prometheus_scraper::{Format, ParseError, TextFormat, borrowed::MetricFamily, parse_payload};
use rusqlite::Connection;
use std::{
    error::Error,
    fmt,
    fs::File,
    io::{ErrorKind, Read},
    net::{SocketAddr, TcpStream},
    path::absolute,
    thread,
    time::Duration,
};
use ureq::Agent;

fn parse_prometheus<'a>(s: &'a str) -> Result<Vec<MetricFamily<'a>>, ParseError> {
    parse_payload(s.as_bytes(), Format::Text(TextFormat::Prometheus)).collect()
}

#[derive(Debug)]
enum SeedError {
    IO(std::io::Error),
    Parse(ParseError),
    DB(rusqlite::Error),
}
impl fmt::Display for SeedError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SeedError::IO(e) => e.fmt(fmt),
            SeedError::Parse(e) => e.fmt(fmt),
            SeedError::DB(e) => e.fmt(fmt),
        }
    }
}

fn seed_database(config: Config, connection: &mut Connection) -> Result<(), SeedError> {
    log::info!("Seeding database");
    // TODO: accept arbitrary text file
    let mut f = File::open("./prometheus.txt").map_err(SeedError::IO)?;
    let mut s = String::new();
    f.read_to_string(&mut s).map_err(SeedError::IO)?;
    let metrics = parse_prometheus(&s).map_err(SeedError::Parse)?;

    // for n in 0..(30 * 24 * 60) {
    //     if n % 100 == 0 {
    //         log::debug!("Seed progress: {n}");
    //     }
        database::store_snapshot(connection, &metrics).map_err(SeedError::DB)?;
    // }

    Ok(())
}

fn bind_http_listener(addr: SocketAddr) -> std::io::Result<TcpListener> {
    let http_listener = TcpListener::bind(addr).map_err(|err| {
        log::error!("Error listening on {addr}: {err}");
        err
    })?;
    log::info!("Listening on {addr}");
    Ok(http_listener)
}

fn background_loop(duration: Duration, waker: Waker) {
    std::thread::spawn(move || {
        loop {
            thread::sleep(duration);
            if let Err(e) = waker.wake() {
                log::error!("Error waking up main thread: {e}");
            }
        }
    });
}

const TOKEN_PULL_METRICS: Token = Token(0);
const TOKEN_START_HTTP: usize = 1;

fn main_loop(
    config: Config,
    mut connection: Connection,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut poll = Poll::new().map_err(|error| PollError {
        source: error,
        message: "could not create event poll",
    })?;

    let mut listeners = Vec::with_capacity(2);

    for (addr, n) in config.listen_addrs.iter().zip(TOKEN_START_HTTP..) {
        let mut listener = bind_http_listener(*addr)?;
        let token = Token(n);

        poll.registry()
            .register(&mut listener, token, Interest::READABLE)
            .map_err(|error| PollError {
                source: error,
                message: "could not register TCP listener for wakeup events",
            })?;

        listeners.push((token, listener));
    }

    let waker = Waker::new(poll.registry(), TOKEN_PULL_METRICS).map_err(|error| PollError {
        source: error,
        message: "could not register cross-thread waker for wakeup events",
    })?;

    background_loop(config.poll_rate_duration(), waker);

    let mut events = Events::with_capacity(128);
    let mut buf = [0u8; 1024 * 4];

    let http_client: Agent = Agent::config_builder()
        .user_agent("grafbabe/ureq")
        .http_status_as_error(true)
        .max_redirects(1)
        .timeout_global(Some(Duration::from_millis(500)))
        .build()
        .into();

    loop {
        poll.poll(&mut events, None)?;

        for event in &events {
            if !event.is_readable() {
                continue;
            }

            match event.token() {
                TOKEN_PULL_METRICS => {
                    if let Err(error) =
                        pull_metrics(&mut connection, &http_client, &config.prometheus_addr)
                    {
                        log::error!("{error}");
                    }
                }
                event_token => {
                    for (token, listener) in &listeners {
                        if event_token == *token {
                            while let Some(stream) = accept_tcp(listener) {
                                if let Err(error) =
                                    http_handler::handle_http(stream, &mut buf, &mut connection)
                                {
                                    log::error!("{error}");
                                }
                            }
                            break;
                        }
                    }
                }
            }
        }
    }
}

fn accept_tcp(listener: &TcpListener) -> Option<TcpStream> {
    match listener.accept().and_then(|(stream, _addr)| {
        let stream: TcpStream = stream.into();
        stream.set_nonblocking(false).map(|_| stream)
    }) {
        Ok(stream) => Some(stream),
        Err(ref e) if e.kind() == ErrorKind::WouldBlock => None,
        Err(error) => {
            log::error!("Error accepting connection: {error}");
            None
        }
    }
}

enum PullMetricsError<'url> {
    Request { url: &'url str, cause: ureq::Error },
    MalformedBody { url: &'url str, cause: ParseError },
    Database(rusqlite::Error),
}

impl<'url> fmt::Display for PullMetricsError<'url> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PullMetricsError::Request { url, cause } => {
                write!(f, "Error making request to {url}: {cause}")
            }
            PullMetricsError::MalformedBody { url, cause } => {
                write!(f, "Error parsing {url} body response: {cause}")
            }
            PullMetricsError::Database(error) => {
                write!(f, "Error writing snapshot to database: {error}")
            }
        }
    }
}

fn pull_metrics<'url>(
    connection: &mut Connection,
    http_client: &Agent,
    url: &'url str,
) -> Result<(), PullMetricsError<'url>> {
    log::debug!(prometheus_addr:% = url; "Pulling prometheus metrics");

    let body: String = http_client
        .get(url)
        .call()
        .map_err(|cause| PullMetricsError::Request { url, cause })?
        .body_mut()
        .read_to_string()
        .map_err(|cause| PullMetricsError::Request { url, cause })?;
    let snapshot =
        parse_prometheus(&body).map_err(|cause| PullMetricsError::MalformedBody { url, cause })?;
    database::store_snapshot(connection, &snapshot).map_err(PullMetricsError::Database)?;
    database::prune_old_metrics(connection).map_err(PullMetricsError::Database)?;

    Ok(())
}

fn main() {
    let config = match parse_config() {
        Ok(c) => c,
        Err(error) => {
            match error {
                ConfigError::JustPrintUsage(name) => eprintln!("{}", usage(Some(name))),
                ConfigError::ParseError(path, error) => {
                    let absolute_path = absolute(&path).unwrap_or(path.into());
                    let path_str = absolute_path.to_string_lossy();
                    eprintln!("could not parse config at {path_str}: {error}");
                }
                _ => {
                    _ = init_error_logger();
                    log::error!(error:%; "Could not parse arguments");
                    eprintln!("{error}\n\n{}", usage(None));
                }
            }
            std::process::exit(1);
        }
    };

    init_logger(config.log_level, config.log_format);

    let database_path = config.database_path();

    let mut connection = database::get_connection(database_path).unwrap();
    database::init_database(&connection).unwrap();

    match config.command {
        Command::Seed => {
            if let Err(e) = seed_database(config, &mut connection) {
                log::error!("Aborted database seed: {e}");
                std::process::exit(1);
            }
        }
        Command::Serve => {
            if let Err(e) = main_loop(config, connection) {
                log::error!("Aborted main loop: {e}");
                std::process::exit(1);
            }
        }
    }
}

#[derive(Debug)]
struct PollError {
    source: std::io::Error,
    message: &'static str,
}

impl fmt::Display for PollError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        self.message.fmt(fmt)
    }
}

impl Error for PollError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}
