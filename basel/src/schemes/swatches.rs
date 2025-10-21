use std::borrow::Borrow;
use std::hash::{Hash, Hasher};
use std::result::Result as StdResult;
use std::str::FromStr;

use hex_color::{Case, Display as HexDisplay, HexColor, ParseHexColorError};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::names::Validated;
use crate::Result;
use crate::output::{Ascii, Unicode};

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
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
pub(crate) struct SwatchColor(HexDisplay);

impl SwatchColor {
    pub(crate) fn parse(s: &str) -> Result<Self> {
        s.parse()
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
        Self::parse(s.as_str()).map_err(serde::de::Error::custom)
    }
}

pub(crate) type SwatchName = Validated<"swatch", Unicode>;
pub(crate) type SwatchAsciiName = Validated<"swatch", Ascii>;

#[non_exhaustive]
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Swatch {
    pub name: SwatchName,
    pub color: SwatchColor,
    pub ascii: SwatchAsciiName,
}

impl Swatch {
    pub(crate) fn parse(display_key: &str, val: &toml::Value) -> Result<Self> {
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
    pub(crate) const fn hex(&self) -> &HexDisplay {
        &self.color.0
    }

    #[must_use]
    pub(crate) const fn rgb(&self) -> (u8, u8, u8) {
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
