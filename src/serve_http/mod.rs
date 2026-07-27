mod async_loop;
mod background_poll;
mod file_contents;
mod http_handler;

pub use async_loop::serve_http;
#[cfg(feature = "mock_data")]
pub use async_loop::serve_mock_http;
#[cfg(not(debug_assertions))]
pub use file_contents::{INCLUDED_FILES, INCLUDED_FILES_ROOT};
