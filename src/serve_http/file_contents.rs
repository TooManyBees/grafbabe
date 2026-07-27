#[cfg(not(debug_assertions))]
use include_str_etag::{include_dir_etag, include_dir_root};
use std::borrow::Cow;
use std::fs::File;
use std::io::{Error, ErrorKind, Read};
use std::path::{Path, absolute};
use std::time::UNIX_EPOCH;

pub fn file_contents(
    root: Option<&str>,
    path: &str,
    if_none_match: Option<&str>,
) -> Result<FileResult, Error> {
    #[cfg(debug_assertions)]
    let result = read_file(root, path);
    #[cfg(not(debug_assertions))]
    let result = if root.is_some() {
        read_file(root, path)
    } else {
        read_included(path)
    };

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

fn read_file(root: Option<&str>, filename: &str) -> Result<FileResult, Error> {
    let root = Path::new(root.unwrap_or("frontend"));
    let path = root.join(filename);

    if !absolute(&path)?.starts_with(absolute(root)?) {
        log::debug!(path = path.to_string_lossy(); "Requested path outside of frontend directory");
        return Ok(FileResult::NotFound);
    }

    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(FileResult::NotFound),
        Err(e) if e.kind() == ErrorKind::IsADirectory => return Ok(FileResult::NotFound),
        Err(e) => return Err(e),
    };
    let metadata = file.metadata()?;
    let etag = match metadata.modified()?.duration_since(UNIX_EPOCH) {
        Ok(duration) => format!("W/\"{}-{}\"", duration.as_millis(), metadata.len()),
        Err(e) => format!("W/\"-{}-{}\"", e.duration().as_millis(), metadata.len()),
    };

    let mut string = String::new();
    file.read_to_string(&mut string)?;
    Ok(FileResult::Found {
        contents: Cow::Owned(string),
        etag: Cow::Owned(etag),
    })
}

#[cfg(not(debug_assertions))]
pub static INCLUDED_FILES: &'static [(&'static str, (&'static str, &'static str))] =
    include_dir_etag!("frontend");
#[cfg(not(debug_assertions))]
pub static INCLUDED_FILES_ROOT: &'static str = include_dir_root!("frontend");

#[cfg(not(debug_assertions))]
fn read_included(path: &str) -> Result<FileResult, Error> {
    let maybe_found = INCLUDED_FILES.iter().find(|(key, _)| *key == path);
    let (contents, etag) = match maybe_found {
        Some((_, found)) => found,
        None => return Ok(FileResult::NotFound),
    };

    Ok(FileResult::Found {
        contents: Cow::Borrowed(contents),
        etag: Cow::Borrowed(etag),
    })
}
