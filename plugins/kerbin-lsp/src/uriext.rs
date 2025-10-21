use std::str::FromStr;

use lsp_types::Uri;

pub trait UriExt {
    fn file_path(path: &str) -> Result<Uri, String>;
}

impl UriExt for Uri {
    fn file_path(path: &str) -> Result<Uri, String> {
        Uri::from_str(&format!("file://{path}")).map_err(|x| x.to_string())
    }
}
