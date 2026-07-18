use crate::database::get_events;
use crate::models::Window;
use httparse::{EMPTY_HEADER, Request, Status};
use rusqlite::Connection;
use std::fmt;

use std::fs::File;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;

pub fn handle_http(
    mut stream: TcpStream,
    buf: &mut [u8],
    connection: &mut Connection,
) -> Result<(), HttpError> {
    let len = stream.read(buf).map_err(HttpError::Receive)?;
    let mut http_headers = [EMPTY_HEADER; 24];
    let mut req = Request::new(&mut http_headers);
    let body_offset = match req.parse(&buf[..len]).map_err(HttpError::Parse)? {
        Status::Complete(offset) => offset,
        Status::Partial => {
            empty_http_response(stream, StatusCode::BAD_REQUEST).map_err(HttpError::Respond)?;
            return Ok(());
        }
    };

    let (method, path, query) = match (req.method, req.path) {
        (None, _) | (_, None) => {
            empty_http_response(stream, StatusCode::BAD_REQUEST).map_err(HttpError::Respond)?;
            return Ok(());
        }
        (Some(method), Some(path)) => match path.split_once('?') {
            Some((p, q)) => (method, p, Some(q)),
            None => (method, path, None),
        },
    };

    let result = match (method, path, query) {
        ("GET", "/", _) => serve_file(stream, "./data/dashboard.html"),
        ("GET", "/chart.umd.min.js", _) => serve_file(stream, "./data/chart.umd.min.js"),
        ("GET", "/chart.umd.min.js.map", _) => serve_file(stream, "./data/chart.umd.min.js.map"),
        ("GET", "/metrics", query) => Ok(get_metrics(stream, query, connection)?),
        _ => empty_http_response(stream, StatusCode::NOT_FOUND),
    };

    match result {
        Ok(status_code) => {
            // TODO log something
            Ok(())
        }
        Err(e) => Err(HttpError::Respond(e)),
    }
}

fn empty_http_response(
    mut stream: TcpStream,
    status_code: StatusCode,
) -> std::io::Result<StatusCode> {
    stream.write_fmt(format_args!(
        "HTTP/1.1 {} {}\r\nConnection: close\r\n\r\n",
        status_code.as_str(),
        status_code.reason(),
    ))?;
    stream.flush()?;
    Ok(status_code)
}

fn content_type(path: &str) -> Option<&str> {
    match Path::new(path).extension().and_then(|ext| ext.to_str()) {
        Some("html") => Some("text/html;charset=utf-8"),
        Some("js") => Some("application/javascript;charset=utf-8"),
        Some("css") => Some("text/css;charset=utf-8"),
        Some(_) => None,
        None => None,
    }
}

fn serve_file(mut stream: TcpStream, path: &str) -> std::io::Result<StatusCode> {
    let mut file = File::open(path)?;
    let mut string = String::new();
    file.read_to_string(&mut string)?;
    let bytes = string.as_bytes();
    let status_code = StatusCode::OK;
    stream.write_fmt(format_args!(
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\n",
        status_code.as_str(),
        status_code.reason(),
        bytes.len(),
    ))?;
    if let Some(content_type) = content_type(path) {
        stream.write_fmt(format_args!("Content-Type: {}\r\n", content_type,))?;
    }
    stream.write(b"\r\n")?;
    stream.write(bytes)?;
    stream.flush()?;
    Ok(status_code)
}

fn parse_querystring(querystring: &str) -> Vec<(&str, &str)> {
    querystring
        .split('&')
        .map(|pair| pair.split_once('=').unwrap_or((pair, "")))
        .collect()
}

fn get_queryparam<'a, 'b>(params: Option<&[(&str, &'a str)]>, target: &'b str) -> Option<&'a str> {
    params.and_then(|params| {
        params
            .iter()
            .find_map(|(key, value)| if *key == target { Some(*value) } else { None })
    })
}

fn get_metrics(
    mut stream: TcpStream,
    query: Option<&str>,
    connection: &mut Connection,
) -> Result<StatusCode, HttpError> {
    let query = query.map(parse_querystring);
    let range = get_queryparam(query.as_deref(), "range");
    let num_samples = get_queryparam(query.as_deref(), "num_samples")
        .map(|param| usize::from_str_radix(param, 10));

    let window = match range {
        None => Window::Hour,
        Some("15m") => Window::QuarterHour,
        Some("30m") => Window::HalfHour,
        Some("1h") => Window::Hour,
        Some("4h") => Window::Hour4,
        Some("12h") => Window::Hour12,
        Some("1d") => Window::Day,
        Some("1w") => Window::Week,
        Some("1m") => Window::Month,
        _ => {
            let _ = empty_http_response(stream, StatusCode::BAD_REQUEST);
            return Ok(StatusCode::BAD_REQUEST);
        }
    };

    let num_samples = match num_samples {
        None => 100,
        Some(Ok(n)) => n,
        Some(Err(e)) => {
            let _ = empty_http_response(stream, StatusCode::BAD_REQUEST);
            return Ok(StatusCode::BAD_REQUEST);
        }
    };

    let events = get_events(connection, num_samples, window).map_err(HttpError::Database)?;
    let json = serde_json::to_string(&events).map_err(HttpError::Serde)?;
    let status_code = StatusCode::OK;

    stream.write_fmt(format_args!(
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nContent-Type: application/json;charset=utf-8\r\n\r\n{}",
        status_code.as_str(),
        status_code.reason(),
        json.len(),
        json,
    )).map_err(HttpError::Respond)?;

    Ok(StatusCode::OK)
}

#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug)]
enum StatusCode {
    OK,
    BAD_REQUEST,
    NOT_FOUND,
}

impl StatusCode {
    fn as_str(self) -> &'static str {
        use StatusCode::*;
        match self {
            OK => "200",
            BAD_REQUEST => "400",
            NOT_FOUND => "404",
        }
    }

    fn reason(self) -> &'static str {
        use StatusCode::*;
        match self {
            OK => "Ok",
            BAD_REQUEST => "Bad Request",
            NOT_FOUND => "Not Found",
        }
    }
}

impl fmt::Display for StatusCode {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{} {}", self.as_str(), self.reason())
    }
}

#[derive(Debug)]
pub enum HttpError {
    Receive(std::io::Error),
    Parse(httparse::Error),
    Database(rusqlite::Error),
    Serde(serde_json::Error),
    Respond(std::io::Error),
}

impl fmt::Display for HttpError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            HttpError::Receive(e) => write!(fmt, "Error receiving HTTP request: {}", e),
            HttpError::Parse(e) => write!(fmt, "Error parsing HTTP request: {}", e),
            HttpError::Database(e) => write!(fmt, "Error with database: {}", e),
            HttpError::Serde(e) => write!(fmt, "Error serializing JSON: {}", e),
            HttpError::Respond(e) => write!(fmt, "Error responding to HTTP request: {}", e),
        }
    }
}
