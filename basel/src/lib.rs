#![feature(more_qualified_paths)]
#![feature(stmt_expr_attributes)]
#![feature(str_as_str)]
#![feature(unqualified_local_imports)]
#![feature(supertrait_item_shadowing)]
#![feature(non_exhaustive_omitted_patterns_lint)]
#![feature(must_not_suspend)]
#![feature(multiple_supertrait_upcastable)]
#![feature(strict_provenance_lints)]
#![allow(missing_docs, reason = "todo: better documentation")]
#![allow(clippy::missing_docs_in_private_items, reason = "todo: documentation")]
#![allow(clippy::missing_errors_doc, reason = "todo: documentation")]
#![allow(clippy::redundant_pub_crate, reason = "a fuckton of false positives")]

use std::io::Error as IoError;
use std::path::{Path, PathBuf};
use std::result::Result as StdResult;

use serde::Serialize;

use crate::format::Error as FormatError;
use crate::render::Error as RenderError;
use crate::schemes::Error as SchemeError;
use crate::templates::Error as TemplateError;
use crate::upstream::Error as UpstreamError;

mod directives;
mod format;
pub mod render;
pub mod schemes;
mod slots;
mod swatches;
pub mod templates;
mod upstream;

pub use crate::schemes::{Meta, ResolvedSlot};
pub use crate::slots::{BaseSlot, Error as SlotError, OptionalSlot, SlotKind, SlotName, SlotValue};
pub use crate::swatches::{
    Error as SwatchError, Swatch, SwatchAsciiName, SwatchColor, SwatchDisplayName,
};

pub(crate) const SCHEME_MARKER: &str = "SCHEME";
pub(crate) const SWATCH_MARKER: &str = "SWATCH";
pub(crate) const SLOT_VARIANT_SEPARATOR: char = '_';
pub(crate) const TEMPLATE_SUFFIX: &str = ".jinja";
pub(crate) const SKIP_RENDERING_PREFIX: char = '_';

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
    #[error("formatting error: {0}")]
    Formatting(Box<FormatError>),
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

macro_rules! module_error_with_internal_bug_from {
    ($error_type:ty, $variant:ident, $module:literal) => {
        impl From<Box<$error_type>> for Error {
            fn from(err: Box<$error_type>) -> Self {
                match *err {
                    <$error_type>::InternalBug(reason) => Self::InternalBug {
                        module: $module,
                        reason,
                    },
                    other => Self::$variant(Box::new(other)),
                }
            }
        }

        impl From<$error_type> for Error {
            fn from(err: $error_type) -> Self {
                Box::new(err).into()
            }
        }
    };
}

macro_rules! module_error_from {
    ($error_type:ty, $variant:ident) => {
        impl From<Box<$error_type>> for Error {
            fn from(err: Box<$error_type>) -> Self {
                Self::$variant(err)
            }
        }

        impl From<$error_type> for Error {
            fn from(err: $error_type) -> Self {
                Box::new(err).into()
            }
        }
    };
}

module_error_from!(UpstreamError, Upstream);
module_error_with_internal_bug_from!(SchemeError, Scheme, "schemes");
module_error_with_internal_bug_from!(TemplateError, Template, "templates");
module_error_with_internal_bug_from!(RenderError, Render, "render");
module_error_from!(FormatError, Formatting);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ColorFormat {
    #[default]
    Hex,
    Name,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TextFormat {
    #[default]
    Unicode,
    Ascii,
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
    pub strip_directives: Vec<Vec<String>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            dirs: Dirs::default(),
            upstream: None,
            strip_directives: vec![vec![
                "#:tombi".to_owned(),
                "lint.disabled".to_owned(),
                "=".to_owned(),
                "true".to_owned(),
            ]],
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Special {
    pub upstream_file: Option<String>,
    pub upstream_repo: Option<String>,
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
