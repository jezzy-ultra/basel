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
#![expect(
    incomplete_features,
    reason = "`unsized_const_params` is useful but not finalized yet"
)]
#![expect(
    clippy::redundant_pub_crate,
    reason = "seems to be broken for `pub(crate)` errors"
)]

use std::result::Result as StdResult;

mod extensions;
mod io;
mod manifest;
mod output;
mod render;
pub(crate) mod schemes;
mod templates;

use self::config::Error as ConfigError;
use self::manifest::Error as ManifestError;
pub(crate) use self::manifest::{Entry as ManifestEntry, Manifest};
use self::output::UpstreamError;
pub(crate) use self::schemes::Scheme;
use self::schemes::{Error as SchemeError, NameError, RoleError, SwatchError};
use self::templates::{DirectiveError, ProviderError};

pub mod cli;
pub mod config;

pub use self::config::Config;
pub use self::extensions::PathExt;

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
#[expect(
    private_interfaces,
    reason = "this is fine for this kind of error type I think?"
)]
pub enum Error {
    #[error("configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("manifest error: {0}")]
    Manifest(#[from] ManifestError),

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

    #[error("directive error: {0}")]
    Directive(#[from] DirectiveError),

    #[error("git provider error: {0}")]
    Provider(#[from] ProviderError),

    #[error("error rendering: {0}")]
    Rendering(#[source] anyhow::Error),

    #[error("upstream error: {0}")]
    Upstream(#[from] UpstreamError),

    #[error("file system error: {0}")]
    Io(#[from] std::io::Error),

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
