use proc_macro::{Delimiter, Group, Literal, Punct, Spacing, TokenStream, TokenTree};
use std::fs::File;
use std::io::Read;
use std::path::{PathBuf, absolute};
use std::time::UNIX_EPOCH;

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
    let metadata = file.metadata()?;
    let len = metadata.len();
    let etag = match metadata.modified()?.duration_since(UNIX_EPOCH) {
        Ok(duration) => format!("W/\"{}-{}\"", duration.as_millis(), len),
        Err(e) => format!("W/\"-{}-{}\"", e.duration().as_millis(), len),
    };
    Ok((contents, etag))
}
