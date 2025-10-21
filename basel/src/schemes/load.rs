use std::fs;
use std::path::Path;
use std::result::Result as StdResult;

use indexmap::{IndexMap, IndexSet};
use serde::Serialize;
use walkdir::WalkDir;

use super::{
    Error, Meta, ResolvedRole, RoleError, RoleKind, RoleName, RoleValue, Scheme, SchemeAsciiName,
    SchemeName, Swatch, SwatchError, roles, validate_meta_field,
};
use crate::Result;
use crate::extensions::PathExt as _;

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
        val: &toml::Value,
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
                RoleKind::Base => Err(RoleError::MissingRequired(role.to_string()).into()),
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

    fn parse_palette(val: &toml::Value, path: &str) -> Result<IndexSet<Swatch>> {
        let table = val.as_table().ok_or_else(|| Error::Deserializing {
            section: "palette".to_owned(),
            path: path.to_owned(),
            src: Box::new(<toml::de::Error as serde::de::Error>::custom(
                "palette must be a table",
            )),
        })?;

        let mut palette = IndexSet::new();

        for (display_key, v) in table {
            let swatch = Swatch::parse(display_key, v)?;
            palette.insert(swatch);
        }

        Self::check_ascii_collisions(&palette)?;
        Self::check_case_collisions(&palette)?;

        Ok(palette)
    }

    pub(crate) fn check_ascii_collisions(palette: &IndexSet<Swatch>) -> Result<()> {
        let mut ascii_to_display: IndexMap<String, Vec<String>> = IndexMap::new();

        for swatch in palette {
            ascii_to_display
                .entry(swatch.ascii.to_string())
                .or_default()
                .push(swatch.name.to_string());
        }

        for (ascii_name, display_names) in ascii_to_display {
            if display_names.len() > 1 {
                return Err(SwatchError::CollidingAsciiNames {
                    ascii_name,
                    display_names,
                }
                .into());
            }
        }

        Ok(())
    }

    pub(crate) fn check_case_collisions(palette: &IndexSet<Swatch>) -> Result<()> {
        let mut lowercase_to_original: IndexMap<String, Vec<String>> = IndexMap::new();

        for swatch in palette {
            let lowercase = swatch.name.as_str().to_lowercase();
            lowercase_to_original
                .entry(lowercase)
                .or_default()
                .push(swatch.name.to_string());
        }

        for (_lowercase, original_names) in lowercase_to_original {
            if original_names.len() > 1 {
                return Err(SwatchError::CollidingNameCases {
                    names: original_names,
                }
                .into());
            }
        }

        Ok(())
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

pub(crate) fn single(name: &str, path: &Path) -> Result<Scheme> {
    let path_str = path.to_string_lossy().to_string();

    let content = fs::read_to_string(&path_str).map_err(|src| Error::ReadingFile {
        path: path_str.clone(),
        src,
    })?;

    let root: toml::Table = toml::from_str(&content).map_err(|src| Error::ParsingRaw {
        path: path_str.clone(),
        src: Box::new(src),
    })?;

    let scheme: Option<SchemeName> = root
        .get("scheme")
        .map(|v| {
            let s = v.as_str().ok_or_else(|| Error::Deserializing {
                section: "scheme".to_owned(),
                path: path_str.clone(),
                src: Box::new(<toml::de::Error as serde::de::Error>::custom(
                    "`scheme` must be a string",
                )),
            })?;
            SchemeName::parse(s).map_err(|e| Error::Deserializing {
                section: "scheme".to_owned(),
                path: path_str.clone(),
                src: Box::new(<toml::de::Error as serde::de::Error>::custom(format!(
                    "{e}"
                ))),
            })
        })
        .transpose()?;

    let scheme_ascii: Option<SchemeAsciiName> = root
        .get("scheme_ascii")
        .map(|v| {
            let s = v.as_str().ok_or_else(|| Error::Deserializing {
                section: "scheme_ascii".to_owned(),
                path: path_str.clone(),
                src: Box::new(<toml::de::Error as serde::de::Error>::custom(
                    "`scheme_ascii` must be a string",
                )),
            })?;
            SchemeAsciiName::parse(s).map_err(|e| Error::Deserializing {
                section: "scheme_ascii".to_owned(),
                path: path_str.clone(),
                src: Box::new(<toml::de::Error as serde::de::Error>::custom(format!(
                    "{e}"
                ))),
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
        src: Box::new(<toml::de::Error as serde::de::Error>::missing_field(
            "palette",
        )),
    })?;

    let palette = RawScheme::parse_palette(palette_val, &path_str)?;

    let roles_val = root.get("roles").ok_or_else(|| Error::Deserializing {
        section: "roles".to_owned(),
        path: path_str.clone(),
        src: Box::new(<toml::de::Error as serde::de::Error>::missing_field(
            "roles",
        )),
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

pub(crate) fn all(dir: &str) -> Result<IndexMap<String, Scheme>> {
    let mut schemes = IndexMap::new();

    for entry in WalkDir::new(dir).into_iter().filter_map(StdResult::ok) {
        let path = entry.path();
        if path.is_toml() {
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
            let scheme = single(name, path)?;
            schemes.insert(name.to_owned(), scheme);
        }
    }

    Ok(schemes)
}
