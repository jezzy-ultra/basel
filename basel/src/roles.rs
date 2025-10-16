use std::fmt::{Display, Formatter, Result as FmtResult};
use std::result::Result as StdResult;
use std::str::FromStr;

use owo_colors::OwoColorize as _;
use owo_colors::Stream::Stdout;
use serde::Serialize;

use crate::swatches::SwatchName;

pub(crate) const ROLE_VARIANT_SEPARATOR: char = '_';

// TODO: consider using a macro to generate roles as enums?
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

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("undefined role `{0}`")]
    UndefinedRole(String),
    #[error("circular role references: {}", format_circular_chain(.0))]
    CircularReference(Vec<String>),
    #[error("required role `{0}` missing")]
    MissingRequired(String),
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

pub(crate) type Result<T> = StdResult<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct RoleName(&'static str);

impl RoleName {
    #[expect(clippy::unreachable, reason = "guaranteed by role design")]
    #[must_use]
    pub fn classify(&self) -> RoleKind {
        if is_base(self.as_str()) {
            RoleKind::Base(BaseRole(*self))
        } else {
            let base_str = extract_base(self.as_str());
            let base_role = ROLES
                .iter()
                .find(|&&role| role == base_str)
                .copied()
                .map_or_else(|| unreachable!("optional roles always have a base"), Self);

            RoleKind::Optional(OptionalRole {
                name: *self,
                base: BaseRole(base_role),
            })
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        s.parse()
    }

    #[must_use]
    pub const fn as_str(&self) -> &str {
        self.0
    }
}

impl FromStr for RoleName {
    type Err = Error;

    fn from_str(s: &str) -> StdResult<Self, Self::Err> {
        ROLES
            .iter()
            .find(|&&role| role == s)
            .copied()
            .map(Self)
            .ok_or_else(|| Error::UndefinedRole(s.to_owned()))
    }
}

impl Display for RoleName {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.0)
    }
}

pub(crate) fn iter() -> impl Iterator<Item = RoleName> {
    ROLES.iter().copied().map(RoleName)
}

fn extract_base(role: &str) -> &str {
    if let Some((base, _)) = role.rsplit_once(ROLE_VARIANT_SEPARATOR) {
        return base;
    }

    role
}

fn is_base(role: &str) -> bool {
    extract_base(role) == role
}

pub(crate) fn base() -> impl Iterator<Item = RoleName> {
    ROLES.iter().copied().filter(|&s| is_base(s)).map(RoleName)
}

#[derive(Debug, Clone, Copy)]
pub struct BaseRole(RoleName);

impl BaseRole {
    #[must_use]
    pub const fn name(&self) -> &RoleName {
        &self.0
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OptionalRole {
    name: RoleName,
    base: BaseRole,
}

impl OptionalRole {
    #[must_use]
    pub const fn name(&self) -> &RoleName {
        &self.name
    }

    #[must_use]
    pub const fn base(&self) -> &RoleName {
        self.base.name()
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
pub enum RoleKind {
    Base(BaseRole),
    Optional(OptionalRole),
}

#[non_exhaustive]
#[derive(Debug, Serialize)]
pub enum RoleValue {
    Swatch(SwatchName),
    Role(RoleName),
}

impl RoleValue {
    pub fn parse(val: &str) -> Result<Self> {
        if let Some(swatch_name) = val.strip_prefix('$') {
            let display_name = SwatchName::parse(swatch_name).map_err(|_err| {
                Error::UndefinedRole(format!("invalid swatch reference: `${swatch_name}`"))
            })?;
            Ok(Self::Swatch(display_name))
        } else {
            let role_name = val.parse()?;
            Ok(Self::Role(role_name))
        }
    }
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
            .collect::<Result<Vec<RoleName>>>()
            .unwrap_or_else(|e| panic!("invalid `ROLE` name {e}"));
    }

    fn role_name_errors_on_invalid_name() {}
}
