use std::borrow::Cow;
use std::io::{Error, ErrorKind, Read};

pub fn file_contents(path: &str) -> Result<Option<Cow<'static, str>>, Error> {
    #[cfg(not(feature = "include_html"))]
    let result = read_file(path);
    #[cfg(feature = "include_html")]
    let result = read_included(path);
    result
}

#[cfg(not(feature = "include_html"))]
fn read_file(path: &str) -> Result<Option<Cow<'static, str>>, Error> {
    use std::fs::File;

    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) if e.kind() == ErrorKind::IsADirectory => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut string = String::new();
    file.read_to_string(&mut string)?;
    Ok(Some(Cow::Owned(string)))
}

#[cfg(feature = "include_html")]
fn read_included(path: &str) -> Result<Option<Cow<'static, str>>, Error> {
    let found = match path {
        "./data/chart.umd.min.js" => include_str!("../../data/chart.umd.min.js"),
        "./data/chart.umd.min.js.map" => include_str!("../../data/chart.umd.min.js.map"),
        "./data/dashboard.html" => include_str!("../../data/dashboard.html"),
        "./data/dashboard.js" => include_str!("../../data/dashboard.js"),
        _ => return Ok(None),
    };

    Ok(Some(Cow::Borrowed(found)))
}
