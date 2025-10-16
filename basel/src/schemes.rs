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

use crate::roles::{self, Error as RoleError, RoleKind, RoleName, RoleValue};
use crate::swatches::{
    ColorObject, RenderConfig, Swatch, check_ascii_collisions, check_case_collisions,
};
use crate::{ColorFormat, Result, Special, TextFormat, is_toml, name_type};

pub(crate) const SCHEME_MARKER: &str = "SCHEME";

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("role `{role}` references non-existent swatch `{swatch}`")]
    UndefinedSwatch { role: String, swatch: String },
    #[error("invalid toml syntax in `{path}`: {src}")]
    ParsingRaw { path: String, src: Box<TomlDeError> },
    #[error("invalid meta field `{field}`: {reason}")]
    InvalidMeta { field: String, reason: String },
    #[error("invalid roles structure in `{path}`: {reason}")]
    InvalidRolesStructure { path: String, reason: String },
    #[error("failed to deserialize `{section}` section in `{path}`: {src}")]
    Deserializing {
        section: String,
        path: String,
        src: Box<TomlDeError>,
    },
    #[error("failed to serialize scheme: {src}")]
    Serializing { src: Box<JsonError> },
    #[error("failed to read scheme `{path}`: {src}")]
    ReadingFile { path: String, src: IoError },
}

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
        }
        .into());
    }

    Ok(())
}

#[non_exhaustive]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResolvedRole {
    pub hex: String,
    pub swatch: String,
    pub ascii: String,
}

name_type!(SchemeName, SchemeAsciiName, "scheme");

#[non_exhaustive]
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
    scheme: Option<SchemeName>,
    scheme_ascii: Option<SchemeAsciiName>,
    meta: Meta,
    palette: IndexSet<Swatch>,
    roles: IndexMap<RoleName, RoleValue>,
}

impl RawScheme {
    fn parse_role(
        role_key: &str,
        val: &TomlValue,
        path: &str,
        parsed: &mut IndexMap<RoleName, RoleValue>,
    ) -> Result<()> {
        let role_name = role_key
            .parse()
            .map_err(|_src| Error::InvalidRolesStructure {
                path: path.to_owned(),
                reason: format!("invalid role name: `{role_key}`"),
            })?;

        let val_str = val.as_str().ok_or_else(|| Error::InvalidRolesStructure {
            path: path.to_owned(),
            reason: format!("role `{role_key}` must be a string"),
        })?;

        parsed.insert(role_name, RoleValue::parse(val_str)?);
        Ok(())
    }

    fn parse_roles(
        roles_val: &toml::Value,
        path: &String,
    ) -> Result<IndexMap<RoleName, RoleValue>> {
        let mut result = IndexMap::new();

        let table = roles_val
            .as_table()
            .ok_or_else(|| Error::InvalidRolesStructure {
                path: path.to_owned(),
                reason: "`roles` must be a table".to_owned(),
            })?;

        for (key, val) in table {
            if let Some(nested_table) = val.as_table() {
                for (nested_key, nested_val) in nested_table {
                    let full_key = format!("{key}.{nested_key}");
                    Self::parse_role(&full_key, nested_val, path, &mut result)?;
                }
            } else {
                Self::parse_role(key, val, path, &mut result)?;
            }
        }

        Ok(result)
    }

    fn resolve_role(
        &self,
        role: RoleName,
        visited: &mut IndexSet<RoleName>,
    ) -> Result<ResolvedRole> {
        if !visited.insert(role) {
            let mut chain: Vec<String> = visited.iter().map(ToString::to_string).collect();
            chain.push(role.to_string());

            return Err(crate::Error::Role(RoleError::CircularReference(chain)));
        }

        match self.roles.get(&role) {
            Some(RoleValue::Swatch(display_name)) => {
                Ok(self.palette.get(display_name.as_str()).map_or_else(
                    || {
                        Err(Error::UndefinedSwatch {
                            swatch: display_name.to_string(),
                            role: role.to_string(),
                        })
                    },
                    |swatch| {
                        Ok(ResolvedRole {
                            hex: swatch.hex().to_string(),
                            swatch: swatch.name.to_string(),
                            ascii: swatch.ascii.to_string(),
                        })
                    },
                )?)
            }
            Some(RoleValue::Role(role_name)) => self.resolve_role(*role_name, visited),
            None => match role.classify() {
                RoleKind::Base(_) => Err(RoleError::MissingRequired(role.to_string()).into()),
                RoleKind::Optional(opt) => self.resolve_role(*opt.base(), visited),
            },
        }
    }

    fn resolve_roles(&self) -> Result<IndexMap<RoleName, ResolvedRole>> {
        let mut resolved_roles = IndexMap::new();
        let mut missing_roles: Vec<String> = Vec::new();

        for base_role in roles::base() {
            if !self.roles.contains_key(&base_role) {
                missing_roles.push(base_role.to_string());
            }
        }

        if !missing_roles.is_empty() {
            return Err(RoleError::MissingRequired(format!(
                "missing required roles: {}",
                missing_roles.join(", ")
            ))
            .into());
        }

        for role in roles::iter() {
            let mut visited = IndexSet::new();
            match self.resolve_role(role, &mut visited) {
                Ok(resolved) => {
                    resolved_roles.insert(role, resolved);
                }
                Err(e) => return Err(e),
            }
        }

        Ok(resolved_roles)
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

    fn scheme_names(&self, fallback_name: &str) -> Result<(SchemeName, SchemeAsciiName)> {
        let name = match self.scheme.clone() {
            Some(name) => name,
            None => SchemeName::parse(fallback_name)?,
        };

        let name_ascii = match self.scheme_ascii.clone() {
            Some(ascii) => ascii,
            None => name.to_ascii()?,
        };

        Ok((name, name_ascii))
    }

    fn into_scheme(self, fallback_name: &str) -> Result<Scheme> {
        let resolved_roles = self.resolve_roles()?;
        let (scheme, scheme_ascii) = Self::scheme_names(&self, fallback_name)?;

        Ok(Scheme {
            name: scheme,
            name_ascii: scheme_ascii,
            meta: self.meta.clone(),
            palette: self.palette,
            roles: self.roles,
            resolved_roles,
        })
    }
}

#[non_exhaustive]
#[derive(Debug, Serialize)]
pub struct Scheme {
    #[serde(rename(serialize = "scheme"))]
    pub name: SchemeName,
    #[serde(rename(serialize = "scheme_ascii"))]
    pub name_ascii: SchemeAsciiName,
    pub meta: Meta,
    pub palette: IndexSet<Swatch>,
    #[serde(skip)]
    pub roles: IndexMap<RoleName, RoleValue>,
    #[serde(flatten)]
    pub resolved_roles: IndexMap<RoleName, ResolvedRole>,
}

impl Scheme {
    fn ascii_fallback(unicode: Option<&String>, ascii: Option<&String>) -> Option<String> {
        ascii
            .cloned()
            .or_else(|| unicode.map(|s| deunicode::deunicode(s)))
    }

    fn insert_meta(&self, context: &mut BTreeMap<String, JinjaValue>, config: &Arc<RenderConfig>) {
        context.insert("scheme".to_owned(), JinjaValue::from_serialize(&self.name));
        context.insert(
            "scheme_ascii".to_owned(),
            JinjaValue::from_serialize(&self.name_ascii),
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

    fn map_swatches_to_roles(&self) -> IndexMap<String, Vec<String>> {
        let mut map: IndexMap<String, Vec<String>> = IndexMap::new();

        for swatch in &self.palette {
            map.insert(swatch.name.to_string(), Vec::new());
        }

        for (role_name, resolved_role) in &self.resolved_roles {
            if let Some(roles) = map.get_mut(&resolved_role.swatch) {
                roles.push(role_name.to_string());
            }
        }

        map
    }

    fn insert_palette(
        &self,
        context: &mut BTreeMap<String, JinjaValue>,
        swatch_roles: &IndexMap<String, Vec<String>>,
        config: &Arc<RenderConfig>,
    ) {
        let palette: Vec<JinjaValue> = self
            .palette
            .iter()
            .map(|swatch| {
                let name = swatch.name.to_string();

                let roles = swatch_roles.get(&name).cloned().unwrap_or_default();

                JinjaValue::from_serialize(ColorObject::swatch(
                    swatch.hex().to_string(),
                    name,
                    swatch.ascii.to_string(),
                    swatch.rgb(),
                    roles,
                    Arc::clone(config),
                ))
            })
            .collect();

        context.insert("palette".to_owned(), JinjaValue::from(palette));
    }

    fn rgb(&self, role_name: &RoleName, resolved_role: &ResolvedRole) -> Result<(u8, u8, u8)> {
        self.palette
            .get(resolved_role.swatch.as_str())
            .ok_or_else(|| crate::Error::InternalBug {
                module: "schemes",
                reason: format!(
                    "resolved role `{role_name}` references missing swatch `${}`",
                    &resolved_role.swatch
                ),
            })
            .map(Swatch::rgb)
    }

    fn insert_grouped_role(
        role_obj: ColorObject,
        groups: &mut BTreeMap<String, BTreeMap<String, JinjaValue>>,
        group: &str,
        key: &str,
    ) {
        groups
            .entry(group.to_owned())
            .or_default()
            .insert(key.to_owned(), JinjaValue::from_object(role_obj));
    }

    fn insert_role(
        &self,
        context: &mut BTreeMap<String, JinjaValue>,
        groups: &mut BTreeMap<String, BTreeMap<String, JinjaValue>>,
        role_name: &RoleName,
        resolved_role: &ResolvedRole,
        config: &Arc<RenderConfig>,
    ) -> Result<()> {
        let parts: Vec<&str> = role_name.as_str().split('.').collect();

        let rgb = self.rgb(role_name, resolved_role)?;

        let obj = ColorObject::role(
            resolved_role.hex.clone(),
            resolved_role.swatch.clone(),
            resolved_role.ascii.clone(),
            rgb,
            Arc::clone(config),
        );

        match parts.as_slice() {
            [key] => {
                context.insert((*key).to_owned(), JinjaValue::from_object(obj));
            }
            [group, key] => {
                Self::insert_grouped_role(obj, groups, group, key);
            }
            _ => {
                return Err(crate::Error::InternalBug {
                    module: "schemes",
                    reason: format!("role {role_name} not formatted like `[group.]role`"),
                });
            }
        }

        Ok(())
    }

    fn insert_current_swatch(
        &self,
        context: &mut BTreeMap<String, JinjaValue>,
        swatch_name: &str,
        swatch_roles: &IndexMap<String, Vec<String>>,
        config: &Arc<RenderConfig>,
    ) -> Result<()> {
        let swatch = self
            .palette
            .get(swatch_name)
            .ok_or_else(|| crate::Error::InternalBug {
                module: "schemes",
                reason: format!(
                    "current swatch `{swatch_name}` not in palette, but we should only be \
                     receiving valid swatch names"
                ),
            })?;

        let roles = swatch_roles.get(swatch_name).cloned().unwrap_or_default();

        let obj = ColorObject::swatch(
            swatch.hex().to_string(),
            swatch.name.to_string(),
            swatch.ascii.to_string(),
            swatch.rgb(),
            roles,
            Arc::clone(config),
        );

        context.insert("swatch".to_owned(), JinjaValue::from_object(obj));

        Ok(())
    }

    fn insert_set_test_roles(&self, context: &mut BTreeMap<String, JinjaValue>) {
        let set_roles: Vec<String> = self.roles.keys().map(ToString::to_string).collect();
        context.insert("_set".to_owned(), JinjaValue::from(set_roles));
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

        let swatch_roles = Self::map_swatches_to_roles(self);

        self.insert_meta(&mut ctx, &config);
        self.insert_palette(&mut ctx, &swatch_roles, &config);

        for (role_name, resolved_role) in &self.resolved_roles {
            self.insert_role(&mut ctx, &mut groups, role_name, resolved_role, &config)?;
        }

        for (group_name, group_map) in groups {
            ctx.insert(group_name, JinjaValue::from(group_map));
        }

        if let Some(name) = current_swatch {
            self.insert_current_swatch(&mut ctx, name, &swatch_roles, &config)?;
        }

        Self::insert_special(&mut ctx, special);
        self.insert_set_test_roles(&mut ctx);

        Ok(ctx)
    }
}

pub fn load(name: &str, path: &Path) -> Result<Scheme> {
    let path_str = path.to_string_lossy().to_string();

    let content = fs::read_to_string(&path_str).map_err(|src| Error::ReadingFile {
        path: path_str.clone(),
        src,
    })?;

    let root: Table = toml::from_str(&content).map_err(|src| Error::ParsingRaw {
        path: path_str.clone(),
        src: Box::new(src),
    })?;

    let scheme: Option<SchemeName> = root
        .get("scheme")
        .map(|v| {
            let s = v.as_str().ok_or_else(|| Error::Deserializing {
                section: "scheme".to_owned(),
                path: path_str.clone(),
                src: Box::new(<TomlDeError as SerdeDeError>::custom(
                    "`scheme` must be a string",
                )),
            })?;
            SchemeName::parse(s).map_err(|e| Error::Deserializing {
                section: "scheme".to_owned(),
                path: path_str.clone(),
                src: Box::new(<TomlDeError as SerdeDeError>::custom(format!("{e}"))),
            })
        })
        .transpose()?;

    let scheme_ascii: Option<SchemeAsciiName> = root
        .get("scheme_ascii")
        .map(|v| {
            let s = v.as_str().ok_or_else(|| Error::Deserializing {
                section: "scheme_ascii".to_owned(),
                path: path_str.clone(),
                src: Box::new(<TomlDeError as SerdeDeError>::custom(
                    "`scheme_ascii` must be a string",
                )),
            })?;
            SchemeAsciiName::parse(s).map_err(|e| Error::Deserializing {
                section: "scheme_ascii".to_owned(),
                path: path_str.clone(),
                src: Box::new(<TomlDeError as SerdeDeError>::custom(format!("{e}"))),
            })
        })
        .transpose()?;

    let meta: Meta = root
        .get("meta")
        .map(|v| {
            v.clone().try_into().map_err(|src| Error::Deserializing {
                section: "meta".to_owned(),
                path: path_str.clone(),
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
        path: path_str.clone(),
        src: Box::new(<TomlDeError as SerdeDeError>::missing_field("palette")),
    })?;

    let palette = RawScheme::parse_palette(palette_val, &path_str)?;

    let roles_val = root.get("roles").ok_or_else(|| Error::Deserializing {
        section: "roles".to_owned(),
        path: path_str.clone(),
        src: Box::new(<TomlDeError as SerdeDeError>::missing_field("roles")),
    })?;

    let roles = RawScheme::parse_roles(roles_val, &path_str)?;

    let raw = RawScheme {
        scheme,
        scheme_ascii,
        meta,
        palette,
        roles,
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
                .ok_or_else(|| crate::Error::InternalBug {
                    module: "schemes",
                    reason: format!(
                        "attempted to load scheme with corrupted path `{}`",
                        path.display(),
                    ),
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

    fn assert_role_hex_equals(context: &BTreeMap<String, JinjaValue>, role: &str, expected: &str) {
        let obj = context
            .get(role)
            .unwrap_or_else(|| panic!("role `{role}` not found in context`"));
        let actual = obj
            .get_attr("hex")
            .unwrap_or_else(|_| panic!("role `{role}` missing `hex` field"));

        assert_eq!(actual, expected.into(), "role `{role}` has wrong hex value");
    }

    fn assert_nested_role_hex_equals(
        context: &BTreeMap<String, JinjaValue>,
        group: &str,
        role: &str,
        expected: &str,
    ) {
        let group_obj = context
            .get(group)
            .unwrap_or_else(|| panic!("group `{group}` not found in context"));
        let role_obj = group_obj
            .get_attr(role)
            .unwrap_or_else(|_| panic!("role `{group}.{role}` not found in context"));
        let actual = role_obj
            .get_attr("hex")
            .unwrap_or_else(|_| panic!("role `{group}.{role}` missing `hex` field"));

        assert_eq!(
            actual,
            expected.into(),
            "role `{group}.{role}` has wrong hex value"
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

            [roles.ansi]
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
