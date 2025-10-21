use std::io;

use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};

use self::names::Validated;
use crate::Result;
use crate::output::{Ascii, Unicode};

pub(crate) mod load;
pub(crate) mod names;
pub(crate) mod roles;
pub(crate) mod swatches;

pub(crate) use self::names::Error as NameError;
pub(crate) use self::roles::{Error as RoleError, RoleName};
use self::roles::{RoleKind, RoleValue};
pub(crate) use self::swatches::{Error as SwatchError, Swatch};

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("role `{role}` references non-existent swatch `{swatch}`")]
    UndefinedSwatch { role: String, swatch: String },
    #[error("invalid toml syntax in `{path}`: {src}")]
    ParsingRaw {
        path: String,
        src: Box<toml::de::Error>,
    },
    #[error("invalid meta field `{field}`: {reason}")]
    InvalidMeta { field: String, reason: String },
    #[error("invalid roles structure in `{path}`: {reason}")]
    InvalidRolesStructure { path: String, reason: String },
    #[error("failed to deserialize `{section}` section in `{path}`: {src}")]
    Deserializing {
        section: String,
        path: String,
        src: Box<toml::de::Error>,
    },
    #[error("failed to read scheme `{path}`: {src}")]
    ReadingFile { path: String, src: io::Error },
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
pub(crate) struct ResolvedRole {
    pub hex: String,
    pub swatch: String,
    pub ascii: String,
}

pub(crate) type SchemeName = Validated<"scheme", Unicode>;
pub(crate) type SchemeAsciiName = Validated<"scheme", Ascii>;

#[non_exhaustive]
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub(crate) struct Meta {
    pub author: Option<String>,
    pub author_ascii: Option<String>,
    pub license: Option<String>,
    pub license_ascii: Option<String>,
    pub blurb: Option<String>,
    pub blurb_ascii: Option<String>,
}

#[non_exhaustive]
#[derive(Debug, Serialize)]
pub(crate) struct Scheme {
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::io::Write as _;

    use indoc::indoc;
    use minijinja::Value as JinjaValue;
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

        load::single(name, temp.path())
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
