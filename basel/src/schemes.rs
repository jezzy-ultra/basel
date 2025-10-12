use std::collections::BTreeMap;
use std::fs;
use std::io::Error as IoError;
use std::path::Path;
use std::result::Result as StdResult;
use std::sync::Arc;

use indexmap::{IndexMap, IndexSet};
use minijinja::Value as JinjaValue;
use serde::de::Error as SerdeDeError;
use serde::{Deserialize, Serialize};
use serde_json::Error as JsonError;
use toml::de::Error as TomlDeError;
use toml::{Table, Value as TomlValue};
use walkdir::WalkDir;

use crate::slots::{self, Error as SlotError, SlotKind, SlotName, SlotValue};
use crate::swatches::{
    ColorObject, Error as SwatchError, RenderConfig, Swatch, check_ascii_collisions,
    check_case_collisions,
};
use crate::{ColorFormat, Special, TextFormat, is_toml};

const MAX_META_FIELD_LENGTH: usize = 1000;

fn validate_meta_field(name: &str, value: Option<&String>) -> Result<()> {
    if let Some(text) = value
        && text.len() > MAX_META_FIELD_LENGTH
    {
        return Err(Error::InvalidMeta {
            field: name.to_owned(),
            reason: format!(
                "too long ({} characters; max is {MAX_META_FIELD_LENGTH})",
                text.len()
            ),
        });
    }

    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("slot resolution error: {0}")]
    Slot(#[from] SlotError),
    #[error("failed to parse palette: {0}")]
    Swatch(#[from] SwatchError),
    #[error("slot `{slot}` references non-existent swatch `{swatch}`")]
    UndefinedSwatch { slot: String, swatch: String },
    #[error("invalid toml syntax in `{path}`: {src}")]
    ParsingRaw { path: String, src: TomlDeError },
    #[error("invalid meta field `{field}`: {reason}")]
    InvalidMeta { field: String, reason: String },
    #[error("invalid slots structure in `{path}`: {reason}")]
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

pub type Result<T> = StdResult<T, Error>;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResolvedSlot {
    pub hex: String,
    pub swatch: String,
    pub ascii: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Meta {
    pub author: Option<String>,
    pub author_ascii: Option<String>,
    pub license: Option<String>,
    pub license_ascii: Option<String>,
    pub blurb: Option<String>,
    pub blurb_ascii: Option<String>,
}

#[derive(Debug, Serialize)]
struct RawScheme {
    meta: Meta,
    palette: IndexSet<Swatch>,
    slots: IndexMap<SlotName, SlotValue>,
}

impl RawScheme {
    fn parse_slot(
        slot_key: &str,
        val: &TomlValue,
        path: &str,
        parsed: &mut IndexMap<SlotName, SlotValue>,
    ) -> Result<()> {
        let slot_name = slot_key
            .parse()
            .map_err(|_src| Error::InvalidSlotsStructure {
                path: path.to_owned(),
                reason: format!("invalid slot name: `{slot_key}`"),
            })?;

        let val_str = val.as_str().ok_or_else(|| Error::InvalidSlotsStructure {
            path: path.to_owned(),
            reason: format!("slot `{slot_key}` must be a string"),
        })?;

        parsed.insert(slot_name, SlotValue::parse(val_str)?);
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
        slot: SlotName,
        visited: &mut IndexSet<SlotName>,
    ) -> Result<ResolvedSlot> {
        if !visited.insert(slot) {
            let mut chain: Vec<String> = visited.iter().map(ToString::to_string).collect();
            chain.push(slot.to_string());

            return Err(Error::Slot(SlotError::CircularReference(chain)));
        }

        match self.slots.get(&slot) {
            Some(SlotValue::Swatch(display_name)) => {
                self.palette.get(display_name.as_str()).map_or_else(
                    || {
                        Err(Error::UndefinedSwatch {
                            swatch: display_name.to_string(),
                            slot: slot.to_string(),
                        })
                    },
                    |swatch| {
                        Ok(ResolvedSlot {
                            hex: swatch.hex().to_string(),
                            swatch: swatch.name().to_string(),
                            ascii: swatch.ascii().to_string(),
                        })
                    },
                )
            }
            Some(SlotValue::Slot(slot_name)) => self.resolve_slot(*slot_name, visited),
            None => match slot.classify() {
                SlotKind::Base(_) => Err(SlotError::MissingRequired(slot.to_string()).into()),
                SlotKind::Optional(opt) => self.resolve_slot(*opt.base(), visited),
            },
        }
    }

    fn resolve_slots(&self) -> Result<IndexMap<SlotName, ResolvedSlot>> {
        let mut resolved_slots = IndexMap::new();
        let mut missing_slots: Vec<String> = Vec::new();

        for base_slot in slots::base() {
            if !self.slots.contains_key(&base_slot) {
                missing_slots.push(base_slot.to_string());
            }
        }

        if !missing_slots.is_empty() {
            return Err(SlotError::MissingRequired(format!(
                "missing required slots: {}",
                missing_slots.join(", ")
            ))
            .into());
        }

        for slot in slots::iter() {
            let mut visited = IndexSet::new();
            match self.resolve_slot(slot, &mut visited) {
                Ok(resolved) => {
                    resolved_slots.insert(slot, resolved);
                }
                Err(e) => return Err(e),
            }
        }

        Ok(resolved_slots)
    }

    fn parse_palette(val: &TomlValue, path: &str) -> Result<IndexSet<Swatch>> {
        let table = val.as_table().ok_or_else(|| Error::Deserializing {
            section: "palette".to_owned(),
            path: path.to_owned(),
            src: Box::new(<TomlDeError as SerdeDeError>::custom(
                "palette must be a table",
            )),
        })?;

        let mut palette = IndexSet::new();

        for (display_key, v) in table {
            let swatch = Swatch::parse(display_key, v)?;
            palette.insert(swatch);
        }

        check_ascii_collisions(&palette)?;
        check_case_collisions(&palette)?;

        Ok(palette)
    }

    fn into_scheme(self, name: &str) -> Result<Scheme> {
        let resolved_slots = self.resolve_slots()?;

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
    pub palette: IndexSet<Swatch>,
    #[serde(skip)]
    pub slots: IndexMap<SlotName, SlotValue>,
    #[serde(flatten)]
    pub resolved_slots: IndexMap<SlotName, ResolvedSlot>,
}

impl Scheme {
    fn ascii_fallback(unicode: Option<&String>, ascii: Option<&String>) -> Option<String> {
        ascii
            .cloned()
            .or_else(|| unicode.map(|s| deunicode::deunicode(s)))
    }

    fn insert_meta(&self, context: &mut BTreeMap<String, JinjaValue>, config: &Arc<RenderConfig>) {
        context.insert(
            "SCHEME".to_owned(),
            JinjaValue::from_serialize(&self.scheme),
        );

        let author_ascii =
            Self::ascii_fallback(self.meta.author.as_ref(), self.meta.author_ascii.as_ref());
        let license_ascii =
            Self::ascii_fallback(self.meta.license.as_ref(), self.meta.license_ascii.as_ref());
        let blurb_ascii =
            Self::ascii_fallback(self.meta.blurb.as_ref(), self.meta.blurb_ascii.as_ref());

        let meta_ctx = if config.text_format == TextFormat::Ascii {
            Meta {
                author: author_ascii.clone(),
                author_ascii,
                license: license_ascii.clone(),
                license_ascii,
                blurb: blurb_ascii.clone(),
                blurb_ascii,
            }
        } else {
            self.meta.clone()
        };

        context.insert("meta".to_owned(), JinjaValue::from_serialize(&meta_ctx));
    }

    fn map_swatches_to_slots(&self) -> IndexMap<String, Vec<String>> {
        let mut map: IndexMap<String, Vec<String>> = IndexMap::new();

        for swatch in &self.palette {
            map.insert(swatch.name().to_string(), Vec::new());
        }

        for (slot_name, resolved_slot) in &self.resolved_slots {
            if let Some(slots) = map.get_mut(&resolved_slot.swatch) {
                slots.push(slot_name.to_string());
            }
        }

        map
    }

    fn insert_palette(
        &self,
        context: &mut BTreeMap<String, JinjaValue>,
        swatch_slots: &IndexMap<String, Vec<String>>,
        config: &Arc<RenderConfig>,
    ) {
        let palette: Vec<JinjaValue> = self
            .palette
            .iter()
            .map(|swatch| {
                let name = swatch.name().to_string();

                let slots = swatch_slots.get(&name).cloned().unwrap_or_default();

                JinjaValue::from_serialize(ColorObject::swatch(
                    swatch.hex().to_string(),
                    name,
                    swatch.ascii().to_string(),
                    swatch.hex().color().split_rgb(),
                    slots,
                    Arc::clone(config),
                ))
            })
            .collect();

        context.insert("palette".to_owned(), JinjaValue::from(palette));
    }

    fn rgb(&self, slot_name: &SlotName, resolved_slot: &ResolvedSlot) -> Result<(u8, u8, u8)> {
        self.palette
            .get(resolved_slot.swatch.as_str())
            .ok_or_else(|| {
                Error::InternalBug(format!(
                    "resolved slot `{slot_name}` references missing swatch `${}`",
                    &resolved_slot.swatch
                ))
            })
            .map(|swatch| swatch.hex().color().split_rgb())
    }

    fn insert_grouped_slot(
        slot_obj: ColorObject,
        groups: &mut BTreeMap<String, BTreeMap<String, JinjaValue>>,
        group: &str,
        key: &str,
    ) {
        groups
            .entry(group.to_owned())
            .or_default()
            .insert(key.to_owned(), JinjaValue::from_object(slot_obj));
    }

    fn insert_slot(
        &self,
        context: &mut BTreeMap<String, JinjaValue>,
        groups: &mut BTreeMap<String, BTreeMap<String, JinjaValue>>,
        slot_name: &SlotName,
        resolved_slot: &ResolvedSlot,
        config: &Arc<RenderConfig>,
    ) -> Result<()> {
        let parts: Vec<&str> = slot_name.as_str().split('.').collect();

        let rgb = self.rgb(slot_name, resolved_slot)?;

        let obj = ColorObject::slot(
            resolved_slot.hex.clone(),
            resolved_slot.swatch.clone(),
            resolved_slot.ascii.clone(),
            rgb,
            Arc::clone(config),
        );

        match parts.as_slice() {
            [key] => {
                context.insert((*key).to_owned(), JinjaValue::from_object(obj));
            }
            [group, key] => {
                Self::insert_grouped_slot(obj, groups, group, key);
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
        context: &mut BTreeMap<String, JinjaValue>,
        swatch_name: &str,
        swatch_slots: &IndexMap<String, Vec<String>>,
        config: &Arc<RenderConfig>,
    ) -> Result<()> {
        let swatch = self.palette.get(swatch_name).ok_or_else(|| {
            Error::InternalBug(format!(
                "current swatch `{swatch_name}` not in palette, but we should only be receiving \
                 valid swatch names"
            ))
        })?;

        let slots = swatch_slots.get(swatch_name).cloned().unwrap_or_default();

        let obj = ColorObject::swatch(
            swatch.hex().to_string(),
            swatch.name().to_string(),
            swatch.ascii().to_string(),
            swatch.hex().color().split_rgb(),
            slots,
            Arc::clone(config),
        );

        context.insert("SWATCH".to_owned(), JinjaValue::from_object(obj));

        Ok(())
    }

    fn insert_set_test_slots(&self, context: &mut BTreeMap<String, JinjaValue>) {
        let set_slots: Vec<String> = self.slots.keys().map(ToString::to_string).collect();
        context.insert("_set".to_owned(), JinjaValue::from(set_slots));
    }

    fn insert_special(context: &mut BTreeMap<String, JinjaValue>, special: &Special) {
        let mut special_map = BTreeMap::new();

        special_map.insert(
            "upstream_file".to_owned(),
            JinjaValue::from(special.upstream_file.as_deref().unwrap_or("")),
        );

        special_map.insert(
            "upstream_repo".to_owned(),
            JinjaValue::from(special.upstream_repo.as_deref().unwrap_or("")),
        );

        context.insert("special".to_owned(), JinjaValue::from(special_map));
    }

    pub fn to_context(
        &self,
        color_format: ColorFormat,
        text_format: TextFormat,
        special: &Special,
        current_swatch: Option<&str>,
    ) -> Result<BTreeMap<String, JinjaValue>> {
        let mut ctx = BTreeMap::new();

        let mut groups: BTreeMap<String, BTreeMap<String, JinjaValue>> = BTreeMap::new();

        let config = RenderConfig::new(color_format, text_format);

        let swatch_slots = Self::map_swatches_to_slots(self);

        self.insert_meta(&mut ctx, &config);
        self.insert_palette(&mut ctx, &swatch_slots, &config);

        for (slot_name, resolved_slot) in &self.resolved_slots {
            self.insert_slot(&mut ctx, &mut groups, slot_name, resolved_slot, &config)?;
        }

        for (group_name, group_map) in groups {
            ctx.insert(group_name, JinjaValue::from(group_map));
        }

        if let Some(name) = current_swatch {
            self.insert_current_swatch(&mut ctx, name, &swatch_slots, &config)?;
        }

        Self::insert_special(&mut ctx, special);
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

    validate_meta_field("author", meta.author.as_ref())?;
    validate_meta_field("author_ascii", meta.author_ascii.as_ref())?;
    validate_meta_field("license", meta.license.as_ref())?;
    validate_meta_field("license_ascii", meta.license_ascii.as_ref())?;
    validate_meta_field("blurb", meta.blurb.as_ref())?;
    validate_meta_field("blurb_ascii", meta.blurb_ascii.as_ref())?;

    let palette_val = root.get("palette").ok_or_else(|| Error::Deserializing {
        section: "palette".to_owned(),
        path: path.clone(),
        src: Box::new(<TomlDeError as SerdeDeError>::missing_field("palette")),
    })?;

    let palette = RawScheme::parse_palette(palette_val, &path)?;

    let slots_val = root.get("slots").ok_or_else(|| Error::Deserializing {
        section: "slots".to_owned(),
        path: path.clone(),
        src: Box::new(<TomlDeError as SerdeDeError>::missing_field("slots")),
    })?;

    let slots = RawScheme::parse_slots(slots_val, &path)?;

    let raw = RawScheme {
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
        if path.is_file() && is_toml(path) {
            let name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| {
                    Error::InternalBug(format!(
                        "attempted to load scheme with corrupted path `{}`",
                        path.display(),
                    ))
                })?;
            let scheme = load(name, path)?;
            schemes.insert(name.to_owned(), scheme);
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
