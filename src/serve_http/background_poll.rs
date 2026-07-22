use crate::database;
use crate::{ParseError, parse_prometheus};
use mio::Waker;
use rusqlite::Connection;
use std::time::Duration;
use std::{fmt, thread};
use ureq::Agent;

pub fn background_loop(duration: Duration, waker: Waker) {
    std::thread::spawn(move || {
        loop {
            thread::sleep(duration);
            if let Err(e) = waker.wake() {
                log::error!("Error waking up main thread: {e}");
            }
        }
    });
}

pub fn pull_metrics<'url>(
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

pub enum PullMetricsError<'url> {
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
