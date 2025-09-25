#![allow(clippy::cargo_common_metadata, reason = "todo: documentation")]
#![allow(clippy::missing_docs_in_private_items, reason = "todo: documentation")]
#![allow(clippy::missing_errors_doc, reason = "todo: documentation")]
#![expect(clippy::missing_panics_doc, reason = "todo: better error handling")]
#![allow(clippy::panic_in_result_fn, reason = "todo: better error handling")]
#![allow(clippy::panic, reason = "todo: better error handling")]
#![allow(clippy::unwrap_used, reason = "todo: better error handling")]

use std::{io, result};

use scheme::ResolveError;

pub mod render;
pub mod scheme;
pub mod template;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("TOML parsing error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("scheme resolution error: {0}")]
    Resolve(#[from] ResolveError),
    #[error("template error: {0}")]
    Template(#[from] minijinja::Error),
}

pub type Result<T> = result::Result<T, Error>;

#[derive(Debug)]
pub struct Config {
    pub scheme_dir: String,
    pub template_dir: String,
    pub output_dir: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            scheme_dir: "schemes".to_owned(),
            template_dir: "templates".to_owned(),
            output_dir: "render".to_owned(),
        }
    }
}

fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .is_some_and(|s| s.starts_with('.'))
}
