#![feature(more_qualified_paths)]
#![feature(stmt_expr_attributes)]
#![feature(str_as_str)]
#![feature(supertrait_item_shadowing)]
#![feature(non_exhaustive_omitted_patterns_lint)]
#![feature(must_not_suspend)]
#![feature(multiple_supertrait_upcastable)]
#![feature(strict_provenance_lints)]
#![allow(missing_docs, reason = "todo: better documentation")]
#![allow(clippy::missing_docs_in_private_items, reason = "todo: documentation")]
#![allow(clippy::missing_errors_doc, reason = "todo: documentation")]
#![allow(clippy::redundant_pub_crate, reason = "a fuckton of false positives")]

use std::io;
use std::path::Path;
use std::result::Result as StdResult;

use serde::Serialize;

pub mod cli;
pub mod config;
pub mod directives;
mod format;
pub mod manifest;
mod names;
pub mod render;
mod roles;
pub mod schemes;
mod swatches;
pub mod templates;
pub mod upstream;

pub use crate::config::{Config, Error as ConfigError};
pub use crate::manifest::Error as ManifestError;
pub use crate::names::Error as NameError;
pub use crate::roles::{BaseRole, Error as RoleError, OptionalRole, RoleKind, RoleName, RoleValue};
pub use crate::schemes::{
    Error as SchemeError, Meta, ResolvedRole, Scheme, SchemeAsciiName, SchemeName,
};
pub use crate::swatches::{Error as SwatchError, Swatch, SwatchAsciiName, SwatchColor, SwatchName};
pub use crate::upstream::Error as UpstreamError;

pub const SCHEME_MARKER: &str = "SCHEME";
pub const SCHEME_VARIABLE: &str = "scheme";
pub const SWATCH_MARKER: &str = "SWATCH";
pub const SWATCH_VARIABLE: &str = "swatch";
pub const ROLE_VARIANT_SEPARATOR: char = '_';
pub const SET_TEST_OBJECT: &str = "_set";
pub const JINJA_TEMPLATE_SUFFIX: &str = ".jinja";
pub const SKIP_RENDERING_PREFIX: char = '_';

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("configuration error: {0}")]
    Config(#[from] ConfigError),
    #[error("upstream error: {0}")]
    Upstream(#[from] UpstreamError),
    #[error("name validation error: {0}")]
    Name(#[from] NameError),
    #[error("palette error: {0}")]
    Swatch(#[from] SwatchError),
    #[error("role error: {0}")]
    Role(#[from] RoleError),
    #[error("scheme error: {0}")]
    Scheme(#[from] SchemeError),
    #[error("error processing template: {0}")]
    Template(String),
    #[error("manifest error: {0}")]
    Manifest(#[from] ManifestError),
    #[error("error rendering: {0}")]
    Rendering(String),
    #[error("error formatting: {0}")]
    Formatting(String),
    #[error("file system error: {0}")]
    Io(#[from] io::Error),
    #[error("internal error in {module}: {reason}! this is a bug!")]
    InternalBug {
        module: &'static str,
        reason: String,
    },
}

impl Error {
    pub(crate) fn rendering(err: impl Into<anyhow::Error>) -> Self {
        Self::Rendering(err.into().to_string())
    }

    pub(crate) fn template(err: impl Into<anyhow::Error>) -> Self {
        Self::Template(err.into().to_string())
    }

    pub(crate) fn formatting(err: impl Into<anyhow::Error>) -> Self {
        Self::Formatting(err.into().to_string())
    }
}

pub type Result<T> = StdResult<T, Error>;

#[non_exhaustive]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ColorFormat {
    #[default]
    Hex,
    Name,
}

#[non_exhaustive]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TextFormat {
    #[default]
    Unicode,
    Ascii,
}

#[non_exhaustive]
#[derive(Debug, Default, Clone)]
pub struct Special {
    pub upstream_file: Option<String>,
    pub upstream_repo: Option<String>,
}

pub(crate) fn extract_filename_from(path: &str) -> &str {
    if let Some((_, file)) = path.rsplit_once('/') {
        return file;
    }

    path
}

pub(crate) fn extract_parents_from(path: &str) -> Option<&str> {
    if let Some((ancestors, _)) = path.rsplit_once('/') {
        return Some(ancestors);
    }

    None
}

pub(crate) fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case(extension))
}

pub(crate) fn is_toml(path: &Path) -> bool {
    has_extension(path, "toml")
}

pub(crate) fn is_markdown(path: &Path) -> bool {
    has_extension(path, "md")
}

pub(crate) fn is_json(path: &Path) -> bool {
    has_extension(path, "json")
}

pub(crate) fn is_jsonc(path: &Path) -> bool {
    has_extension(path, "jsonc")
}

pub(crate) fn is_json5(path: &Path) -> bool {
    has_extension(path, "json5")
}

pub(crate) fn is_xml(path: &Path) -> bool {
    has_extension(path, "xml")
}

pub(crate) fn is_svg(path: &Path) -> bool {
    has_extension(path, "svg")
}
