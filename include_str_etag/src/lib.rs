use proc_macro::{Delimiter, Group, Literal, Punct, Spacing, TokenStream, TokenTree};
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

    let (contents, etag) = match read_file_and_etag(&path) {
        Ok(pair) => pair,
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

static HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

fn read_file_and_etag<P: AsRef<Path>>(filename: &P) -> std::io::Result<(String, String)> {
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
    Ok((contents, etag))
}
