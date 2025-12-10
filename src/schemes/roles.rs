//! Semantic color vocabulary to define how a scheme's palette is used.
//!
//! _Roles_ are a collection of common semantic components of themeable
//! interfaces that sit as an intermediary abstraction layer between the *what*
//! — the colors used by the scheme — and a template's rendered output by
//! describing *how* the palette gets rendered into concrete key/value pairs.
//! For instance, the cutiepro scheme maps **blackboard** (`#181716`) to the
//! `bg` role:
//!
//! ```toml
//! [palette]
//! blackboard = "#181716"
//!
//! [roles]
//! bg = "$blackboard"
//! ```
//!
//! In the Helix port template, `ui.background` gets assigned to the resolved
//! color value of `bg`:
//!
//! ```toml
//! # before rendering:
//! "ui.background" = "{{ bg }}"
//!
//! # rendered with cutiepro:
//! "ui.background" = "#181716"
//! ```
//!
//! ...while in kitty's configuration the same thing is expressed as:
//!
//! ```shell
//! # before rendering:
//! background  {{ bg }}
//!
//! # rendered with cutiepro:
//! background  #181716
//! ```

use std::fmt::{Display, Formatter, Result as FmtResult};
use std::result::Result as StdResult;
use std::str::FromStr;

use owo_colors::OwoColorize as _;
use owo_colors::Stream::Stdout;
use serde::Serialize;

use super::SwatchName;

// TODO: consider using a macro to generate roles as enum variants?
const ROLES: &[&str] = &[
    "bg",
    "bg_alt",
    "fg",
    "fg_alt",
    "toolbar",
    "toolbar_popup",
    "toolbar_alt",
    "select",
    "select_2nd",
    "select_alt",
    "accent",
    "accent_2nd",
    "accent_separator",
    "accent_popup",
    "accent_linenum",
    "inactive",
    "focus",
    "guide",
    "guide_inlay",
    "guide_linenum",
    "guide_ruler",
    "guide_whitespace",
    "match",
    "error",
    "warning",
    "info",
    "hint",
    "debug.active",
    "debug.breakpoint",
    "debug.frameline",
    "mode.normal",
    "mode.normal_2nd",
    "mode.insert",
    "mode.insert_2nd",
    "mode.select",
    "mode.select_2nd",
    "syntax.variable",
    "syntax.variable_builtin",
    "syntax.variable_parameter",
    "syntax.variable_member",
    "syntax.keyword",
    "syntax.keyword_operator",
    "syntax.keyword_function",
    "syntax.keyword_conditional",
    "syntax.keyword_repeat",
    "syntax.keyword_import",
    "syntax.keyword_return",
    "syntax.keyword_exception",
    "syntax.keyword_directive",
    "syntax.keyword_storage",
    "syntax.type",
    "syntax.type_builtin",
    "syntax.type_variant",
    "syntax.function",
    "syntax.function_builtin",
    "syntax.function_method",
    "syntax.function_macro",
    "syntax.constant",
    "syntax.constant_builtin",
    "syntax.constant_boolean",
    "syntax.constant_number",
    "syntax.constant_character",
    "syntax.label",
    "syntax.constructor",
    "syntax.string",
    "syntax.attribute",
    "syntax.namespace",
    "syntax.tag",
    "syntax.tag_builtin",
    "syntax.comment",
    "syntax.comment_doc",
    "syntax.operator",
    "syntax.punctuation",
    "syntax.punctuation_rainbow1",
    "syntax.punctuation_rainbow2",
    "syntax.punctuation_rainbow3",
    "syntax.punctuation_rainbow4",
    "syntax.punctuation_rainbow5",
    "syntax.punctuation_rainbow6",
    "syntax.special",
    "syntax.special_function",
    "syntax.special_character",
    "syntax.special_string",
    "syntax.special_punctuation",
    "diff.plus",
    "diff.minus",
    "diff.delta",
    "diff.delta_moved",
    "diff.delta_conflict",
    "markup.heading",
    "markup.heading_2nd",
    "markup.heading_3rd",
    "markup.heading_4th",
    "markup.heading_5th",
    "markup.heading_6th",
    "markup.list",
    "markup.list_numbered",
    "markup.list_checked",
    "markup.list_unchecked",
    "markup.link",
    "markup.link_text",
    "markup.bold",
    "markup.italic",
    "markup.strikethrough",
    "markup.quote",
    "markup.raw",
    "ansi.black",
    "ansi.black_bright",
    "ansi.red",
    "ansi.red_bright",
    "ansi.green",
    "ansi.green_bright",
    "ansi.yellow",
    "ansi.yellow_bright",
    "ansi.blue",
    "ansi.blue_bright",
    "ansi.magenta",
    "ansi.magenta_bright",
    "ansi.cyan",
    "ansi.cyan_bright",
    "ansi.white",
    "ansi.white_bright",
];

const VARIANT_SEPARATOR: char = '_';

type Result<T> = StdResult<T, Error>;

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("undefined role `{0}`")]
    Undefined(String),

    #[error("circular role reference: {}", format_circular_chain(.0))]
    CircularReference(Vec<String>),

    #[error("required role `{0}` missing")]
    MissingRequired(String),
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
pub(crate) enum Kind {
    Base(Name),
    Optional { base: Name },
}

impl Kind {
    pub(crate) const fn base(&self) -> &Name {
        match self {
            Self::Base(name) => name,
            Self::Optional { base } => base,
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Serialize)]
pub(crate) enum Value {
    Swatch(SwatchName),
    Role(Name),
}

impl Value {
    pub(crate) fn parse(val: &str) -> Result<Self> {
        if let Some(swatch_name) = val.strip_prefix('$') {
            let display_name = SwatchName::parse(swatch_name).map_err(|_err| {
                Error::Undefined(format!("invalid swatch reference: `${swatch_name}`"))
            })?;
            Ok(Self::Swatch(display_name))
        } else {
            let role_name = val.parse()?;
            Ok(Self::Role(role_name))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub(crate) struct Name(&'static str);

impl Name {
    #[expect(clippy::unreachable, reason = "guaranteed by role design")]
    #[must_use]
    pub(crate) fn classify(&self) -> Kind {
        if is_base(self.as_str()) {
            Kind::Base(*self)
        } else {
            let base_str = extract_base(self.as_str());
            let base_role = ROLES
                .iter()
                .find(|&&role| role == base_str)
                .copied()
                .map_or_else(|| unreachable!("optional roles always have a base"), Self);

            Kind::Optional { base: base_role }
        }
    }

    #[must_use]
    pub(crate) const fn as_str(&self) -> &str {
        self.0
    }
}

impl Display for Name {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.0)
    }
}

impl FromStr for Name {
    type Err = Error;

    fn from_str(s: &str) -> StdResult<Self, Self::Err> {
        ROLES
            .iter()
            .find(|&&role| role == s)
            .copied()
            .map(Self)
            .ok_or_else(|| Error::Undefined(s.to_owned()))
    }
}

pub(crate) fn iter() -> impl Iterator<Item = Name> {
    ROLES.iter().copied().map(Name)
}

pub(crate) fn base() -> impl Iterator<Item = Name> {
    ROLES.iter().copied().filter(|&s| is_base(s)).map(Name)
}

fn format_circular_chain(roles: &[String]) -> String {
    roles
        .iter()
        .enumerate()
        .map(|(i, role)| {
            if i == roles.len() - 1 {
                format!(
                    "`{}`",
                    role.if_supports_color(Stdout, |text| text.red().underline().to_string())
                )
            } else {
                format!("`{role}`")
            }
        })
        .collect::<Vec<_>>()
        .join(" -> ")
}

fn extract_base(role: &str) -> &str {
    if let Some((base, _)) = role.rsplit_once(VARIANT_SEPARATOR) {
        return base;
    }

    role
}

fn is_base(role: &str) -> bool {
    extract_base(role) == role
}

#[cfg(test)]
mod tests {
    use pretty_assertions::{assert_eq, assert_ne};

    use super::*;

    fn create_invalid_role_names() -> Vec<&'static str> {
        vec![
            "selection",
            "42",
            "acc ent",
            "alt_accent",
            "syntax.keyword_function_alt",
        ]
    }

    #[test]
    fn roles_are_valid() {
        let roles = iter()
            .map(|s| s.as_str().parse())
            .collect::<Result<Vec<Name>>>()
            .unwrap_or_else(|e| panic!("invalid `ROLE` name {e}"));
    }

    fn role_name_errors_on_invalid_name() {}
}
