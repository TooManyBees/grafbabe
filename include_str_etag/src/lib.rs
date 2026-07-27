use proc_macro::{Delimiter, Group, Literal, Punct, Spacing, TokenStream, TokenTree};
use std::ffi::OsString;
use std::fs::File;
use std::io::{ErrorKind, Read};
use std::iter::once;
use std::path::{Path, PathBuf, absolute};

const GRAFBABE_FRONTEND: &'static str = "GRAFBABE_FRONTEND";

#[proc_macro]
pub fn include_str_etag(input: TokenStream) -> TokenStream {
    let mut tokens = input.into_iter();
    let filename = match (tokens.next(), tokens.next()) {
        (Some(TokenTree::Literal(l)), None) => {
            let s = l.to_string();
            s.trim_matches('"').to_string()
        }
        (Some(_), _) => {
            panic!("invalid include_str_etag argument: must be one literal");
        }
        (None, _) => {
            panic!("this macro takes one parameter, but 0 were given");
        }
    };

    let frontend_dir = std::env::var(GRAFBABE_FRONTEND)
        .map(PathBuf::from)
        .unwrap_or("frontend".into());
    let path = frontend_dir.join(&filename);

    let IncludedFile { contents, etag } = match read_file_and_etag(&path) {
        Ok(file) => file,
        Err(e) => {
            if e.kind() == ErrorKind::NotFound {
                let dir = absolute(&frontend_dir).unwrap_or(frontend_dir);
                let dir_str = dir.to_string_lossy();
                if std::env::var(GRAFBABE_FRONTEND).is_ok() {
                    panic!("File {filename} not found in {dir_str} (set by {GRAFBABE_FRONTEND})");
                } else {
                    panic!(
                        "File {filename} not found in {dir_str}. You may be compiling from outside the project directory, or you removed the `frontend` directory."
                    );
                }
            } else {
                let pathname = absolute(&path).unwrap_or(path);
                let path_str = pathname.to_string_lossy();
                panic!("Error reading file {path_str}: {e}");
            }
        }
    };

    TokenTree::Group(Group::new(
        Delimiter::Parenthesis,
        TokenStream::from_iter(
            [
                TokenTree::Literal(Literal::string(&contents)),
                TokenTree::Punct(Punct::new(',', Spacing::Alone)),
                TokenTree::Literal(Literal::string(&etag)),
            ]
            .into_iter(),
        ),
    ))
    .into()
}

#[proc_macro]
pub fn include_dir_etag(input: TokenStream) -> TokenStream {
    let mut tokens = input.into_iter();
    let mut frontend_dir: String = match (tokens.next(), tokens.next()) {
        (Some(TokenTree::Literal(l)), None) => {
            let s = l.to_string();
            s.trim_matches('"').to_string()
        }
        (Some(_), _) => {
            panic!("invalid include_dir_etag argument: must be one literal");
        }
        (None, _) => {
            panic!("this macro takes one parameter, but 0 were given");
        }
    };

    frontend_dir = std::env::var(GRAFBABE_FRONTEND).unwrap_or(frontend_dir.to_string());

    let paths_to_include = match list_paths_to_include(&frontend_dir) {
        Ok(paths) => paths,
        Err(e) => if e.kind() == ErrorKind::NotFound {
            let dir = absolute(&frontend_dir).unwrap_or(frontend_dir.into());
            let dir_str = dir.to_string_lossy();
            if std::env::var(GRAFBABE_FRONTEND).is_ok() {
                panic!("Frontend directory {dir_str} not found (set by {GRAFBABE_FRONTEND})");
            } else {
                panic!("Frontend directory {dir_str} not found. You may be compiling outside the project directory, or you removed the `frontend` directory.")
            }
        } else {
            let dir = absolute(&frontend_dir).unwrap_or(frontend_dir.into());
            let dir_str = dir.to_string_lossy();
            if std::env::var(GRAFBABE_FRONTEND).is_ok() {
                panic!("Couldn't include frontend directory {dir_str:?} (set by {GRAFBABE_FRONTEND}): {e}");
            } else {
                panic!("Couldn't include frontend directory {dir_str:?}: {e}");
            }
        }
    };

    TokenStream::from_iter(
        [
            TokenTree::Punct(Punct::new('&', Spacing::Joint)),
            TokenTree::Group(Group::new(
                Delimiter::Bracket,
                TokenStream::from_iter(paths_to_include.iter().map(|path| {
                    let IncludedFile { contents, etag } = match read_file_and_etag(path) {
                        Ok(f) => f,
                        Err(e) => {
                            let path_str = path.to_string_lossy();
                            panic!("Error reading file {path_str}: {e}");
                        }
                    };
                    let path_literal = os_path_to_url_path(path);
                    let path_literal = path_literal
                        .strip_prefix(&frontend_dir)
                        .unwrap_or(&path_literal);
                    let path_literal = path_literal.strip_prefix('/').unwrap_or(&path_literal);
                    TokenStream::from_iter(
                        [
                            TokenTree::Group(Group::new(
                                Delimiter::Parenthesis,
                                TokenStream::from_iter(
                                    [
                                        TokenTree::Literal(Literal::string(&path_literal)),
                                        TokenTree::Punct(Punct::new(',', Spacing::Alone)),
                                        TokenTree::Group(Group::new(
                                            Delimiter::Parenthesis,
                                            TokenStream::from_iter(
                                                [
                                                    TokenTree::Literal(Literal::string(&contents)),
                                                    TokenTree::Punct(Punct::new(
                                                        ',',
                                                        Spacing::Alone,
                                                    )),
                                                    TokenTree::Literal(Literal::string(&etag)),
                                                ]
                                                .into_iter(),
                                            ),
                                        )),
                                    ]
                                    .into_iter(),
                                ),
                            )),
                            TokenTree::Punct(Punct::new(',', Spacing::Alone)),
                        ]
                        .into_iter(),
                    )
                })),
            )),
        ]
        .into_iter(),
    )
    .into()
}

#[proc_macro]
pub fn include_dir_root(input: TokenStream) -> TokenStream {
    let mut tokens = input.into_iter();
    let mut frontend_dir: String = match (tokens.next(), tokens.next()) {
        (Some(TokenTree::Literal(l)), None) => {
            let s = l.to_string();
            s.trim_matches('"').to_string()
        }
        (Some(_), _) => {
            panic!("invalid include_dir_root argument: must be one literal");
        }
        (None, _) => {
            panic!("this macro takes one parameter, but 0 were given");
        }
    };

    frontend_dir = std::env::var(GRAFBABE_FRONTEND).unwrap_or(frontend_dir.to_string());

    TokenTree::Literal(Literal::string(&frontend_dir)).into()
}

fn list_paths_to_include<P: AsRef<Path>>(root: &P) -> std::io::Result<Vec<PathBuf>> {
    let mut result = vec![];
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            if let Some(file_name) = entry.path().file_name().map(|s| s.to_string_lossy()) {
                if file_name.starts_with('.') {
                    continue;
                }
                if file_name.ends_with(".js.map") {
                    continue
                }
            }
            result.push(entry.path());
        }
    }

    result.sort();

    Ok(result)
}

fn os_path_to_url_path(os_path: &Path) -> String {
    let mut path = OsString::with_capacity(os_path.as_os_str().len());
    for (i, c) in os_path.components().enumerate() {
        if i > 0 {
            path.push("/");
        }
        path.push(c);
    }
    path.to_string_lossy().to_string()
}

struct IncludedFile {
    contents: String,
    etag: String,
}

static HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

fn read_file_and_etag<P: AsRef<Path>>(filename: &P) -> std::io::Result<IncludedFile> {
    let mut file = File::open(&filename)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let hash = xxhash_rust::xxh3::xxh3_128(contents.as_bytes());
    let hash_hex_chars = hash.to_le_bytes().into_iter().flat_map(|byte| {
        let left = (byte >> 4) as usize;
        let right = (byte & 0b00001111) as usize;

        once(HEX_CHARS[left] as char).chain(once(HEX_CHARS[right] as char))
    });
    let etag: String = once('"').chain(hash_hex_chars).chain(once('"')).collect();
    Ok(IncludedFile { contents, etag })
}
