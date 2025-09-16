use std::fs;
use std::ops::Deref;
use std::path::PathBuf;
use std::str::FromStr as _;
use std::sync::Arc;

use hex_color::{Case, Display as HexDisplay, HexColor, ParseHexColorError};
use indexmap::{IndexMap, IndexSet};
use minijinja::Value;
use minijinja::value::Object;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use strum::{self, Display, EnumIter, EnumProperty, EnumString, IntoEnumIterator as _};
use walkdir::WalkDir;

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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Palette {
    #[serde(flatten)]
    pub swatches: IndexMap<String, Swatch>,
}

impl Palette {
    #[must_use]
    fn new() -> Self {
        Self {
            swatches: IndexMap::new(),
        }
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Swatch> {
        self.swatches.get(name)
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize, Clone, Debug)]
enum SlotValue {
    Other(Slot),
    Swatch(String),
}

impl<'de> Deserialize<'de> for SlotValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let val = String::deserialize(deserializer)?;
        val.strip_prefix('$').map_or_else(
            || {
                Slot::from_str(&val).map_or_else(
                    |_| {
                        Err(serde::de::Error::custom(format!(
                            "invalid slot value `{val}`: must either be a palette reference \
                             (starting with `$`) or a valid slot name"
                        )))
                    },
                    |slot| Ok(Self::Other(slot)),
                )
            },
            |name| Ok(Self::Swatch(name.to_owned())),
        )
    }
}

#[derive(
    Serialize,
    Deserialize,
    Display,
    EnumIter,
    EnumProperty,
    EnumString,
    Clone,
    Copy,
    Hash,
    Eq,
    PartialEq,
    Debug,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum Slot {
    Bg,
    Fg,
    #[strum(props(fallback = "bg"))]
    Color0,
    Color1,
    Color2,
    Color3,
    Color4,
    Color5,
    Color6,
    #[strum(props(fallback = "fg"))]
    Color7,
    #[strum(props(fallback = "color0"))]
    Color8,
    #[strum(props(fallback = "color1"))]
    Color9,
    #[strum(props(fallback = "color2"))]
    Color10,
    #[strum(props(fallback = "color3"))]
    Color11,
    #[strum(props(fallback = "color4"))]
    Color12,
    #[strum(props(fallback = "color5"))]
    Color13,
    #[strum(props(fallback = "color6"))]
    Color14,
    #[strum(props(fallback = "color7"))]
    Color15,
    Select0,
    Select1,
    #[strum(props(fallback = "color15"))]
    AltSelect,
    #[strum(props(fallback = "color0"))]
    AltText,
    #[strum(props(fallback = "color7"))]
    Comment,
    #[strum(props(fallback = "color14"))]
    Link,
    Accent0,
    Accent1,
    #[strum(props(fallback = "alt_text"))]
    AccentText,
    #[strum(props(fallback = "comment"))]
    Inactive,
    #[strum(props(fallback = "alt_text"))]
    InactiveText,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ResolvedSlot {
    pub swatch: String,
    pub hex: String,
}

impl Object for ResolvedSlot {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        match key.as_str()? {
            "swatch" => Some(Value::from(self.swatch.clone())),
            "hex" => Some(Value::from(self.hex.clone())),
            _ => None,
        }
    }
}

impl Serialize for ResolvedSlot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.hex)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Meta {
    pub author: Option<String>,
    pub license: Option<String>,
    pub blurb: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Scheme {
    pub scheme: String,
    #[serde(flatten)]
    pub meta: Meta,
    pub palette: Palette,
    #[serde(flatten)]
    pub resolved: IndexMap<Slot, ResolvedSlot>,
}

impl Scheme {}

#[derive(Serialize, Deserialize, Debug)]
struct Raw {
    #[serde(flatten)]
    meta: Meta,
    palette: Palette,
    slots: IndexMap<Slot, SlotValue>,
}

impl Raw {
    fn resolve_slot(
        &self,
        slot: Slot,
        visited: &mut IndexSet<Slot>,
    ) -> Result<ResolvedSlot, ResolveError> {
        if !visited.insert(slot) {
            return Err(ResolveError::Circular);
        }

        match self.slots.get(&slot) {
            Some(SlotValue::Swatch(name)) => self.palette.get(name).map_or_else(
                || Err(ResolveError::UndefinedSwatch(name.clone())),
                |swatch| {
                    Ok(ResolvedSlot {
                        swatch: name.clone(),
                        hex: swatch.to_string(),
                    })
                },
            ),
            Some(SlotValue::Other(other)) => self.resolve_slot(*other, visited),
            None => slot.get_str("fallback").map_or_else(
                || Err(ResolveError::MissingRequired(slot.to_string())),
                |fallback_name| {
                    let fallback_slot = Slot::from_str(fallback_name).unwrap_or_else(|_| {
                        panic!(
                            "invalid fallback slot name `{fallback_name}` for slot `{slot}` (this \
                             is a bug!)"
                        )
                    });
                    self.resolve_slot(fallback_slot, visited)
                },
            ),
        }
    }

    fn resolve_all_slots(&self) -> Result<IndexMap<Slot, ResolvedSlot>, ResolveError> {
        let mut resolved_slots = IndexMap::new();
        let mut missing = Vec::new();

        for slot in Slot::iter() {
            let mut visited = IndexSet::new();
            match self.resolve_slot(slot, &mut visited) {
                Ok(resolved) => {
                    resolved_slots.insert(slot, resolved);
                }
                Err(ResolveError::MissingRequired(_)) => missing.push(slot.to_string()),
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

    fn try_into_scheme(self, name: &str) -> Result<Scheme, ResolveError> {
        let resolved = self.resolve_all_slots()?;

        Ok(Scheme {
            scheme: name.to_owned(),
            meta: self.meta,
            palette: self.palette,
            resolved,
        })
    }
}

pub fn load(name: &str, path: PathBuf) -> Result<Scheme, ResolveError> {
    let content = fs::read_to_string(path).unwrap();
    let raw: Raw = toml::from_str(&content).unwrap();

    raw.try_into_scheme(name)
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
