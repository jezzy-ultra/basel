use std::borrow::Borrow;
use std::fmt::{Formatter, Result as FmtResult};
use std::hash::{Hash, Hasher};
use std::result::Result as StdResult;
use std::str::FromStr;
use std::sync::Arc;

use hex_color::{Case, Display as HexDisplay, HexColor, ParseHexColorError};
use indexmap::{IndexMap, IndexSet};
use minijinja::Value as JinjaValue;
use minijinja::value::{Enumerator, Object as JinjaObject};
use serde::de::Error as SerdeDeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use toml::Value as TomlValue;

use crate::{ColorFormat, Result, TextFormat, name_type};

pub(crate) const SWATCH_MARKER: &str = "SWATCH";
pub(crate) const SWATCH_VARIABLE: &str = "swatch";

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("hex parsing error: {0}")]
    ParsingHex(#[from] ParseHexColorError),
    #[error(
        "{} fall back to the same ascii name `{ascii_name}`",
        format_names(display_names)
    )]
    CollidingAsciiNames {
        ascii_name: String,
        display_names: Vec<String>,
    },
    #[error("swatches {} differ only in case", format_names(names))]
    CollidingNameCases { names: Vec<String> },
    #[error("invalid toml structure for swatch `{name}`: {reason}")]
    InvalidTomlStructure { name: String, reason: String },
}

fn format_names(names: &[String]) -> String {
    names
        .iter()
        .map(|n| format!("`{n}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SwatchColor(HexDisplay);

impl SwatchColor {
    pub fn parse(s: &str) -> Result<Self> {
        s.parse()
    }

    #[must_use]
    pub const fn hex(self) -> HexColor {
        self.0.color()
    }
}

impl From<HexColor> for SwatchColor {
    fn from(color: HexColor) -> Self {
        Self(HexDisplay::new(color).with_case(Case::Lower))
    }
}

impl From<HexDisplay> for SwatchColor {
    fn from(display: HexDisplay) -> Self {
        Self(display.with_case(Case::Lower))
    }
}

impl FromStr for SwatchColor {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self> {
        Self::try_from(s)
    }
}

impl TryFrom<&str> for SwatchColor {
    type Error = crate::Error;

    fn try_from(s: &str) -> Result<Self> {
        let color = HexColor::parse(s)
            .map_err(Error::from)
            .map_err(crate::Error::from)?;

        Ok(Self(HexDisplay::new(color).with_case(Case::Lower)))
    }
}

impl Serialize for SwatchColor {
    fn serialize<S>(&self, serializer: S) -> StdResult<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for SwatchColor {
    fn deserialize<D>(deserializer: D) -> StdResult<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::parse(s.as_str()).map_err(SerdeDeError::custom)
    }
}

name_type!(SwatchName, SwatchAsciiName, "swatch");

#[non_exhaustive]
#[derive(Debug, Serialize, Deserialize)]
pub struct Swatch {
    pub name: SwatchName,
    pub color: SwatchColor,
    pub ascii: SwatchAsciiName,
}

impl Swatch {
    pub fn parse(display_key: &str, val: &TomlValue) -> Result<Self> {
        let display_name = SwatchName::parse(display_key)?;

        if let Some(hex_str) = val.as_str() {
            let hex = SwatchColor::try_from(hex_str)?;
            Ok(Self {
                name: display_name.clone(),
                color: hex,
                ascii: display_name.to_ascii()?,
            })
        } else if let Some(table) = val.as_table() {
            let hex_str = table.get("hex").and_then(|v| v.as_str()).ok_or_else(|| {
                Error::InvalidTomlStructure {
                    name: display_key.to_owned(),
                    reason: "swatch table missing `hex` field (add or make swatch value a string)"
                        .to_owned(),
                }
            })?;

            let hex = SwatchColor::try_from(hex_str)?;

            let ascii = if let Some(ascii_str) = table.get("ascii").and_then(|v| v.as_str()) {
                SwatchAsciiName::parse(ascii_str)?
            } else {
                display_name.to_ascii()?
            };

            Ok({
                let name = display_name;
                Self {
                    name,
                    color: hex,
                    ascii,
                }
            })
        } else {
            Err(crate::Error::Swatch(Error::InvalidTomlStructure {
                name: display_key.to_owned(),
                reason: "must be hex string or `{ hex, ascii }` table".to_owned(),
            }))
        }
    }

    #[must_use]
    pub const fn hex(&self) -> &HexDisplay {
        &self.color.0
    }

    #[must_use]
    pub const fn rgb(&self) -> (u8, u8, u8) {
        self.color.0.color().split_rgb()
    }
}

impl Hash for Swatch {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl PartialEq for Swatch {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for Swatch {}

impl Borrow<str> for Swatch {
    fn borrow(&self) -> &str {
        self.name.as_str()
    }
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
            return Err(Error::CollidingAsciiNames {
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
            return Err(Error::CollidingNameCases {
                names: original_names,
            }
            .into());
        }
    }

    Ok(())
}

#[derive(Debug, Clone)]
pub(crate) struct RenderConfig {
    pub color_format: ColorFormat,
    pub text_format: TextFormat,
}

impl RenderConfig {
    pub(crate) fn new(color_format: ColorFormat, text_format: TextFormat) -> Arc<Self> {
        Arc::new(Self {
            color_format,
            text_format,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub(crate) enum ColorObject {
    Swatch {
        hex: String,
        name: String,
        ascii: String,
        rgb: (u8, u8, u8),
        roles: Vec<String>,
        #[serde(skip)]
        config: Arc<RenderConfig>,
    },
    Role {
        hex: String,
        swatch: String,
        swatch_ascii: String,
        rgb: (u8, u8, u8),
        #[serde(skip)]
        config: Arc<RenderConfig>,
    },
}

impl ColorObject {
    pub(crate) const fn swatch(
        hex: String,
        name: String,
        ascii: String,
        rgb: (u8, u8, u8),
        roles: Vec<String>,
        config: Arc<RenderConfig>,
    ) -> Self {
        Self::Swatch {
            hex,
            name,
            ascii,
            rgb,
            roles,
            config,
        }
    }

    pub(crate) const fn role(
        hex: String,
        swatch: String,
        swatch_ascii: String,
        rgb: (u8, u8, u8),
        config: Arc<RenderConfig>,
    ) -> Self {
        Self::Role {
            hex,
            swatch,
            swatch_ascii,
            rgb,
            config,
        }
    }
}

impl JinjaObject for ColorObject {
    fn render(self: &Arc<Self>, f: &mut Formatter<'_>) -> FmtResult {
        match self.as_ref() {
            Self::Swatch {
                hex,
                name,
                ascii,
                config,
                ..
            } => {
                let text = match config.color_format {
                    ColorFormat::Hex => hex,
                    ColorFormat::Name => match config.text_format {
                        TextFormat::Unicode => name,
                        TextFormat::Ascii => ascii,
                    },
                };

                write!(f, "{text}")
            }
            Self::Role {
                hex,
                swatch,
                swatch_ascii,
                config,
                ..
            } => {
                let text = match config.color_format {
                    ColorFormat::Hex => hex,
                    ColorFormat::Name => match config.text_format {
                        TextFormat::Unicode => swatch,
                        TextFormat::Ascii => swatch_ascii,
                    },
                };

                write!(f, "{text}")
            }
        }
    }

    fn get_value(self: &Arc<Self>, key: &JinjaValue) -> Option<JinjaValue> {
        let key_str = key.as_str()?;
        match self.as_ref() {
            Self::Swatch {
                hex,
                name,
                ascii,
                rgb,
                roles,
                ..
            } => {
                let (r, g, b) = *rgb;

                match key.as_str()? {
                    "hex" => Some(JinjaValue::from(hex)),
                    "name" => Some(JinjaValue::from(name)),
                    "ascii" => Some(JinjaValue::from(ascii)),
                    "roles" => Some(JinjaValue::from_serialize(roles)),
                    "r" => Some(JinjaValue::from(r)),
                    "g" => Some(JinjaValue::from(g)),
                    "b" => Some(JinjaValue::from(b)),
                    "rf" => Some(JinjaValue::from(f64::from(r) / 255.0)),
                    "gf" => Some(JinjaValue::from(f64::from(g) / 255.0)),
                    "bf" => Some(JinjaValue::from(f64::from(b) / 255.0)),
                    _ => None,
                }
            }
            Self::Role {
                hex,
                swatch,
                swatch_ascii,
                rgb,
                ..
            } => {
                let (r, g, b) = *rgb;

                match key_str {
                    "hex" => Some(JinjaValue::from(hex)),
                    "swatch" | "name" => Some(JinjaValue::from(swatch)),
                    "swatch_ascii" | "ascii" => Some(JinjaValue::from(swatch_ascii)),
                    "r" => Some(JinjaValue::from(r)),
                    "g" => Some(JinjaValue::from(g)),
                    "b" => Some(JinjaValue::from(b)),
                    "rf" => Some(JinjaValue::from(f64::from(r) / 255.0)),
                    "gf" => Some(JinjaValue::from(f64::from(g) / 255.0)),
                    "bf" => Some(JinjaValue::from(f64::from(b) / 255.0)),
                    _ => None,
                }
            }
        }
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        match self.as_ref() {
            Self::Swatch { .. } => Enumerator::Str(&[
                "hex", "name", "ascii", "roles", "r", "g", "b", "rf", "gf", "bf",
            ]),
            Self::Role { .. } => Enumerator::Str(&[
                "hex",
                "swatch",
                "swatch_ascii",
                "name",
                "ascii",
                "r",
                "g",
                "b",
                "rf",
                "gf",
                "bf",
            ]),
        }
    }
}
