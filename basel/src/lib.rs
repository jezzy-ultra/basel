#![feature(adt_const_params)]
#![feature(unsized_const_params)]
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
use std::result::Result as StdResult;

mod extensions;
mod output;
mod render;
mod templates;

use self::config::Error as ConfigError;
use self::output::UpstreamError;
use self::render::ManifestError;
use self::schemes::{Error as SchemeError, NameError, RoleError, SwatchError};

pub mod cli;
pub mod config;
pub(crate) mod schemes;

pub use self::config::Config;
pub use self::extensions::PathExt;
pub(crate) use self::schemes::Scheme;

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
#[expect(
    private_interfaces,
    reason = "this is fine for this kind of error type I think?"
)]
pub enum Error {
    #[error("configuration error: {0}")]
    Config(#[from] ConfigError),
    #[error("scheme error: {0}")]
    Scheme(#[from] SchemeError),
    #[error("name validation error: {0}")]
    Name(#[from] NameError),
    #[error("palette error: {0}")]
    Swatch(#[from] SwatchError),
    #[error("role error: {0}")]
    Role(#[from] RoleError),
    #[error("error processing template: {0}")]
    Template(#[source] anyhow::Error),
    #[error("manifest error: {0}")]
    Manifest(#[from] ManifestError),
    #[error("error rendering: {0}")]
    Rendering(#[source] anyhow::Error),
    #[error("upstream error: {0}")]
    Upstream(#[from] UpstreamError),
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
        Self::Rendering(err.into())
    }

    pub(crate) fn template(err: impl Into<anyhow::Error>) -> Self {
        Self::Template(err.into())
    }
}

pub type Result<T> = StdResult<T, Error>;
