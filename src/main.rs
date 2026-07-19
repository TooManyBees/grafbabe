mod database;
mod http_handler;
mod models;

use mio::{Events, Interest, Poll, Token, Waker, net::TcpListener};
use prometheus_scraper::{Format, ParseError, TextFormat, borrowed::MetricFamily, parse_payload};
use rusqlite::Connection;
use std::{
    error::Error,
    fmt,
    fs::File,
    io::{ErrorKind, Read},
    net::{SocketAddr, TcpStream},
    thread,
    time::Duration,
};

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

fn seed_database(connection: &mut Connection) -> Result<(), SeedError> {
    let mut f = File::open("./prometheus.txt").map_err(SeedError::IO)?;
    let mut s = String::new();
    f.read_to_string(&mut s).map_err(SeedError::IO)?;
    let metrics = parse_prometheus(&s).map_err(SeedError::Parse)?;

    // for n in 0..(30 * 24 * 60) {
    //     if n % 100 == 0 {
    //         println!("{n}");
    //     }
        database::store_snapshot(connection, &metrics).map_err(SeedError::DB)?;
    // }

    Ok(())
}

fn bind_http_listener(port: u16) -> std::io::Result<TcpListener> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let http_listener = TcpListener::bind(addr).map_err(|err| {
        // log error
        err
    })?;
    // log listening
    Ok(http_listener)
}

const TOKEN_PULL_METRICS: Token = Token(0);
const TOKEN_TCP_LISTENER: Token = Token(1);

const BACKGROUND_LOOP_DURATION: Duration = Duration::from_mins(1);

fn background_loop(waker: Waker) {
    std::thread::spawn(move || {
        loop {
            thread::sleep(BACKGROUND_LOOP_DURATION);
            if let Err(e) = waker.wake() {
                println!("Error waking up main thread: {}", e);
            }
        }
    });
}

fn main_loop(mut connection: Connection) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut http_listener = bind_http_listener(3000)?;

    let mut poll = Poll::new().map_err(|error| PollError {
        source: error,
        message: "could not create event poll",
    })?;
    poll.registry()
        .register(&mut http_listener, TOKEN_TCP_LISTENER, Interest::READABLE)
        .map_err(|error| PollError {
            source: error,
            message: "could not register TCP listener for wakeup events",
        })?;
    let waker = Waker::new(poll.registry(), TOKEN_PULL_METRICS).map_err(|error| PollError {
        source: error,
        message: "could not register cross-thread waker for wakeup events",
    })?;
    background_loop(waker);

    let mut events = Events::with_capacity(128);
    let mut buf = [0u8; 1024 * 4];

    loop {
        poll.poll(&mut events, None)?;

        for event in &events {
            if !event.is_readable() {
                continue;
            }

            match event.token() {
                TOKEN_TCP_LISTENER => {
                    while let Some(stream) = accept_tcp(&http_listener) {
                        if let Err(error) =
                            http_handler::handle_http(stream, &mut buf, &mut connection)
                        {
                            // TODO log error
                        }
                    }
                }
                TOKEN_PULL_METRICS => pull_metrics(&mut connection),
                _ => {}
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
            // TODO log error
            None
        }
    }
}

fn pull_metrics(connection: &mut Connection) {
    eprintln!("TODO: pull some metrics!");
}

fn main() {
    let mut connection = database::get_connection("./pview.db3").unwrap();
    database::init_database(&connection).unwrap();

    match std::env::args().skip(1).next().as_deref() {
        Some("seed") => seed_database(&mut connection).unwrap(),
        Some("serve") => main_loop(connection).unwrap(),
        Some(cmd) => {
            eprintln!("Unrecognized command {}", cmd);
            std::process::exit(1);
        }
        None => main_loop(connection).unwrap(),
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
