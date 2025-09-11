use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

use hex_color::{Case, HexColor};
use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize};
use strum::{self, Display, EnumIter, EnumProperty, EnumString, IntoEnumIterator};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ResolveError {
    #[error("undefined slot `{0}`")]
    UndefinedSlot(String),
    #[error("undefined palettel color `{0}`")]
    UndefinedPaletteColor(String),
    #[error("circular reference")]
    Circular,
    #[error("required slot `{0}` missing")]
    MissingRequired(String),
}

pub fn load_all(dir: &str) -> Vec<Scheme> {
    let mut schemes = Vec::new();

    for entry in fs::read_dir(dir).unwrap().flatten() {
        if entry.path().extension().unwrap() == "toml" {
            let s = load(entry.path());
            schemes.push(s);
        }
    }

    schemes
}

pub fn load(path: PathBuf) -> Scheme {
    let name = path.file_stem().unwrap().to_string_lossy().to_string();
    let content = fs::read_to_string(path).unwrap();
    let raw = toml::from_str(&content).unwrap();

    resolve(name, raw)
}

pub fn resolve(name: String, raw: Raw) -> Scheme {
    let mut cache = HashMap::new();
    let mut visiting = HashSet::new();

    let slots = Slot::iter()
        .filter_map(|s| {
            raw.resolve_slot(s, &mut cache, &mut visiting)
                .ok()
                .and_then(|hex| HexColor::parse(&hex).ok())
                .map(|rgb| (s, SlotValue::Rgb(rgb)))
        })
        .collect();

    Scheme {
        scheme: name,
        meta: raw.meta,
        slots,
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

#[derive(Serialize, Deserialize, Debug)]
pub struct Raw {
    #[serde(flatten)]
    pub meta: Meta,
    pub palette: IndexMap<String, HexColor>,
    pub slots: IndexMap<Slot, SlotValue>,
}

impl Raw {
    fn resolve_slot(
        &self,
        slot: Slot,
        cache: &mut HashMap<Slot, String>,
        visiting: &mut HashSet<Slot>,
    ) -> Result<String, ResolveError> {
        if let Some(c) = cache.get(&slot) {
            return Ok(c.clone());
        }
        if !visiting.insert(slot) {
            return Err(ResolveError::Circular);
        }

        let val =
            self.slots
                .get(&slot)
                .cloned()
                .unwrap_or_else(|| match slot.get_str("fallback") {
                    None => {
                        panic!("missing required slot `{slot}`")
                    }
                    Some(s) => SlotValue::Other(Slot::from_str(s).unwrap()),
                });
        let out = self.color_from(&val, &slot, cache, visiting)?;

        visiting.remove(&slot);
        cache.insert(slot, out.clone());
        Ok(out)
    }

    fn color_from(
        &self,
        val: &SlotValue,
        slot: &Slot,
        cache: &mut HashMap<Slot, String>,
        visiting: &mut HashSet<Slot>,
    ) -> Result<String, ResolveError> {
        Ok(match val {
            SlotValue::None => return Err(ResolveError::MissingRequired(slot.clone().to_string())),
            SlotValue::Rgb(h) => h.display_rgb().with_case(Case::Lower).to_string(),
            SlotValue::PaletteColor(p) => self
                .palette
                .get(p)
                .ok_or_else(|| ResolveError::UndefinedPaletteColor(p.clone()))?
                .display_rgb()
                .with_case(Case::Lower)
                .to_string(),
            SlotValue::Other(s) => self.resolve_slot(*s, cache, visiting)?,
        })
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Meta {
    pub author: Option<String>,
    pub license: Option<String>,
    pub blurb: Option<String>,
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

#[derive(Serialize, Clone, Debug)]
pub enum SlotValue {
    Other(Slot),
    PaletteColor(String),
    Rgb(HexColor),
    None,
}

impl<'de> Deserialize<'de> for SlotValue {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let val = String::deserialize(d)?;
        Ok(if val == "none" {
            SlotValue::None
        } else if let Ok(hex) = HexColor::parse(&val) {
            SlotValue::Rgb(hex)
        } else if let Some(name) = val.strip_prefix('$') {
            SlotValue::PaletteColor(name.to_string())
        } else if let Ok(slot) = Slot::from_str(&val) {
            SlotValue::Other(slot)
        } else {
            return Err(serde::de::Error::custom(format!("bad slot value: {val}")));
        })
    }
}
