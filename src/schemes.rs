use std::collections::BTreeMap;
use std::fmt::{Formatter, Result as FmtResult};
use std::fs;
use std::io::Error as IoError;
use std::ops::Deref;
use std::path::Path;
use std::result::Result as StdResult;
use std::sync::Arc;

use hex_color::{Case, Display as HexDisplay, HexColor, ParseHexColorError};
use indexmap::{IndexMap, IndexSet};
use minijinja::Value as JinjaValue;
use minijinja::value::{Enumerator, Object as JinjaObject};
use serde::de::Error as SerdeDeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Error as JsonError;
use toml::de::Error as TomlDeError;
use toml::{Table, Value as TomlValue};
use walkdir::WalkDir;

use crate::slots::{self, Error as SlotError, SlotKind, SlotName, SlotValue};
use crate::upstream;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("slot resolution error: {0}")]
    Slot(#[from] SlotError),
    #[error("undefined palette color `{0}`")]
    UndefinedSwatch(String),
    #[error("hex parse error: {0}")]
    ParsingHex(#[from] ParseHexColorError),
    #[error("invalid TOML syntax in `{path}`: {src}")]
    ParsingRaw { path: String, src: TomlDeError },
    #[error("invalid slots structure {path}: {reason}")]
    InvalidSlotsStructure { path: String, reason: String },
    #[error("failed to deserialize `{section}` section in `{path}`: {src}")]
    Deserializing {
        section: String,
        path: String,
        src: Box<TomlDeError>,
    },
    #[error("failed to serialize scheme: {src}")]
    Serializing { src: JsonError },
    #[error("failed to read scheme `{path}`: {src}")]
    ReadingFile { path: String, src: IoError },
    #[error("{0}")]
    InternalBug(String),
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Swatch(HexDisplay);

impl Swatch {
    fn new<T>(input: T) -> Result<Self>
    where
        Self: TryFrom<T, Error = Error>,
    {
        Self::try_from(input)
    }

    #[must_use]
    pub const fn hex(self) -> HexColor {
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
    type Error = Error;

    fn try_from(s: &str) -> Result<Self> {
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
    fn serialize<S>(&self, serializer: S) -> StdResult<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Swatch {
    fn deserialize<D>(deserializer: D) -> StdResult<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::new(s.as_str()).map_err(SerdeDeError::custom)
    }
}

#[derive(Debug, Serialize)]
struct ColorObject {
    hex: String,
    name: String,
    rgb: (u8, u8, u8),
    render_as_name: bool,
}

impl ColorObject {
    const fn new(hex: String, name: String, rgb: (u8, u8, u8), render_as_name: bool) -> Self {
        Self {
            hex,
            name,
            rgb,
            render_as_name,
        }
    }
}

impl JinjaObject for ColorObject {
    fn render(self: &Arc<Self>, f: &mut Formatter<'_>) -> FmtResult {
        write!(
            f,
            "{}",
            if self.render_as_name {
                &self.name
            } else {
                &self.hex
            }
        )
    }

    fn get_value(self: &Arc<Self>, key: &JinjaValue) -> Option<JinjaValue> {
        let (r, g, b) = self.rgb;
        match key.as_str()? {
            "hex" => Some(JinjaValue::from(&self.hex)),
            "name" => Some(JinjaValue::from(&self.name)),
            "r" => Some(JinjaValue::from(r)),
            "g" => Some(JinjaValue::from(g)),
            "b" => Some(JinjaValue::from(b)),
            "rf" => Some(JinjaValue::from(f64::from(r) / 255.0)),
            "gf" => Some(JinjaValue::from(f64::from(g) / 255.0)),
            "bf" => Some(JinjaValue::from(f64::from(b) / 255.0)),
            _ => None,
        }
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::Str(&["hex", "name", "r", "g", "b", "rf", "gf", "bf"])
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResolvedSlot {
    pub swatch_name: String,
    pub hex: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Meta {
    pub author: Option<String>,
    pub license: Option<String>,
    pub blurb: Option<String>,
}

#[derive(Debug, Serialize)]
struct Raw {
    meta: Meta,
    palette: IndexMap<String, Swatch>,
    slots: IndexMap<SlotName, SlotValue>,
}

impl Raw {
    fn parse_slot(
        slot_key: &str,
        val: &TomlValue,
        path: &str,
        result: &mut IndexMap<SlotName, SlotValue>,
    ) -> Result<()> {
        let slot_name = SlotName::parse(slot_key).map_err(|_src| Error::InvalidSlotsStructure {
            path: path.to_owned(),
            reason: format!("invalid slot name: `{slot_key}`"),
        })?;

        let val_str = val.as_str().ok_or_else(|| Error::InvalidSlotsStructure {
            path: path.to_owned(),
            reason: format!("slot `{slot_key}` must be a string"),
        })?;

        result.insert(slot_name, SlotValue::parse(val_str)?);
        Ok(())
    }

    fn parse_slots(
        slots_val: &toml::Value,
        path: &String,
    ) -> Result<IndexMap<SlotName, SlotValue>> {
        let mut result = IndexMap::new();

        let table = slots_val
            .as_table()
            .ok_or_else(|| Error::InvalidSlotsStructure {
                path: path.to_owned(),
                reason: "`slots` must be a table".to_owned(),
            })?;

        for (key, val) in table {
            if let Some(nested_table) = val.as_table() {
                for (nested_key, nested_val) in nested_table {
                    let full_key = format!("{key}.{nested_key}");
                    Self::parse_slot(&full_key, nested_val, path, &mut result)?;
                }
            } else {
                Self::parse_slot(key, val, path, &mut result)?;
            }
        }

        Ok(result)
    }

    fn resolve_slot(
        &self,
        slot: &SlotName,
        visited: &mut IndexSet<SlotName>,
    ) -> Result<ResolvedSlot> {
        if !visited.insert(slot.to_owned()) {
            let mut chain: Vec<String> = visited.iter().map(ToString::to_string).collect();
            chain.push(slot.to_string());

            return Err(Error::Slot(SlotError::CircularReference(chain)));
        }

        match self.slots.get(slot) {
            Some(SlotValue::Swatch(swatch_name)) => self.palette.get(swatch_name).map_or_else(
                || Err(Error::UndefinedSwatch(swatch_name.to_owned())),
                |swatch| {
                    Ok(ResolvedSlot {
                        swatch_name: swatch_name.to_owned(),
                        hex: swatch.to_string(),
                    })
                },
            ),
            Some(SlotValue::Slot(slot_name)) => self.resolve_slot(slot_name, visited),
            None => match slot.clone().classify() {
                SlotKind::Base(_) => Err(SlotError::MissingRequired(slot.to_string()).into()),
                SlotKind::Optional(opt) => self.resolve_slot(&opt.base, visited),
            },
        }
    }

    fn resolve_all(&self) -> Result<IndexMap<SlotName, ResolvedSlot>> {
        let mut resolved_slots = IndexMap::new();
        let mut missing: Vec<String> = Vec::new();

        for base_slot in slots::base() {
            if !self.slots.contains_key(&base_slot) {
                missing.push(base_slot.to_string());
            }
        }

        if !missing.is_empty() {
            return Err(SlotError::MissingRequired(format!(
                "missing required slots: {}",
                missing.join(", ")
            ))
            .into());
        }

        for slot in slots::iter() {
            let mut visited = IndexSet::new();
            match self.resolve_slot(&slot, &mut visited) {
                Ok(resolved) => {
                    resolved_slots.insert(slot, resolved);
                }
                Err(e) => return Err(e),
            }
        }

        Ok(resolved_slots)
    }

    fn into_scheme(self, name: &str) -> Result<Scheme> {
        let resolved_slots = self.resolve_all()?;

        Ok(Scheme {
            scheme: name.to_owned(),
            meta: self.meta,
            palette: self.palette,
            slots: self.slots,
            resolved_slots,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct Scheme {
    #[serde(rename(serialize = "SCHEME"))]
    pub scheme: String,
    pub meta: Meta,
    pub palette: IndexMap<String, Swatch>,
    #[serde(skip)]
    pub slots: IndexMap<SlotName, SlotValue>,
    #[serde(flatten)]
    pub resolved_slots: IndexMap<SlotName, ResolvedSlot>,
}

impl Scheme {
    fn insert_static_fields(&self, ctx: &mut BTreeMap<String, JinjaValue>) {
        ctx.insert(
            "SCHEME".to_owned(),
            JinjaValue::from_serialize(&self.scheme),
        );
        ctx.insert("meta".to_owned(), JinjaValue::from_serialize(&self.meta));

        let palette: Vec<JinjaValue> = self
            .palette
            .iter()
            .map(|(name, swatch)| {
                JinjaValue::from_serialize(ColorObject {
                    hex: swatch.to_string(),
                    name: name.clone(),
                    rgb: swatch.hex().split_rgb(),
                    render_as_name: false,
                })
            })
            .collect();
        ctx.insert("palette".to_owned(), JinjaValue::from(palette));
    }

    fn rgb(&self, slot_name: &SlotName, resolved_slot: &ResolvedSlot) -> Result<(u8, u8, u8)> {
        self.palette
            .get(&resolved_slot.swatch_name)
            .ok_or_else(|| {
                Error::InternalBug(format!(
                    "resolved slot `{slot_name}` references missing swatch `${}`",
                    &resolved_slot.swatch_name
                ))
            })
            .map(|swatch| swatch.hex().split_rgb())
    }

    fn insert_grouped_slot(
        groups: &mut BTreeMap<String, BTreeMap<String, JinjaValue>>,
        group: &str,
        key: &str,
        slot_obj: ColorObject,
    ) {
        groups
            .entry(group.to_owned())
            .or_default()
            .insert(key.to_owned(), JinjaValue::from_object(slot_obj));
    }

    fn insert_slot(
        &self,
        ctx: &mut BTreeMap<String, JinjaValue>,
        groups: &mut BTreeMap<String, BTreeMap<String, JinjaValue>>,
        slot_name: &SlotName,
        resolved_slot: &ResolvedSlot,
        render_swatch_names: bool,
    ) -> Result<()> {
        let parts: Vec<&str> = slot_name.split('.').collect();
        let rgb = self.rgb(slot_name, resolved_slot)?;

        let obj = ColorObject::new(
            resolved_slot.hex.clone(),
            resolved_slot.swatch_name.clone(),
            rgb,
            render_swatch_names,
        );

        match parts.as_slice() {
            [key] => {
                ctx.insert((*key).to_owned(), JinjaValue::from_object(obj));
            }
            [group, key] => {
                Self::insert_grouped_slot(groups, group, key, obj);
            }
            _ => {
                return Err(Error::InternalBug(format!(
                    "slot {slot_name} not formatted like `[group.]slot`"
                )));
            }
        }

        Ok(())
    }

    fn insert_current_swatch(
        &self,
        ctx: &mut BTreeMap<String, JinjaValue>,
        swatch_name: &str,
        render_swatch_names: bool,
    ) -> Result<()> {
        let swatch = self.palette.get(swatch_name).ok_or_else(|| {
            Error::InternalBug(format!(
                "current swatch `{swatch_name}` not in palette, but we should only be receiving \
                 valid swatch names"
            ))
        })?;

        let rgb = swatch.hex().split_rgb();
        let obj = ColorObject::new(
            swatch.to_string(),
            swatch_name.to_owned(),
            rgb,
            render_swatch_names,
        );

        ctx.insert("SWATCH".to_owned(), JinjaValue::from_object(obj));

        Ok(())
    }

    fn insert_set_test_slots(&self, ctx: &mut BTreeMap<String, JinjaValue>) {
        let set_slots: Vec<String> = self.slots.keys().map(ToString::to_string).collect();
        ctx.insert("_set".to_owned(), JinjaValue::from(set_slots));
    }

    fn insert_special_fields(ctx: &mut BTreeMap<String, JinjaValue>, upstream_url: Option<&str>) {
        let mut special = BTreeMap::new();

        if let Some(url) = upstream_url {
            special.insert("upstream_file".to_owned(), JinjaValue::from(url));

            if let Some(base) = upstream::extract_base_url(url) {
                special.insert("upstream_repo".to_owned(), JinjaValue::from(base));
            }
        }

        ctx.insert("special".to_owned(), JinjaValue::from(special));
    }

    pub fn to_context(
        &self,
        render_swatch_names: bool,
        current_swatch: Option<&str>,
        upstream_url: Option<&str>,
    ) -> Result<BTreeMap<String, JinjaValue>> {
        let mut ctx = BTreeMap::new();
        let mut groups: BTreeMap<String, BTreeMap<String, JinjaValue>> = BTreeMap::new();

        self.insert_static_fields(&mut ctx);

        for (slot_name, resolved_slot) in &self.resolved_slots {
            self.insert_slot(
                &mut ctx,
                &mut groups,
                slot_name,
                resolved_slot,
                render_swatch_names,
            )?;
        }

        for (group_name, group_map) in groups {
            ctx.insert(group_name, JinjaValue::from(group_map));
        }

        if let Some(name) = current_swatch {
            self.insert_current_swatch(&mut ctx, name, render_swatch_names)?;
        }

        Self::insert_special_fields(&mut ctx, upstream_url);
        self.insert_set_test_slots(&mut ctx);

        Ok(ctx)
    }
}

pub fn load(name: &str, path: &Path) -> Result<Scheme> {
    let path = path.to_string_lossy().to_string();
    let content = fs::read_to_string(&path).map_err(|src| Error::ReadingFile {
        path: path.clone(),
        src,
    })?;
    let root: Table = toml::from_str(&content).map_err(|src| Error::ParsingRaw {
        path: path.clone(),
        src,
    })?;
    let meta: Meta = root
        .get("meta")
        .map(|v| {
            v.clone().try_into().map_err(|src| Error::Deserializing {
                section: "meta".to_owned(),
                path: path.clone(),
                src: Box::new(src),
            })
        })
        .transpose()?
        .unwrap_or_default();
    let palette: IndexMap<String, Swatch> = root
        .get("palette")
        .ok_or_else(|| Error::Deserializing {
            section: "palette".to_owned(),
            path: path.clone(),
            src: Box::new(<TomlDeError as SerdeDeError>::missing_field("palette")),
        })?
        .clone()
        .try_into()
        .map_err(|src| Error::Deserializing {
            section: "palette".to_owned(),
            path: path.clone(),
            src: Box::new(src),
        })?;
    for swatch_name in palette.keys() {
        if swatch_name.is_empty() {
            return Err(Error::InternalBug(format!("empty swatch name in `{path}`")));
        }
    }
    let slots_value = root.get("slots").ok_or_else(|| Error::Deserializing {
        section: "slots".to_owned(),
        path: path.clone(),
        src: Box::new(<TomlDeError as SerdeDeError>::missing_field("slots")),
    })?;
    let slots = Raw::parse_slots(slots_value, &path)?;
    let raw = Raw {
        meta,
        palette,
        slots,
    };

    raw.into_scheme(name)
}

pub fn load_all(dir: &str) -> Result<IndexMap<String, Scheme>> {
    let mut schemes = IndexMap::new();

    for entry in WalkDir::new(dir).into_iter().filter_map(StdResult::ok) {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
            let name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| {
                    Error::InternalBug(format!(
                        "attempted to load scheme with corrupted path `{}`",
                        path.display(),
                    ))
                })?;
            let s = load(name, path)?;
            schemes.insert(name.to_owned(), s);
        }
    }

    Ok(schemes)
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use indoc::indoc;
    use tempfile::NamedTempFile;

    use super::*;

    fn create_temp_scheme_file(toml: &str) -> NamedTempFile {
        let mut temp = NamedTempFile::new().expect("failed to create temp file");
        temp.write_all(toml.as_bytes())
            .expect("failed to write temp file");

        temp
    }

    fn scheme_from_toml(name: &str, toml: &str) -> Result<Scheme> {
        let temp = create_temp_scheme_file(toml);

        load(name, temp.path())
    }

    fn assert_slot_hex_equals(context: &BTreeMap<String, JinjaValue>, slot: &str, expected: &str) {
        let obj = context
            .get(slot)
            .unwrap_or_else(|| panic!("slot `{slot}` not found in context`"));
        let actual = obj
            .get_attr("hex")
            .unwrap_or_else(|_| panic!("slot `{slot}` missing `hex` field"));

        assert_eq!(actual, expected.into(), "slot `{slot}` has wrong hex value");
    }

    fn assert_nested_slot_hex_equals(
        context: &BTreeMap<String, JinjaValue>,
        group: &str,
        slot: &str,
        expected: &str,
    ) {
        let group_obj = context
            .get(group)
            .unwrap_or_else(|| panic!("group `{group}` not found in context"));
        let slot_obj = group_obj
            .get_attr(slot)
            .unwrap_or_else(|_| panic!("slot `{group}.{slot}` not found in context"));
        let actual = slot_obj
            .get_attr("hex")
            .unwrap_or_else(|_| panic!("slot `{group}.{slot}` missing `hex` field"));

        assert_eq!(
            actual,
            expected.into(),
            "slot `{group}.{slot}` has wrong hex value"
        );
    }

    fn minimal_valid_scheme() -> &'static str {
        indoc! {r##"
            [palette]
            black = "#000"
            red = "#ff0000"
            green = "#00ff00"
            yellow = "#ffff00"
            blue = "#0000ff"
            magenta = "#ff00ff"
            cyan = "#00ffff"
            white = "#fff"

            [slots.ansi]
            black = "$black"
            red = "$red"
            green = "$green"
            yellow = "$yellow"
            blue = "$blue"
            magenta = "$magenta"
            cyan = "$cyan"
            white = "$white"
            "##}
    }
}
