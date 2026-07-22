mod background_poll;
mod http_handler;

use crate::config::Config;
use crate::database::get_metrics;
#[cfg(feature = "mock_data")]
use crate::database::get_mock_data;
use crate::models::{Metrics, Window};
use crate::parse_prometheus;
use crate::serve_http::background_poll::{background_loop, pull_metrics};
use mio::{Events, Interest, Poll, Token, Waker, net::TcpListener};
use rusqlite::Connection;
use std::{
    error::Error,
    fmt,
    fs::File,
    io::{ErrorKind, Read},
    net::{SocketAddr, TcpStream},
    time::Duration,
};
use ureq::Agent;

const TOKEN_PULL_METRICS: Token = Token(0);
const TOKEN_START_HTTP: usize = 1;

pub fn serve_http(
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

    let get_metrics_fn = |connection: &mut Connection,
                          num_samples: usize,
                          window: Window|
     -> Result<Metrics, rusqlite::Error> {
        get_metrics(connection, num_samples, window)
    };

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
                                if let Err(error) = http_handler::handle_http(
                                    stream,
                                    &mut buf,
                                    &mut connection,
                                    get_metrics_fn,
                                ) {
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

#[cfg(feature = "mock_data")]
pub fn serve_mock_http(
    config: Config,
    mut connection: Connection,
    mock_data_path: &str,
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

    let mut events = Events::with_capacity(128);
    let mut buf = [0u8; 1024 * 4];

    let mut f = File::open(mock_data_path)?;
    let mut s = String::new();
    f.read_to_string(&mut s)?;
    let metrics = parse_prometheus(&s)?;
    log::info!("Loaded mock data from {}", mock_data_path);

    let get_metrics_fn = |_connection: &mut Connection,
                          num_samples: usize,
                          window: Window|
     -> Result<Metrics, rusqlite::Error> {
        Ok(get_mock_data(&metrics, num_samples, window))
    };

    loop {
        poll.poll(&mut events, None)?;

        for event in &events {
            if !event.is_readable() {
                continue;
            }

            match event.token() {
                event_token => {
                    for (token, listener) in &listeners {
                        if event_token == *token {
                            while let Some(stream) = accept_tcp(listener) {
                                if let Err(error) = http_handler::handle_http(
                                    stream,
                                    &mut buf,
                                    &mut connection,
                                    get_metrics_fn,
                                ) {
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

fn bind_http_listener(addr: SocketAddr) -> std::io::Result<TcpListener> {
    let http_listener = TcpListener::bind(addr).map_err(|err| {
        log::error!("Error listening on {addr}: {err}");
        err
    })?;
    log::info!("Listening on {addr}");
    Ok(http_listener)
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
