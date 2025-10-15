use std::str::FromStr;

use lsp_types::Uri;

pub trait UriExt {
    fn file_path(path: &str) -> Result<Uri, ()>;
}

impl UriExt for Uri {
    fn file_path(path: &str) -> Result<Uri, ()> {
        Uri::from_str(&format!("file://{path}")).map_err(|_| ())
    }
}
