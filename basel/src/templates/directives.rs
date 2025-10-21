use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use anyhow::bail;
use indexmap::IndexMap;
use log::{debug, error};

use self::DirectiveType::{Basel, Other};
use crate::output::{ColorStyle, Style, TextStyle};
use crate::{Error, PathExt as _, Result};

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

fn parse_style(raw: &IndexMap<String, String>, template_name: &str) -> Style {
    let mut style = Style::default();

    for (directive, val) in raw {
        match directive.as_str() {
            "render_swatch_names" => {
                style.color = if parse_bool(directive, val, template_name) {
                    ColorStyle::Name
                } else {
                    ColorStyle::Hex
                };
            }
            "render_as_ascii" => {
                style.text = if parse_bool(directive, val, template_name) {
                    TextStyle::Ascii
                } else {
                    TextStyle::Unicode
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

    style
}

fn parse_bool(directive: &str, val: &str, template_name: &str) -> bool {
    match val {
        "true" => true,
        "false" => false,
        _ => {
            error!(
                "invalid value `{val}` for directive `{directive}` in {template_name}: expected \
                 `true` or `false`, defaulting to false"
            );

            false
        }
    }
}

#[non_exhaustive]
#[derive(Debug)]

pub(crate) struct Directives {
    pub style: Arc<Style>,
    pub output_lines: HashSet<String>,
}

type Type = str;

impl Directives {
    fn matches_pattern(line: &str, patterns: &[Vec<String>]) -> bool {
        patterns.iter().any(|p| {
            p.iter()
                .try_fold(0, |from, part| {
                    line.get(from..)
                        .and_then(|s| s.find(part))
                        .map(|pos| from + pos + part.len())
                })
                .is_some()
        })
    }

    fn classify_line(
        line: &str,
        strip_patterns: &[Vec<String>],
        file_path: &str,
    ) -> anyhow::Result<LineType> {
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

            // TODO: add more help
            bail!("incomplete basel directive in `{file_path}`: `{part}`")
        }

        if trimmed.contains("tombi") {
            eprintln!("[DEBUG] trimmed: {trimmed:?}");
            eprintln!("[DEBUG] strip_patterns: {strip_patterns:?}");
            eprintln!(
                "[DEBUG] match result: {}",
                Self::matches_pattern(trimmed, strip_patterns)
            );
        }

        if Self::matches_pattern(trimmed, strip_patterns) {
            return Ok(LineType::Directive(Other(trimmed.to_owned())));
        }

        Ok(LineType::Content)
    }

    fn trim_file_ends(content: &[&Type]) -> String {
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

    fn from_template_internal(
        content: &str,
        strip_patterns: &[Vec<String>],
        template_name: &str,
        file_path: &str,
    ) -> anyhow::Result<(Self, String)> {
        let mut basel_raw = IndexMap::new();

        let mut output_lines = HashSet::new();

        let mut content_lines = Vec::new();

        for line in content.lines() {
            let classified = Self::classify_line(line, strip_patterns, file_path)?;

            if line.contains("tombi") {
                eprintln!("[DEBUG] line: {line:?}");
                eprintln!("[DEBUG] classified as: {classified:?}");
            }

            match classified {
                LineType::Directive(Basel { key, val }) => {
                    basel_raw.insert(key, val);
                }
                LineType::Directive(Other(text)) => {
                    output_lines.insert(Self::canonicalize(&text));
                }
                LineType::Content => {
                    content_lines.push(line);
                }
            }
        }

        let filtered = Self::trim_file_ends(&content_lines);

        let style = Arc::new(parse_style(&basel_raw, template_name));

        Ok((
            Self {
                style,
                output_lines,
            },
            filtered,
        ))
    }

    pub(crate) fn from_template(
        content: &str,
        strip_patterns: &[Vec<String>],
        template_name: &str,
        file_path: &str,
    ) -> Result<(Self, String)> {
        Self::from_template_internal(content, strip_patterns, template_name, file_path)
            .map_err(Error::template)
    }

    fn canonicalize(directive: &str) -> String {
        directive
            .to_lowercase()
            .split('=')
            .map(str::trim)
            .collect::<Vec<_>>()
            .join(" = ")
    }

    pub(crate) fn make_header(&self, output_path: &Path) -> String {
        eprintln!(
            "[DEBUG] output_lines before make_header: {:?}",
            self.output_lines
        );
        eprintln!("[DEBUG] output_path.is_toml(): {}", output_path.is_toml());

        let mut directives = self.output_lines.clone();

        if output_path.is_toml() {
            let format_disabled_str = "#:tombi format.disabled = true";

            let canonical = Self::canonicalize(format_disabled_str);

            if !directives
                .iter()
                .any(|d| Self::canonicalize(d) == canonical)
            {
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
