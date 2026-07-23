use std::borrow::Cow;
use std::io::Error;

pub fn file_contents(path: &str, if_none_match: Option<&str>) -> Result<FileResult, Error> {
    #[cfg(not(feature = "include_html"))]
    let result = read_file(path);
    #[cfg(feature = "include_html")]
    let result = read_included(path);

    if let Some(if_none_match) = if_none_match {
        if let Ok(FileResult::Found { ref etag, .. }) = result {
            if etag == if_none_match {
                return Ok(FileResult::NotModified);
            }
        }
    }

    result
}

pub enum FileResult {
    NotFound,
    NotModified,
    Found {
        contents: Cow<'static, str>,
        etag: Cow<'static, str>,
    },
}

#[cfg(not(feature = "include_html"))]
fn read_file(path: &str) -> Result<FileResult, Error> {
    use std::fs::File;
    use std::io::{ErrorKind, Read};
    use std::time::UNIX_EPOCH;

    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(FileResult::NotFound),
        Err(e) if e.kind() == ErrorKind::IsADirectory => return Ok(FileResult::NotFound),
        Err(e) => return Err(e),
    };
    let metadata = file.metadata()?;
    let last_modified = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .expect("I'm not interested in running before 1970");
    let etag = format!("W/\"{}-{}\"", last_modified.as_millis(), metadata.len());

    let mut string = String::new();
    file.read_to_string(&mut string)?;
    Ok(FileResult::Found {
        contents: Cow::Owned(string),
        etag: Cow::Owned(etag),
    })
}

#[cfg(feature = "include_html")]
fn read_included(path: &str) -> Result<FileResult, Error> {
    use include_str_etag::include_str_etag;

    let (contents, etag) = match path {
        "./data/chart.umd.min.js" => include_str_etag!("./data/chart.umd.min.js"),
        "./data/chart.umd.min.js.map" => include_str_etag!("./data/chart.umd.min.js.map"),
        "./data/dashboard.html" => include_str_etag!("./data/dashboard.html"),
        "./data/dashboard.js" => include_str_etag!("./data/dashboard.js"),
        _ => return Ok(FileResult::NotFound),
    };

    Ok(FileResult::Found {
        contents: Cow::Borrowed(contents),
        etag: Cow::Borrowed(etag),
    })
}
