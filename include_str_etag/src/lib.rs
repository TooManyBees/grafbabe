use proc_macro::{Delimiter, Group, Literal, Punct, Spacing, TokenStream, TokenTree};
use std::fs::File;
use std::io::Read;
use std::path::{PathBuf, absolute};

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

    let (contents, etag) = match read_file_and_etag(&filename) {
        Ok(pair) => pair,
        Err(e) => {
            let pathname = absolute(&filename).unwrap_or(PathBuf::from(&filename));
            let path_str = pathname.to_string_lossy();
            panic!("Error reading file {path_str}: {e}");
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

fn read_file_and_etag(filename: &str) -> std::io::Result<(String, String)> {
    let mut file = File::open(&filename)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let hash = xxhash_rust::xxh3::xxh3_128(contents.as_bytes());
    let hash_hex_chars = hash.to_le_bytes().into_iter().flat_map(|byte| {
        let left = (byte >> 4) as u32;
        let right = (byte & 0b00001111) as u32;
        [left, right].into_iter().map(move |n| {
            char::from_digit(n, 16).unwrap_or_else(|| panic!("failed to convert nybble {n:02x} to char"))
        })
    });
    let etag: String = std::iter::once('"')
        .chain(hash_hex_chars)
        .chain(std::iter::once('"'))
        .collect();
    Ok((contents, etag))
}
