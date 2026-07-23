use crate::models::{Metrics, Window};
use crate::serve_http::file_contents::{FileResult, file_contents};
use httparse::{EMPTY_HEADER, Request, Status};
use rusqlite::Connection;
use std::{
    fmt,
    io::{Read, Write},
    net::TcpStream,
    path::Path,
    time::Instant,
};

pub fn handle_http<F: FnMut(&mut Connection, usize, Window) -> Result<Metrics, rusqlite::Error>>(
    mut stream: TcpStream,
    buf: &mut [u8],
    connection: &mut Connection,
    get_metrics_fn: F,
) -> Result<(), HttpError> {
    let start_time = Instant::now();

    let len = stream.read(buf).map_err(HttpError::Receive)?;
    let mut http_headers = [EMPTY_HEADER; 24];
    let mut req = Request::new(&mut http_headers);
    let _body_offset = match req.parse(&buf[..len]).map_err(HttpError::Parse)? {
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

    let if_none_match = req
        .headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case("if-none-match"))
        .and_then(|h| std::str::from_utf8(h.value).ok());

    let result = match (method, path, query) {
        ("GET", "/", _) => serve_file(stream, if_none_match, "./data/dashboard.html"),
        ("GET", "/dashboard.js", _) => serve_file(stream, if_none_match, "./data/dashboard.js"),
        ("GET", "/chart.umd.min.js", _) => {
            serve_file(stream, if_none_match, "./data/chart.umd.min.js")
        }
        ("GET", "/chart.umd.min.js.map", _) => {
            serve_file(stream, if_none_match, "./data/chart.umd.min.js.map")
        }
        ("GET", "/metrics", query) => Ok(serve_metrics(stream, query, connection, get_metrics_fn)?),
        _ => empty_http_response(stream, StatusCode::NOT_FOUND),
    };

    match result {
        Ok(status_code) => {
            let elapsed_ms = Instant::now().duration_since(start_time).as_millis();
            log::info!(method, path, status_code = status_code.as_str(), elapsed_ms; "Served response");
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

fn serve_file(
    mut stream: TcpStream,
    if_none_match: Option<&str>,
    path: &str,
) -> std::io::Result<StatusCode> {
    let (contents, etag) = match file_contents(path, if_none_match)? {
        FileResult::Found { contents, etag } => (contents, etag),
        FileResult::NotModified => return empty_http_response(stream, StatusCode::NOT_MODIFIED),
        FileResult::NotFound => return empty_http_response(stream, StatusCode::NOT_FOUND),
    };
    let bytes = contents.as_bytes();
    let status_code = StatusCode::OK;
    stream.write_fmt(format_args!(
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nEtag: {}\r\n",
        status_code.as_str(),
        status_code.reason(),
        bytes.len(),
        etag,
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

fn serve_metrics<F: FnMut(&mut Connection, usize, Window) -> Result<Metrics, rusqlite::Error>>(
    mut stream: TcpStream,
    query: Option<&str>,
    connection: &mut Connection,
    mut get_metrics_fn: F,
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
        Some(Err(_)) => {
            let _ = empty_http_response(stream, StatusCode::BAD_REQUEST);
            return Ok(StatusCode::BAD_REQUEST);
        }
    };

    let metrics = get_metrics_fn(connection, num_samples, window).map_err(HttpError::Database)?;
    let json = serde_json::to_string(&metrics).map_err(HttpError::Serde)?;
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
    NOT_MODIFIED,
}

impl StatusCode {
    fn as_str(self) -> &'static str {
        use StatusCode::*;
        match self {
            OK => "200",
            NOT_MODIFIED => "304",
            BAD_REQUEST => "400",
            NOT_FOUND => "404",
        }
    }

    fn reason(self) -> &'static str {
        use StatusCode::*;
        match self {
            OK => "Ok",
            NOT_MODIFIED => "Not Modified",
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
