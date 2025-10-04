#![feature(unqualified_local_imports)]
#![feature(supertrait_item_shadowing)]
#![feature(non_exhaustive_omitted_patterns_lint)]
#![feature(must_not_suspend)]
#![feature(multiple_supertrait_upcastable)]
#![feature(strict_provenance_lints)]
#![allow(missing_docs, reason = "TODO: better documentation")]
#![allow(clippy::missing_docs_in_private_items, reason = "TODO: documentation")]
#![allow(clippy::missing_errors_doc, reason = "TODO: documentation")]

use std::io::Error as IoError;
use std::path::PathBuf;
use std::result::Result as StdResult;

use crate::render::Error as RenderError;
use crate::schemes::Error as SchemeError;
use crate::templates::Error as TemplateError;
use crate::upstream::Error as UpstreamError;
pub mod render;
pub mod schemes;
pub mod slots;
pub mod templates;
pub mod upstream;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("upstream error: {0}")]
    Upstream(Box<UpstreamError>),
    #[error("scheme error: {0}")]
    Scheme(Box<SchemeError>),
    #[error("template error: {0}")]
    Template(Box<TemplateError>),
    #[error("render error: {0}")]
    Render(Box<RenderError>),
    #[error("file system error: {0}")]
    Io(#[from] IoError),
    #[error("internal error in {module}: {reason}! this is a bug!")]
    InternalBug {
        module: &'static str,
        reason: String,
    },
}

pub type Result<T> = StdResult<T, Error>;

impl From<Box<UpstreamError>> for Error {
    fn from(err: Box<UpstreamError>) -> Self {
        match *err {
            UpstreamError::InternalBug(reason) => Self::InternalBug {
                module: "upstream",
                reason,
            },
            other => Self::Upstream(Box::new(other)),
        }
    }
}

impl From<UpstreamError> for Error {
    fn from(err: UpstreamError) -> Self {
        Box::new(err).into()
    }
}

impl From<Box<SchemeError>> for Error {
    fn from(err: Box<SchemeError>) -> Self {
        match *err {
            SchemeError::InternalBug(reason) => Self::InternalBug {
                module: "schemes",
                reason,
            },
            other => Self::Scheme(Box::new(other)),
        }
    }
}

impl From<SchemeError> for Error {
    fn from(err: SchemeError) -> Self {
        Box::new(err).into()
    }
}

impl From<Box<TemplateError>> for Error {
    fn from(err: Box<TemplateError>) -> Self {
        match *err {
            TemplateError::InternalBug(reason) => Self::InternalBug {
                module: "templates",
                reason,
            },
            other => Self::Template(Box::new(other)),
        }
    }
}

impl From<TemplateError> for Error {
    fn from(err: TemplateError) -> Self {
        Box::new(err).into()
    }
}

impl From<Box<RenderError>> for Error {
    fn from(err: Box<RenderError>) -> Self {
        match *err {
            RenderError::InternalBug(reason) => Self::InternalBug {
                module: "render",
                reason,
            },
            other => Self::Render(Box::new(other)),
        }
    }
}

impl From<RenderError> for Error {
    fn from(err: RenderError) -> Self {
        Box::new(err).into()
    }
}

#[derive(Debug)]
pub struct Dirs {
    pub schemes: String,
    pub templates: String,
    pub render: String,
}

impl Default for Dirs {
    fn default() -> Self {
        Self {
            schemes: "schemes".to_owned(),
            templates: "templates".to_owned(),
            render: "render".to_owned(),
        }
    }
}

#[derive(Debug, Default)]
pub struct Upstream {
    pub repo_path: Option<PathBuf>,
    pub pattern: Option<String>,
    pub branch: Option<String>,
}

#[derive(Debug)]
pub struct Config {
    pub dirs: Dirs,
    pub upstream: Option<Upstream>,
    pub ignored_directives: Vec<Vec<String>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            dirs: Dirs::default(),
            upstream: None,
            ignored_directives: vec![vec!["#:tombi".to_owned(), "=".to_owned()]],
        }
    }
}
