use std::collections::HashSet;
use std::path::Path;
use std::result::Result as StdResult;

use indexmap::IndexMap;
use log::{debug, error};

use self::DirectiveType::{Basel, Other};
use crate::{ColorFormat, TextFormat, is_toml};

#[expect(
    unnameable_types,
    reason = "internal error intentionally exposed through public `crate::Error`"
)]
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid directive `{directive}` in `{path}`: {reason}")]
    ParsingDirective {
        directive: String,
        path: String,
        reason: String,
    },
}

pub(crate) type Result<T> = StdResult<T, Error>;

#[derive(Debug, Clone, PartialEq)]
enum LineType {
    Content,
    Directive(DirectiveType),
}

#[derive(Debug, Clone, PartialEq)]
enum DirectiveType {
    Basel { key: String, val: String },
    Other(String),
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Config {
    color_format: ColorFormat,
    text_format: TextFormat,
}

impl Config {
    pub(crate) const fn color_format(&self) -> ColorFormat {
        self.color_format
    }

    pub(crate) const fn text_format(&self) -> TextFormat {
        self.text_format
    }

    fn parse_bool(directive: &str, val: &str, template_name: &str) -> bool {
        match val {
            "true" => true,
            "false" => false,
            _ => {
                error!(
                    "invalid value `{val}` for directive `{directive}` in {template_name}: \
                     expected `true` or `false`, defaulting to false"
                );

                false
            }
        }
    }

    fn from(raw: &IndexMap<String, String>, template_name: &str) -> Self {
        let mut cfg = Self::default();

        for (directive, val) in raw {
            match directive.as_str() {
                "render_swatch_names" => {
                    cfg.color_format = if Self::parse_bool(directive, val, template_name) {
                        ColorFormat::Name
                    } else {
                        ColorFormat::Hex
                    };
                }
                "render_as_ascii" => {
                    cfg.text_format = if Self::parse_bool(directive, val, template_name) {
                        TextFormat::Ascii
                    } else {
                        TextFormat::Unicode
                    };
                }
                _ => {
                    debug!(
                        "ignoring unknown directive `{directive}` with value `{val}` in \
                         `{template_name}`"
                    );
                }
            }
        }

        cfg
    }
}

#[derive(Debug)]
pub(crate) struct Directives {
    config: Config,
    output_lines: HashSet<String>,
}

type Type = str;

impl Directives {
    pub(crate) const fn config(&self) -> &Config {
        &self.config
    }

    fn matches_pattern(line: &str, patterns: &[Vec<String>]) -> bool {
        patterns
            .iter()
            .any(|p| p.iter().all(|part| line.contains(part)))
    }

    fn classify_line(
        line: &str,
        strip_patterns: &[Vec<String>],
        file_path: &str,
    ) -> Result<LineType> {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with("##") || !trimmed.starts_with('#') {
            return Ok(LineType::Content);
        }

        if let Some(part) = trimmed.strip_prefix("#basel:") {
            if let Some((key, val)) = part.trim().split_once('=') {
                return Ok(LineType::Directive(Basel {
                    key: key.trim().to_owned(),
                    val: val.trim().to_owned(),
                }));
            }

            return Err(Error::ParsingDirective {
                directive: part.to_owned(),
                path: file_path.to_owned(),
                reason: "incomplete basel directive".to_owned(),
            });
        }

        if Self::matches_pattern(trimmed, strip_patterns) {
            return Ok(LineType::Directive(Other(trimmed.to_owned())));
        }

        Ok(LineType::Content)
    }

    fn trim_ends(content: &[&Type]) -> String {
        let Some(start) = content.iter().position(|l| !l.trim().is_empty()) else {
            return String::new();
        };

        let end = content
            .iter()
            .rposition(|l| !l.trim().is_empty())
            .map_or(content.len(), |i| i + 1);

        #[expect(
            clippy::indexing_slicing,
            reason = "start and end are always within bounds"
        )]
        content[start..end].join("\n")
    }

    pub(crate) fn from_template(
        content: &str,
        strip_patterns: &[Vec<String>],
        template_name: &str,
        file_path: &str,
    ) -> Result<(Self, String)> {
        let mut basel_raw = IndexMap::new();
        let mut output_lines = HashSet::new();
        let mut content_lines = Vec::new();

        for line in content.lines() {
            let classified = Self::classify_line(line, strip_patterns, file_path)?;
            match classified {
                LineType::Directive(Basel { key, val }) => {
                    basel_raw.insert(key, val);
                }
                LineType::Directive(Other(text)) => {
                    output_lines.insert(Self::normalize(&text));
                }
                LineType::Content => {
                    content_lines.push(line);
                }
            }
        }

        let filtered = Self::trim_ends(&content_lines);
        let config = Config::from(&basel_raw, template_name);

        Ok((
            Self {
                config,
                output_lines,
            },
            filtered,
        ))
    }

    fn normalize(directive: &str) -> String {
        directive
            .split('=')
            .map(str::trim)
            .collect::<Vec<_>>()
            .join("=")
    }

    pub(crate) fn make_header(&self, output_path: &Path) -> String {
        let mut directives = self.output_lines.clone();

        if is_toml(output_path) {
            let format_disabled_str = "#:tombi format.disabled = true";
            let normalized = Self::normalize(format_disabled_str);

            if !directives.iter().any(|d| Self::normalize(d) == normalized) {
                directives.insert(format_disabled_str.to_owned());
            }
        }

        let mut header: Vec<_> = directives.into_iter().collect();
        header.sort();

        if header.is_empty() {
            String::new()
        } else {
            format!("{}\n\n", header.join("\n"))
        }
    }
}
