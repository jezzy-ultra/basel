use unicode_normalization::UnicodeNormalization as _;

use crate::Result;

const MAX_NAME_LENGTH: usize = 255;

const WINDOWS_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid {context} name `{name}`: {reason}")]
    Invalid {
        context: String,
        name: String,
        reason: String,
    },
    #[error("invalid {context} ascii name `{name}`: {reason}")]
    InvalidAscii {
        context: String,
        name: String,
        reason: String,
    },
    // TODO: offer suggestions on how to fix
    #[error(
        "invalid generated ascii fallback `{ascii_name}` for {context} `{display_name}`: {reason}"
    )]
    GeneratingAscii {
        context: String,
        display_name: String,
        ascii_name: String,
        reason: String,
    },
}

fn is_reserved(name: &str) -> bool {
    let upper = name.to_uppercase();

    WINDOWS_RESERVED.iter().any(|&reserved| upper == reserved)
}

fn is_safe(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_'
}

pub(crate) fn normalize_and_validate(name: &str, context: &str) -> Result<String> {
    let normalized = name.nfc().collect::<String>();

    if normalized.is_empty() {
        return Err(Error::Invalid {
            context: context.to_owned(),
            name: name.to_owned(),
            reason: "empty".to_owned(),
        }
        .into());
    }

    if !normalized.chars().all(is_safe) {
        return Err(Error::Invalid {
            context: context.to_owned(),
            name: name.to_owned(),
            reason: "contains character that's not a unicode letter, number, `-` or `_`".to_owned(),
        }
        .into());
    }

    if normalized.len() > MAX_NAME_LENGTH {
        return Err(Error::Invalid {
            context: context.to_owned(),
            name: name.to_owned(),
            reason: format!(
                "too long ({} characters; max is {MAX_NAME_LENGTH})",
                normalized.len()
            ),
        }
        .into());
    }

    if is_reserved(&normalized) {
        return Err(Error::Invalid {
            context: context.to_owned(),
            name: name.to_owned(),
            reason: "uses reserved windows name".to_owned(),
        }
        .into());
    }

    Ok(normalized)
}

pub(crate) fn validate_set_ascii(name: &str, context: &str) -> Result<()> {
    if !name.is_ascii() {
        // TODO: which character(s)?
        return Err(Error::InvalidAscii {
            context: context.to_owned(),
            name: name.to_owned(),
            reason: "contains non-ascii character(s)".to_owned(),
        }
        .into());
    }

    Ok(())
}

fn validate_auto_ascii(display_name: &str, ascii_name: &str, context: &str) -> Result<()> {
    if ascii_name.len() > MAX_NAME_LENGTH {
        return Err(Error::GeneratingAscii {
            context: context.to_owned(),
            display_name: display_name.to_owned(),
            ascii_name: ascii_name.to_owned(),
            reason: format!(
                "too long ({} characters; max is {MAX_NAME_LENGTH})",
                ascii_name.len()
            ),
        }
        .into());
    }

    if is_reserved(ascii_name) {
        return Err(Error::GeneratingAscii {
            context: context.to_owned(),
            display_name: display_name.to_owned(),
            ascii_name: ascii_name.to_owned(),
            reason: "uses reserved windows name".to_owned(),
        }
        .into());
    }

    if ascii_name.ends_with('.') {
        return Err(Error::GeneratingAscii {
            context: context.to_owned(),
            display_name: display_name.to_owned(),
            ascii_name: ascii_name.to_owned(),
            reason: "ends with `.`".to_owned(),
        }
        .into());
    }

    Ok(())
}

#[expect(clippy::assigning_clones, reason = "can't use `clone_into`")]
pub(crate) fn to_ascii(display_name: &str, context: &str) -> Result<String> {
    let mut ascii_name = deunicode::deunicode(display_name);

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
        return Err(Error::GeneratingAscii {
            context: context.to_owned(),
            display_name: display_name.to_owned(),
            ascii_name,
            reason: "transliteration produced no valid filename characters".to_owned(),
        }
        .into());
    }

    validate_auto_ascii(display_name, &ascii_name, context)?;

    Ok(ascii_name)
}

#[macro_export]
macro_rules! name_type {
    ($display_type:ident, $ascii_type:ident, $context:expr) => {
        #[derive(
            ::core::fmt::Debug,
            ::core::clone::Clone,
            ::core::cmp::PartialEq,
            ::core::cmp::Eq,
            ::core::hash::Hash,
            ::serde::Serialize,
            ::serde::Deserialize,
        )]
        pub struct $display_type(String);

        impl $display_type {
            pub fn parse(s: &str) -> $crate::Result<Self> {
                <Self as ::core::str::FromStr>::from_str(s)
            }

            pub fn to_ascii(&self) -> $crate::Result<$ascii_type> {
                let ascii_string = $crate::names::to_ascii(&self.0, $context)?;

                Ok($ascii_type(ascii_string))
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl ::core::str::FromStr for $display_type {
            type Err = $crate::Error;

            fn from_str(s: &str) -> $crate::Result<Self> {
                let normalized = $crate::names::normalize_and_validate(s, $context)?;

                Ok(Self(normalized))
            }
        }

        impl ::core::fmt::Display for $display_type {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                ::core::write!(f, "{}", &self.0)
            }
        }

        #[derive(
            ::core::fmt::Debug,
            ::core::clone::Clone,
            ::core::cmp::PartialEq,
            ::core::cmp::Eq,
            ::core::hash::Hash,
            ::serde::Serialize,
            ::serde::Deserialize,
        )]
        pub struct $ascii_type(::std::string::String);

        impl $ascii_type {
            pub fn parse(s: &str) -> $crate::Result<Self> {
                <Self as ::core::str::FromStr>::from_str(s)
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl ::core::str::FromStr for $ascii_type {
            type Err = $crate::Error;

            fn from_str(s: &str) -> $crate::Result<Self> {
                let normalized = $crate::names::normalize_and_validate(s, $context)?;
                $crate::names::validate_set_ascii(&normalized, $context)?;

                Ok(Self(normalized))
            }
        }

        impl ::core::fmt::Display for $ascii_type {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                ::core::write!(f, "{}", &self.0)
            }
        }
    };
}
