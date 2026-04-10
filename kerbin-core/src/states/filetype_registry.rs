use std::collections::HashMap;

use crate::*;

#[derive(State, Default)]
pub struct FiletypeRegistry {
    pub ext_map: HashMap<String, String>,
    pub filename_map: HashMap<String, String>,
    pub first_line_patterns: Vec<(String, String)>,
}

impl FiletypeRegistry {
    /// Map a file extension to a filetype. Existing registrations win.
    pub fn register_ext(&mut self, ext: impl Into<String>, filetype: impl Into<String>) {
        self.ext_map.entry(ext.into()).or_insert(filetype.into());
    }

    /// Map an exact filename to a filetype. Existing registrations win.
    pub fn register_filename(&mut self, filename: impl Into<String>, filetype: impl Into<String>) {
        self.filename_map
            .entry(filename.into())
            .or_insert(filetype.into());
    }

    /// Add a first-line regex pattern that maps to a filetype.
    pub fn register_first_line(&mut self, pattern: impl Into<String>, filetype: impl Into<String>) {
        self.first_line_patterns
            .push((pattern.into(), filetype.into()));
    }

    /// Detect a filetype from a file path and optional first line of content.
    /// Priority: exact filename → extension → first-line regex.
    pub fn detect(&self, path: &str, first_line: Option<&str>) -> Option<String> {
        let p = std::path::Path::new(path);
        let filename = p.file_name()?.to_str()?;

        if let Some(ft) = self.filename_map.get(filename) {
            return Some(ft.clone());
        }

        if let Some(ext) = p.extension().and_then(|e| e.to_str())
            && let Some(ft) = self.ext_map.get(ext) {
                return Some(ft.clone());
            }

        if let Some(line) = first_line {
            for (pattern, ft) in &self.first_line_patterns {
                if ::regex::Regex::new(pattern).is_ok_and(|r: ::regex::Regex| r.is_match(line)) {
                    return Some(ft.clone());
                }
            }
        }

        None
    }
}
