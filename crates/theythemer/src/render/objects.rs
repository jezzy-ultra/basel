use std::fmt::{Formatter, Result as FmtResult};
use std::sync::Arc;

use minijinja::value::Enumerator;
use serde::Serialize;

use crate::output::{ColorStyle, Style, TextStyle};

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub(crate) enum Color {
    Swatch {
        hex: String,
        name: String,
        ascii: String,
        rgb: (u8, u8, u8),
        roles: Vec<String>,
        #[serde(skip)]
        style: Arc<Style>,
    },
    Role {
        hex: String,
        swatch: String,
        swatch_ascii: String,
        rgb: (u8, u8, u8),
        #[serde(skip)]
        style: Arc<Style>,
    },
}

impl Color {
    pub(crate) const fn swatch(
        hex: String,
        name: String,
        ascii: String,
        rgb: (u8, u8, u8),
        roles: Vec<String>,
        style: Arc<Style>,
    ) -> Self {
        Self::Swatch {
            hex,
            name,
            ascii,
            rgb,
            roles,
            style,
        }
    }

    pub(crate) const fn role(
        hex: String,
        swatch: String,
        swatch_ascii: String,
        rgb: (u8, u8, u8),
        style: Arc<Style>,
    ) -> Self {
        Self::Role {
            hex,
            swatch,
            swatch_ascii,
            rgb,
            style,
        }
    }
}

impl minijinja::value::Object for Color {
    fn render(self: &Arc<Self>, f: &mut Formatter<'_>) -> FmtResult {
        match self.as_ref() {
            Self::Swatch {
                hex,
                name,
                ascii,
                style,
                ..
            } => {
                let text = match style.color {
                    ColorStyle::Hex => hex,
                    ColorStyle::Name => match style.text {
                        TextStyle::Unicode => name,
                        TextStyle::Ascii => ascii,
                    },
                };

                write!(f, "{text}")
            }
            Self::Role {
                hex,
                swatch,
                swatch_ascii,
                style,
                ..
            } => {
                let text = match style.color {
                    ColorStyle::Hex => hex,
                    ColorStyle::Name => match style.text {
                        TextStyle::Unicode => swatch,
                        TextStyle::Ascii => swatch_ascii,
                    },
                };

                write!(f, "{text}")
            }
        }
    }

    fn get_value(self: &Arc<Self>, key: &minijinja::Value) -> Option<minijinja::Value> {
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
                    "hex" => Some(minijinja::Value::from(hex)),
                    "name" => Some(minijinja::Value::from(name)),
                    "ascii" => Some(minijinja::Value::from(ascii)),
                    "roles" => Some(minijinja::Value::from_serialize(roles)),
                    "r" => Some(minijinja::Value::from(r)),
                    "g" => Some(minijinja::Value::from(g)),
                    "b" => Some(minijinja::Value::from(b)),
                    "rf" => Some(minijinja::Value::from(f64::from(r) / 255.0)),
                    "gf" => Some(minijinja::Value::from(f64::from(g) / 255.0)),
                    "bf" => Some(minijinja::Value::from(f64::from(b) / 255.0)),
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
                    "hex" => Some(minijinja::Value::from(hex)),
                    "swatch" | "name" => Some(minijinja::Value::from(swatch)),
                    "swatch_ascii" | "ascii" => Some(minijinja::Value::from(swatch_ascii)),
                    "r" => Some(minijinja::Value::from(r)),
                    "g" => Some(minijinja::Value::from(g)),
                    "b" => Some(minijinja::Value::from(b)),
                    "rf" => Some(minijinja::Value::from(f64::from(r) / 255.0)),
                    "gf" => Some(minijinja::Value::from(f64::from(g) / 255.0)),
                    "bf" => Some(minijinja::Value::from(f64::from(b) / 255.0)),
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
