use std::fmt::{Display, Formatter, Result as FmtResult};
use std::ops::Deref;

use owo_colors::OwoColorize as _;
use owo_colors::Stream::Stdout;
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("undefined slot `{0}`")]
    UndefinedSlot(String),
    #[error("circular slot references: {}", format_circular_chain(.0))]
    CircularReference(Vec<String>),
    #[error("required slot `{0}` missing")]
    MissingRequired(String),
}

fn format_circular_chain(slots: &[String]) -> String {
    slots
        .iter()
        .enumerate()
        .map(|(i, slot)| {
            if i == slots.len() - 1 {
                format!(
                    "`{}`",
                    slot.if_supports_color(Stdout, |text| text.red().underline().to_string())
                )
            } else {
                format!("`{slot}`")
            }
        })
        .collect::<Vec<_>>()
        .join(" -> ")
}

pub type Result<T> = std::result::Result<T, Error>;

const SLOTS: &[&str] = &[
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct SlotName(pub String);

impl SlotName {
    pub fn parse(name: &str) -> Result<Self> {
        if SLOTS.contains(&name) {
            Ok(Self(name.to_owned()))
        } else {
            Err(Error::UndefinedSlot(name.to_owned()))
        }
    }

    #[must_use]
    pub fn classify(self) -> SlotKind {
        if is_base(&self) {
            SlotKind::Base(BaseSlot(self))
        } else {
            let base = BaseSlot(Self(derive_base(&self).to_owned()));
            SlotKind::Optional(OptionalSlot { name: self, base })
        }
    }

    #[must_use]
    fn as_str(&self) -> &str {
        &self.0
    }
}

impl Deref for SlotName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl Display for SlotName {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.0)
    }
}

pub fn iter() -> impl Iterator<Item = SlotName> {
    SLOTS.iter().map(|&s| SlotName(s.to_owned()))
}

fn derive_base(slot: &str) -> &str {
    if let Some((base, _)) = slot.rsplit_once('_') {
        return base;
    }

    slot
}

fn is_base(slot: &str) -> bool {
    derive_base(slot) == slot
}

pub fn base() -> impl Iterator<Item = SlotName> {
    SLOTS
        .iter()
        .filter(|&&s| is_base(s))
        .map(|&s| SlotName(s.to_owned()))
}

#[derive(Debug)]
pub struct BaseSlot(SlotName);

impl BaseSlot {
    const fn name(&self) -> &SlotName {
        &self.0
    }
}

impl Deref for BaseSlot {
    type Target = SlotName;

    fn deref(&self) -> &Self::Target {
        self.name()
    }
}

#[derive(Debug)]
pub struct OptionalSlot {
    pub name: SlotName,
    pub base: BaseSlot,
}

impl OptionalSlot {
    const fn name(&self) -> &SlotName {
        &self.name
    }
}

impl Deref for OptionalSlot {
    type Target = SlotName;

    fn deref(&self) -> &Self::Target {
        self.name()
    }
}

#[derive(Debug)]
pub enum SlotKind {
    Base(BaseSlot),
    Optional(OptionalSlot),
}

#[derive(Debug, Serialize)]
pub enum SlotValue {
    Swatch(String),
    Slot(SlotName),
}

impl SlotValue {
    pub fn parse(val: &str) -> Result<Self> {
        if let Some(swatch_name) = val.strip_prefix('$') {
            Ok(Self::Swatch(swatch_name.to_owned()))
        } else {
            let slot_name = SlotName::parse(val)?;
            Ok(Self::Slot(slot_name))
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::{assert_eq, assert_ne};

    use super::*;

    fn create_invalid_slot_names() -> Vec<&'static str> {
        vec![
            "selection",
            "42",
            "acc ent",
            "alt_accent",
            "syntax.keyword_function_alt",
        ]
    }

    #[test]
    fn slots_are_valid() {
        let slots = iter()
            .map(|s| SlotName::parse(&s))
            .collect::<Result<Vec<SlotName>>>()
            .unwrap_or_else(|e| panic!("invalid `SLOT` name {e}"));
    }

    fn slot_name_errors_on_invalid_name() {}
}
