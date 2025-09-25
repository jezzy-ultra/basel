use std::fs;
use std::ops::Deref;
use std::path::PathBuf;

use hex_color::{Case, Display as HexDisplay, HexColor, ParseHexColorError};
use indexmap::{IndexMap, IndexSet};
use phf::{OrderedMap, phf_ordered_map};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::json;
use toml::{Table, Value};
use walkdir::WalkDir;

pub const SLOTS_WITH_FALLBACKS: OrderedMap<&str, Option<&str>> = phf_ordered_map! {
    "bg" => None,
    "alt_bg" => None,
    "fg" => None,
    "alt_fg" => None,
    "toolbar" => None,
    "toolbar_popup" => Some("toolbar"),
    "alt_toolbar" => None,
    "select" => None,
    "select_2nd" => Some("select"),
    "alt_select" => None,
    "accent" => None,
    "accent_2nd" => Some("accent"),
    "accent_separator" => Some("accent"),
    "accent_popup" => Some("accent"),
    "accent_linenum" => Some("accent"),
    "inactive" => None,
    "focus" => None,
    "guide" => None,
    "guide_inlay" => Some("guide"),
    "guide_linenum" => Some("guide"),
    "guide_ruler" => Some("guide"),
    "guide_whitespace" => Some("guide"),
    "match" => None,
    "error" => None,
    "warning" => None,
    "hint" => None,
    "info" => None,
    "unnecessary" => None,
    "debug.active" => None,
    "debug.breakpoint" => None,
    "debug.frameline" => None,
    "mode.normal" => None,
    "mode.normal_2nd" => Some("mode.normal"),
    "mode.insert" => None,
    "mode.insert_2nd" => Some("mode.insert"),
    "mode.select" => None,
    "mode.select_2nd" => Some("mode.select"),
    "syntax.variable" => None,
    "syntax.variable_builtin" => Some("syntax.variable"),
    "syntax.variable_parameter" => Some("syntax.variable"),
    "syntax.variable_member" => Some("syntax.variable"),
    "syntax.keyword" => None,
    "syntax.keyword_operator" => Some("syntax.keyword"),
    "syntax.keyword_function" => Some("syntax.keyword"),
    "syntax.keyword_conditional" => Some("syntax.keyword"),
    "syntax.keyword_repeat" => Some("syntax.keyword"),
    "syntax.keyword_import" => Some("syntax.keyword"),
    "syntax.keyword_return" => Some("syntax.keyword"),
    "syntax.keyword_exception" => Some("syntax.keyword"),
    "syntax.keyword_directive" => Some("syntax.keyword"),
    "syntax.keyword_storage" => Some("syntax.keyword"),
    "syntax.type" => None,
    "syntax.type_builtin" => Some("syntax.type"),
    "syntax.type_variant" => Some("syntax.type"),
    "syntax.function" => None,
    "syntax.function_builtin" => Some("syntax.function"),
    "syntax.function_method" => Some("syntax.function"),
    "syntax.function_macro" => Some("syntax.function"),
    "syntax.constant" => None,
    "syntax.constant_builtin" => Some("syntax.constant"),
    "syntax.constant_boolean" => Some("syntax.constant"),
    "syntax.constant_number" => Some("syntax.constant"),
    "syntax.constant_character" => Some("syntax.constant"),
    "syntax.label" => None,
    "syntax.constructor" => None,
    "syntax.string" => None,
    "syntax.attribute" => None,
    "syntax.namespace" => None,
    "syntax.tag" => None,
    "syntax.tag_builtin" => Some("syntax.tag"),
    "syntax.comment" => None,
    "syntax.operator" => None,
    "syntax.punctuation" => None,
    "syntax.punctuation_rainbow1" => Some("syntax.punctuation"),
    "syntax.punctuation_rainbow2" => Some("syntax.punctuation"),
    "syntax.punctuation_rainbow3" => Some("syntax.punctuation"),
    "syntax.punctuation_rainbow4" => Some("syntax.punctuation"),
    "syntax.punctuation_rainbow5" => Some("syntax.punctuation"),
    "syntax.punctuation_rainbow6" => Some("syntax.punctuation"),
    "syntax.special" => None,
    "syntax.special_function" => Some("syntax.special"),
    "syntax.special_character" => Some("syntax.special"),
    "syntax.special_string" => Some("syntax.special"),
    "syntax.special_punctuation" => Some("syntax.special"),
    "diff.plus" => None,
    "diff.minus" => None,
    "diff.delta" => None,
    "diff.delta_moved" => Some("diff.delta"),
    "diff.delta_conflict" => Some("diff.delta"),
    "markup.heading" => None,
    "markup.heading_2nd" => Some("markup.heading"),
    "markup.heading_3rd" => Some("markup.heading"),
    "markup.heading_4th" => Some("markup.heading"),
    "markup.heading_5th" => Some("markup.heading"),
    "markup.heading_6th" => Some("markup.heading"),
    "markup.list" => None,
    "markup.list_numbered" => Some("markup.list"),
    "markup.list_checked" => Some("markup.list"),
    "markup.list_unchecked" => Some("markup.list"),
    "markup.link" => None,
    "markup.link_text" => Some("markup.link"),
    "markup.bold" => None,
    "markup.italic" => None,
    "markup.strikethrough" => None,
    "markup.quote" => None,
    "markup.raw" => None,
    "ansi.black" => None,
    "ansi.black_bright" => Some("ansi.black"),
    "ansi.red" => None,
    "ansi.red_bright" => Some("ansi.red"),
    "ansi.green" => None,
    "ansi.green_bright" => Some("ansi.green"),
    "ansi.yellow" => None,
    "ansi.yellow_bright" => Some("ansi.yellow"),
    "ansi.blue" => None,
    "ansi.blue_bright" => Some("ansi.blue"),
    "ansi.magenta" => None,
    "ansi.magenta_bright" => Some("ansi.magenta"),
    "ansi.cyan" => None,
    "ansi.cyan_bright" => Some("ansi.cyan"),
    "ansi.white" => None,
    "ansi.white_bright" => Some("ansi.white"),
};

#[derive(thiserror::Error, Debug)]
pub enum ResolveError {
    #[error("hex parse error: {0}")]
    HexParse(#[from] ParseHexColorError),
    #[error("undefined palette color `{0}`")]
    UndefinedSwatch(String),
    #[error("undefined slot `{0}`")]
    UndefinedSlot(String),
    #[error("circular reference")]
    Circular,
    #[error("required slot `{0}` missing")]
    MissingRequired(String),
}

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub struct Swatch(HexDisplay);

impl Swatch {
    fn new<T>(input: T) -> Result<Self, ResolveError>
    where
        Self: TryFrom<T, Error = ResolveError>,
    {
        Self::try_from(input)
    }

    #[must_use]
    pub const fn color(self) -> HexColor {
        self.0.color()
    }
}

impl From<HexColor> for Swatch {
    fn from(color: HexColor) -> Self {
        Self(HexDisplay::new(color).with_case(Case::Lower))
    }
}

impl From<HexDisplay> for Swatch {
    fn from(display: HexDisplay) -> Self {
        Self(display.with_case(Case::Lower))
    }
}

impl TryFrom<&str> for Swatch {
    type Error = ResolveError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let color = HexColor::parse(s)?;
        Ok(Self(HexDisplay::new(color).with_case(Case::Lower)))
    }
}

impl Deref for Swatch {
    type Target = HexDisplay;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Serialize for Swatch {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Swatch {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::new(s.as_str()).map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Clone, Debug)]
enum SlotValue {
    Other(String),
    Swatch(String),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ResolvedSlot {
    pub swatch_name: String,
    pub hex: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Meta {
    pub author: String,
    pub license: String,
    pub blurb: String,
    pub upstream: String,
    // FIXME: add default value
    pub upstream_template: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Scheme {
    #[serde(rename(serialize = "SCHEME"))]
    pub scheme: String,
    pub meta: Meta,
    pub palette: IndexMap<String, Swatch>,
    #[serde(flatten)]
    pub resolved_slots: IndexMap<String, ResolvedSlot>,
}

impl Scheme {
    #[must_use]
    pub fn create_context(
        &self,
        directives: &IndexMap<String, String>,
    ) -> serde_json::Map<String, serde_json::Value> {
        let mut context = serde_json::Map::new();
        let use_swatch_names = directives
            .get("use_swatch_names")
            .is_some_and(|v| v == "true");

        context.insert(
            "SCHEME".to_owned(),
            serde_json::to_value(&self.scheme).unwrap(),
        );
        context.insert("meta".to_owned(), serde_json::to_value(&self.meta).unwrap());

        let p: Vec<serde_json::Value> = self
            .palette
            .iter()
            .map(|(name, swatch)| {
                json!({
                    "name": name,
                    "hex": swatch.to_string(),
                })
            })
            .collect();
        context.insert("palette".to_owned(), serde_json::Value::Array(p));

        for (slot_path, resolved_slot) in &self.resolved_slots {
            let parts: Vec<&str> = slot_path.split('.').collect();

            let slot = if use_swatch_names {
                serde_json::Value::String(resolved_slot.swatch_name.clone())
            } else {
                serde_json::Value::String(resolved_slot.hex.clone())
            };

            match parts.as_slice() {
                [key] => {
                    context.insert((*key).to_owned(), serde_json::to_value(slot).unwrap());
                }
                [group, key] => {
                    let group_obj = context
                        .entry(group.to_owned())
                        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
                    if let serde_json::Value::Object(group_map) = group_obj {
                        group_map.insert((*key).to_owned(), slot);
                    }
                }
                _ => unreachable!(
                    "all slots validated against `SLOTS_WITH_FALLBACKS` should be covered here"
                ),
            }
        }

        context
    }
}

#[derive(Serialize, Debug)]
struct Raw {
    meta: Meta,
    palette: IndexMap<String, Swatch>,
    slots: IndexMap<String, SlotValue>,
}

impl Raw {
    fn parse_slot_value(s: &str) -> Result<SlotValue, ResolveError> {
        s.strip_prefix('$').map_or_else(
            || {
                if SLOTS_WITH_FALLBACKS.contains_key(s) {
                    Ok(SlotValue::Other(s.to_owned()))
                } else {
                    // FIXME: better errors
                    Err(ResolveError::UndefinedSlot(format!(
                        "invalid slot value `{s}`: must either be the name of a swatch in the \
                         palette (starting with `$`) or a valid slot name"
                    )))
                }
            },
            |swatch_name| Ok(SlotValue::Swatch(swatch_name.to_owned())),
        )
    }

    fn parse_slots(slots_value: &Value) -> Result<IndexMap<String, SlotValue>, ResolveError> {
        let mut result = IndexMap::new();

        let Value::Table(table) = slots_value else {
            // FIXME: better errors
            return Err(ResolveError::UndefinedSlot(
                "`slots` must be a table".to_owned(),
            ));
        };

        for (key, val) in table {
            match val {
                Value::Table(nested_table) => {
                    for (nested_key, nested_val) in nested_table {
                        let dot_key = format!("{key}.{nested_key}");
                        if let Value::String(s) = nested_val {
                            result.insert(dot_key, Self::parse_slot_value(s)?);
                        }
                    }
                }
                Value::String(s) => {
                    result.insert(key.clone(), Self::parse_slot_value(s)?);
                }
                _ => {
                    // FIXME: better error
                    return Err(ResolveError::UndefinedSlot(
                        "slot values must be strings".to_owned(),
                    ));
                }
            }
        }

        Ok(result)
    }

    fn resolve_slot(
        &self,
        slot: &str,
        visited: &mut IndexSet<String>,
    ) -> Result<ResolvedSlot, ResolveError> {
        if !visited.insert(slot.to_owned()) {
            return Err(ResolveError::Circular);
        }

        match self.slots.get(slot) {
            Some(SlotValue::Swatch(name)) => self.palette.get(name).map_or_else(
                || Err(ResolveError::UndefinedSwatch(name.clone())),
                |swatch| {
                    Ok(ResolvedSlot {
                        swatch_name: name.clone(),
                        hex: swatch.to_string(),
                    })
                },
            ),
            Some(SlotValue::Other(other)) => self.resolve_slot(other, visited),
            None => match SLOTS_WITH_FALLBACKS.get(slot) {
                Some(Some(fallback)) => self.resolve_slot(fallback, visited),
                Some(None) => Err(ResolveError::MissingRequired(slot.to_owned())),
                None => Err(ResolveError::UndefinedSlot(slot.to_owned())),
            },
        }
    }

    fn resolve_all_slots(&self) -> Result<IndexMap<String, ResolvedSlot>, ResolveError> {
        let mut resolved_slots = IndexMap::new();
        let mut missing = Vec::new();

        for slot in SLOTS_WITH_FALLBACKS.keys() {
            let mut visited = IndexSet::new();
            match self.resolve_slot(slot, &mut visited) {
                Ok(resolved) => {
                    resolved_slots.insert((*slot).to_owned(), resolved);
                }
                Err(ResolveError::MissingRequired(_)) => missing.push((*slot).to_owned()),
                Err(e) => return Err(e),
            }
        }

        if !missing.is_empty() {
            return Err(ResolveError::MissingRequired(format!(
                "Missing required slots: {}",
                missing.join(", ")
            )));
        }

        Ok(resolved_slots)
    }

    fn into_resolved_scheme(self, name: &str) -> Result<Scheme, ResolveError> {
        let resolved_slots = self.resolve_all_slots()?;

        Ok(Scheme {
            scheme: name.to_owned(),
            meta: self.meta,
            palette: self.palette,
            resolved_slots,
        })
    }
}

#[expect(clippy::indexing_slicing, reason = "FIXME: better error handling")]
pub fn load(name: &str, path: PathBuf) -> Result<Scheme, ResolveError> {
    let content = fs::read_to_string(path).unwrap();

    let root: Table = toml::from_str(&content).unwrap();
    let meta: Meta = root["meta"].clone().try_into().unwrap();
    let palette: IndexMap<String, Swatch> = root["palette"].clone().try_into().unwrap();
    let slots = Raw::parse_slots(&root["slots"])?;

    let raw = Raw {
        meta,
        palette,
        slots,
    };
    raw.into_resolved_scheme(name)
}

pub fn load_all(dir: &str) -> Result<IndexMap<String, Scheme>, ResolveError> {
    let mut schemes = IndexMap::new();

    for entry in WalkDir::new(dir)
        .into_iter()
        .filter_entry(|e| !crate::is_hidden(e))
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
            let name = path.file_stem().unwrap().to_str().unwrap();
            let s = load(name, path.to_path_buf())?;
            schemes.insert(name.to_owned(), s);
        }
    }

    Ok(schemes)
}

// #[cfg(test)]
// mod tests {
//     use indexmap::indexmap;

//     use super::*;

//     fn create_test_raw(palette: IndexMap<String, &str>, slots: IndexMap<Slot,
// SlotValue>) -> Raw {         let palette = Palette {
//             swatches: palette
//                 .into_iter()
//                 .map(|(name, hex)| (name, Swatch::try_from(hex).unwrap()))
//                 .collect(),
//         };

//         Raw {
//             meta: Meta {
//                 author: Some("Testington McTester".to_owned()),
//                 license: Some("TST".to_owned()),
//                 blurb: Some("test test test!!".to_owned()),
//             },
//             palette,
//             slots,
//         }
//     }

//     mod slot_resolution {
//         use super::*;

//         #[test]
//         fn direct_swatch_reference() {
//             let raw = create_test_raw(
//                 indexmap! {
//                     "blackboard".to_owned() => "#181716",
//                 },
//                 indexmap! {
//                     Slot::Bg => SlotValue::Swatch("blackboard".to_owned()),
//                 },
//             );

//             let mut visited = IndexSet::new();
//             let result = raw.resolve_slot(Slot::Bg, &mut visited).unwrap();

//             assert_eq!(result.swatch_name, "blackboard");
//             assert_eq!(result.hex, "#181716");
//         }

//         #[test]
//         fn slot_reference_chain() {
//             let raw = create_test_raw(
//                 indexmap! {
//                     "black".to_owned() => "#000"
//                 },
//                 indexmap! {
//                     Slot::Bg => SlotValue::Swatch("black".to_owned()),
//                     Slot::Color0 => SlotValue::Other(Slot::Bg),
//                     Slot::Color8 => SlotValue::Other(Slot::Color0)
//                 },
//             );

//             let mut visited = IndexSet::new();
//             let result = raw.resolve_slot(Slot::Color8, &mut
// visited).unwrap();

//             assert_eq!(result.swatch_name, "black");
//             assert_eq!(result.hex, "#000000");
//         }
//     }
// }
