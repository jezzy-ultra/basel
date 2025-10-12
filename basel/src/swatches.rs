use std::borrow::Borrow;
use std::fmt::{Display, Formatter, Result as FmtResult};
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
use unicode_normalization::UnicodeNormalization as _;

use crate::{ColorFormat, TextFormat};

const MAX_NAME_LENGTH: usize = 255;

const WINDOWS_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("hex parsing error: {0}")]
    ParsingHex(#[from] ParseHexColorError),
    #[error("invalid swatch name `{name}`: {reason}")]
    InvalidName { name: String, reason: String },
    #[error("invalid swatch ascii name `{name}`: {reason}")]
    InvalidAsciiName { name: String, reason: String },
    // TODO: offer suggestions on how to fix
    #[error(
        "invalid generated ascii fallback `{ascii_name}` for swatch `{display_name}`: {reason}"
    )]
    GeneratingAsciiName {
        display_name: String,
        ascii_name: String,
        reason: String,
    },
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

pub(crate) type Result<T> = StdResult<T, Error>;

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
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Self::try_from(s)
    }
}

impl TryFrom<&str> for SwatchColor {
    type Error = Error;

    fn try_from(s: &str) -> Result<Self> {
        let color = HexColor::parse(s)?;
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

fn is_reserved(name: &str) -> bool {
    let base = name.split('.').next().unwrap_or(name);
    let upper = base.to_uppercase();

    WINDOWS_RESERVED.iter().any(|&reserved| reserved == upper)
}

fn is_safe(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_'
}

fn normalize_and_validate(name: &str) -> Result<String> {
    let normalized = name.nfc().collect::<String>();

    if normalized.is_empty() {
        return Err(Error::InvalidName {
            name: name.to_owned(),
            reason: "empty".to_owned(),
        });
    }

    if !normalized.chars().all(is_safe) {
        return Err(Error::InvalidName {
            name: name.to_owned(),
            reason: "contains character that's not a unicode letter, number, `-` or `_`".to_owned(),
        });
    }

    if normalized.len() > MAX_NAME_LENGTH {
        return Err(Error::InvalidName {
            name: name.to_owned(),
            reason: format!(
                "too long ({} characters; max is {MAX_NAME_LENGTH})",
                normalized.len()
            ),
        });
    }

    if is_reserved(&normalized) {
        return Err(Error::InvalidName {
            name: name.to_owned(),
            reason: "uses reserved windows name".to_owned(),
        });
    }

    Ok(normalized)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SwatchDisplayName(String);

impl SwatchDisplayName {
    pub fn parse(s: &str) -> Result<Self> {
        s.parse()
    }

    #[expect(
        clippy::assigning_clones,
        reason = "can't use `clone_into`: `trim_matches` borrows `ascii_name` immutably"
    )]
    pub fn to_ascii(&self) -> Result<SwatchAsciiName> {
        let mut ascii_name = deunicode::deunicode(&self.0);

        let mut last_was_sep = false;
        ascii_name = ascii_name
            .chars()
            .filter_map(|c| match c {
                c if c.is_ascii_alphanumeric() => {
                    last_was_sep = false;
                    Some(c)
                }
                '-' | '_' => {
                    if last_was_sep {
                        None
                    } else {
                        last_was_sep = true;
                        Some(c)
                    }
                }
                ' ' | '/' | ':' | ',' | ';' | '|' | '+' => {
                    if last_was_sep {
                        None
                    } else {
                        last_was_sep = true;
                        Some('-')
                    }
                }
                _ => {
                    if last_was_sep {
                        None
                    } else {
                        last_was_sep = true;
                        Some('_')
                    }
                }
            })
            .collect::<String>();

        ascii_name = ascii_name.trim_matches(|c| c == '-' || c == '_').to_owned();

        if ascii_name.is_empty() {
            return Err(Error::GeneratingAsciiName {
                display_name: self.0.clone(),
                ascii_name,
                reason: "transliteration produced no valid filename characters".to_owned(),
            });
        }

        validate_auto_ascii(&self.0, &ascii_name)?;

        Ok(SwatchAsciiName(ascii_name))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for SwatchDisplayName {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let normalized = normalize_and_validate(s)?;

        Ok(Self(normalized))
    }
}

impl Display for SwatchDisplayName {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", &self.0)
    }
}

fn validate_set_ascii(name: &str) -> Result<()> {
    if !name.is_ascii() {
        // TODO: which character(s)?
        return Err(Error::InvalidAsciiName {
            name: name.to_owned(),
            reason: "contains non-ascii character(s)".to_owned(),
        });
    }

    Ok(())
}

fn validate_auto_ascii(display_name: &str, ascii_name: &str) -> Result<()> {
    if ascii_name.len() > MAX_NAME_LENGTH {
        return Err(Error::GeneratingAsciiName {
            display_name: display_name.to_owned(),
            ascii_name: ascii_name.to_owned(),
            reason: format!(
                "too long ({} characters; max is {MAX_NAME_LENGTH})",
                ascii_name.len()
            ),
        });
    }

    if is_reserved(ascii_name) {
        return Err(Error::GeneratingAsciiName {
            display_name: display_name.to_owned(),
            ascii_name: ascii_name.to_owned(),
            reason: "uses reserved windows name".to_owned(),
        });
    }

    if ascii_name.ends_with('.') {
        return Err(Error::GeneratingAsciiName {
            display_name: display_name.to_owned(),
            ascii_name: ascii_name.to_owned(),
            reason: "ends with `.`".to_owned(),
        });
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SwatchAsciiName(String);

impl SwatchAsciiName {
    pub fn parse(s: &str) -> Result<Self> {
        s.parse()
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for SwatchAsciiName {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let normalized = normalize_and_validate(s)?;
        validate_set_ascii(&normalized)?;

        Ok(Self(normalized))
    }
}

impl Display for SwatchAsciiName {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", &self.0)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Swatch {
    name: SwatchDisplayName,
    color: SwatchColor,
    ascii: SwatchAsciiName,
}

impl Swatch {
    pub fn parse(display_key: &str, val: &TomlValue) -> Result<Self> {
        let display_name = SwatchDisplayName::parse(display_key)?;

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
            Err(Error::InvalidTomlStructure {
                name: display_key.to_owned(),
                reason: "must be hex string or `{ hex, ascii }` table".to_owned(),
            })
        }
    }

    #[must_use]
    pub const fn hex(&self) -> &HexDisplay {
        &self.color.0
    }

    #[must_use]
    pub const fn name(&self) -> &SwatchDisplayName {
        &self.name
    }

    #[must_use]
    pub const fn ascii(&self) -> &SwatchAsciiName {
        &self.ascii
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
            .entry(swatch.ascii().to_string())
            .or_default()
            .push(swatch.name.to_string());
    }

    for (ascii_name, display_names) in ascii_to_display {
        if display_names.len() > 1 {
            return Err(Error::CollidingAsciiNames {
                ascii_name,
                display_names,
            });
        }
    }

    Ok(())
}

pub(crate) fn check_case_collisions(palette: &IndexSet<Swatch>) -> Result<()> {
    let mut lowercase_to_original: IndexMap<String, Vec<String>> = IndexMap::new();

    for swatch in palette {
        let lowercase = swatch.name().as_str().to_lowercase();
        lowercase_to_original
            .entry(lowercase)
            .or_default()
            .push(swatch.name().to_string());
    }

    for (_lowercase, original_names) in lowercase_to_original {
        if original_names.len() > 1 {
            return Err(Error::CollidingNameCases {
                names: original_names,
            });
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

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum ColorObject {
    Swatch {
        hex: String,
        name: String,
        ascii: String,
        rgb: (u8, u8, u8),
        slots: Vec<String>,
        #[serde(skip)]
        config: Arc<RenderConfig>,
    },
    Slot {
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
        slots: Vec<String>,
        config: Arc<RenderConfig>,
    ) -> Self {
        Self::Swatch {
            hex,
            name,
            ascii,
            rgb,
            slots,
            config,
        }
    }

    pub(crate) const fn slot(
        hex: String,
        swatch: String,
        swatch_ascii: String,
        rgb: (u8, u8, u8),
        config: Arc<RenderConfig>,
    ) -> Self {
        Self::Slot {
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
                hex, name, config, ..
            } => {
                let text = match config.color_format {
                    ColorFormat::Hex => hex,
                    ColorFormat::Name => match config.text_format {
                        TextFormat::Unicode => name,
                        TextFormat::Ascii => {
                            if let Self::Swatch { ascii, .. } = self.as_ref() {
                                ascii
                            } else {
                                unreachable!() // ???
                            }
                        }
                    },
                };

                write!(f, "{text}")
            }
            Self::Slot {
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
                slots,
                ..
            } => {
                let (r, g, b) = *rgb;

                match key.as_str()? {
                    "hex" => Some(JinjaValue::from(hex)),
                    "name" => Some(JinjaValue::from(name)),
                    "ascii" => Some(JinjaValue::from(ascii)),
                    "slots" => Some(JinjaValue::from_serialize(slots)),
                    "r" => Some(JinjaValue::from(r)),
                    "g" => Some(JinjaValue::from(g)),
                    "b" => Some(JinjaValue::from(b)),
                    "rf" => Some(JinjaValue::from(f64::from(r) / 255.0)),
                    "gf" => Some(JinjaValue::from(f64::from(g) / 255.0)),
                    "bf" => Some(JinjaValue::from(f64::from(b) / 255.0)),
                    _ => None,
                }
            }
            Self::Slot {
                hex,
                swatch,
                swatch_ascii,
                rgb,
                ..
            } => {
                let (r, g, b) = *rgb;

                match key_str {
                    "hex" => Some(JinjaValue::from(hex)),
                    "swatch" => Some(JinjaValue::from(swatch)),
                    "swatch_ascii" => Some(JinjaValue::from(swatch_ascii)),
                    "name" => Some(JinjaValue::from(swatch)),
                    "ascii" => Some(JinjaValue::from(swatch_ascii)),
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
                "hex", "name", "ascii", "slots", "r", "g", "b", "rf", "gf", "bf",
            ]),
            Self::Slot { .. } => Enumerator::Str(&[
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
