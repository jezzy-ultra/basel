use std::ops::Deref;
use std::path::PathBuf;
use std::str::FromStr as _;
use std::{fs, result};

use hex_color::{Case, Display as HexDisplay, HexColor, ParseHexColorError};
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use strum::{self, Display, EnumIter, EnumProperty, EnumString, IntoEnumIterator as _};

use crate::Result;

#[derive(thiserror::Error, Debug)]
pub enum ResolveError {
    #[error("hex parse error: {0}")]
    HexParse(#[from] ParseHexColorError),
    #[error("undefined palette color `{0}`")]
    UndefinedPaletteColor(String),
    #[error("undefined slot `{0}`")]
    UndefinedSlot(String),
    #[error("circular reference")]
    Circular,
    #[error("required slot `{0}` missing")]
    MissingRequired(String),
}

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub struct Color(HexDisplay);

impl Color {
    pub fn new<T>(input: T) -> result::Result<Self, ResolveError>
    where
        Self: TryFrom<T, Error = ResolveError>,
    {
        Self::try_from(input)
    }

    #[must_use]
    pub const fn as_hex(self) -> HexColor {
        self.0.color()
    }
}

impl From<HexColor> for Color {
    fn from(hex: HexColor) -> Self {
        Self(HexDisplay::new(hex).with_case(Case::Lower))
    }
}

impl From<HexDisplay> for Color {
    fn from(display: HexDisplay) -> Self {
        Self(display.with_case(Case::Lower))
    }
}

impl TryFrom<&str> for Color {
    type Error = ResolveError;

    fn try_from(s: &str) -> result::Result<Self, Self::Error> {
        let hex = HexColor::parse(s)?;
        Ok(Self(HexDisplay::new(hex).with_case(Case::Lower)))
    }
}

impl Deref for Color {
    type Target = HexDisplay;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Serialize for Color {
    fn serialize<S>(&self, serializer: S) -> result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D>(deserializer: D) -> result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::new(s.as_str()).map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Clone, Debug)]
pub enum SlotValue {
    Other(Slot),
    PaletteColor(String),
    Rgb(Color),
    None,
}

impl<'de> Deserialize<'de> for SlotValue {
    fn deserialize<D>(deserializer: D) -> result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let val = String::deserialize(deserializer)?;
        Ok(if val == "none" {
            Self::None
        } else if let Ok(color) = Color::new(val.as_str()) {
            Self::Rgb(color)
        } else if let Some(name) = val.strip_prefix('$') {
            Self::PaletteColor(name.to_owned())
        } else if let Ok(slot) = Slot::from_str(&val) {
            Self::Other(slot)
        } else {
            return Err(serde::de::Error::custom(format!("bad slot value: {val}")));
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Palette {
    pub colors: IndexMap<String, Color>,
}

impl Palette {
    #[must_use]
    pub fn new() -> Self {
        Self {
            colors: IndexMap::new(),
        }
    }

    pub fn insert(&mut self, name: String, color: Color) -> Option<Color> {
        self.colors.insert(name, color)
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Color> {
        self.colors.get(name)
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::new()
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

#[derive(Serialize, Deserialize, Debug)]
pub struct SlotMap {}

#[derive(Serialize, Deserialize, Debug)]
pub struct Meta {
    pub author: Option<String>,
    pub license: Option<String>,
    pub blurb: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Raw {
    #[serde(flatten)]
    pub meta: Meta,
    pub palette: Palette,
    pub slots: IndexMap<Slot, SlotValue>,
}

impl Raw {
    fn resolve_slot(
        &self,
        slot: Slot,
        cache: &mut IndexMap<Slot, Color>,
        visiting: &mut IndexSet<Slot>,
    ) -> Result<Color> {
        if let Some(c) = cache.get(&slot) {
            return Ok(*c);
        }
        if !visiting.insert(slot) {
            return Err(ResolveError::Circular.into());
        }

        let val = self.slots.get(&slot).cloned().unwrap_or_else(|| {
            slot.get_str("fallback").map_or_else(
                || panic!("missing required slot `{slot}`"),
                |s| SlotValue::Other(Slot::from_str(s).unwrap()),
            )
        });
        let out = self.color_from(val, slot, cache, visiting)?;

        visiting.remove(&slot);
        cache.insert(slot, out);
        Ok(out)
    }

    fn color_from(
        &self,
        val: SlotValue,
        slot: Slot,
        cache: &mut IndexMap<Slot, Color>,
        visiting: &mut IndexSet<Slot>,
    ) -> Result<Color> {
        Ok(match val {
            SlotValue::None => {
                return Err(ResolveError::MissingRequired(slot.to_string()).into());
            }
            SlotValue::Rgb(c) => c,
            SlotValue::PaletteColor(c) => self
                .palette
                .get(&c)
                .copied()
                .ok_or_else(|| ResolveError::UndefinedPaletteColor(c.clone()))?,
            SlotValue::Other(s) => self.resolve_slot(s, cache, visiting)?,
        })
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Scheme {
    pub scheme: String,
    #[serde(flatten)]
    pub meta: Meta,
    #[serde(flatten)]
    pub slots: IndexMap<Slot, SlotValue>,
}

#[must_use]
pub fn resolve(name: &str, raw: Raw) -> Scheme {
    let mut cache = IndexMap::new();
    let mut visiting = IndexSet::new();

    let slots = Slot::iter()
        .filter_map(|s| {
            raw.resolve_slot(s, &mut cache, &mut visiting)
                .ok()
                .map(|c| (s, SlotValue::Rgb(c)))
        })
        .collect();

    Scheme {
        scheme: name.to_owned(),
        meta: raw.meta,
        slots,
    }
}

#[must_use]
pub fn load(name: &str, path: PathBuf) -> Scheme {
    let content = fs::read_to_string(path).unwrap();
    let raw = toml::from_str(&content).unwrap();

    resolve(name, raw)
}

#[must_use]
pub fn load_all(dir: &str) -> IndexMap<String, Scheme> {
    let mut schemes = IndexMap::new();

    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.extension().unwrap() == "toml" {
            let name = path.file_stem().unwrap().to_str().unwrap();
            let s = load(name, path.clone());
            schemes.insert(name.to_owned(), s);
        }
    }

    schemes
}
