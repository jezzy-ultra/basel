use std::fs;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use log::{info, warn};

use crate::scheme::Scheme;
use crate::template::Templates;
use crate::{Config, Result};

#[derive(Default)]
struct RenderConfig {
    render_swatch_names: bool,
}

impl RenderConfig {
    fn parse_bool(directive: &str, val: &str, template_name: &str) -> bool {
        match val.to_lowercase().as_str() {
            "true" => true,
            "false" => false,
            _ => {
                warn!(
                    "Invalid value `{val}` for directive `{directive}` in {template_name}: \
                     expected `true` or `false`, defaulting to false"
                );

                false
            }
        }
    }

    fn parse(directives: &IndexMap<String, String>, template_name: &str) -> Self {
        let mut config = Self::default();

        for (directive, val) in directives {
            match directive.as_str() {
                "render_swatch_names" => {
                    config.render_swatch_names = Self::parse_bool(directive, val, template_name);
                }
                _ => {
                    warn!(
                        "Unknown directive `{directive}` with value `{val}` in `{template_name}`, \
                         ignoring"
                    );
                }
            }
        }

        config
    }
}

fn uses_swatch_iteration(template_name: &str) -> bool {
    template_name.contains("SWATCH")
}

fn resolve_path(
    config: &Config,
    template_name: &str,
    scheme_name: &str,
    swatch_name: Option<&str>,
) -> PathBuf {
    let relative_path = template_name
        .strip_suffix(".jinja")
        .unwrap_or(template_name);

    let filename = Path::new(relative_path)
        .file_name()
        .unwrap()
        .to_string_lossy();

    let parent_dirs = Path::new(relative_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));

    let output = swatch_name.map_or_else(
        || filename.replace("SCHEME", scheme_name),
        |swatch| {
            filename
                .replace("SCHEME", scheme_name)
                .replace("SWATCH", swatch)
        },
    );

    Path::new(&config.output_dir)
        .join(scheme_name)
        .join(parent_dirs)
        .join(output)
}

fn write_file(
    template: &minijinja::Template<'_, '_>,
    context: serde_json::Map<String, serde_json::Value>,
    output_path: &PathBuf,
) -> Result<()> {
    let rendered = template.render(context)?;

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(output_path, rendered)?;
    info!("Generated: {}", output_path.display());

    Ok(())
}

pub fn all(
    config: &Config,
    templates: &Templates,
    schemes: &IndexMap<String, Scheme>,
) -> Result<()> {
    for (template_name, (template, directives)) in templates.templates_with_directives() {
        let render_config = RenderConfig::parse(&directives, template_name);

        if uses_swatch_iteration(template_name) {
            for (scheme_name, scheme) in schemes {
                for swatch_name in scheme.palette.keys() {
                    let path = resolve_path(config, template_name, scheme_name, Some(swatch_name));
                    let context =
                        scheme.create_context(render_config.render_swatch_names, Some(swatch_name));

                    write_file(&template, context, &path)?;
                }
            }
        } else {
            for (scheme_name, scheme) in schemes {
                let path = resolve_path(config, template_name, scheme_name, None);
                let context = scheme.create_context(render_config.render_swatch_names, None);

                write_file(&template, context, &path)?;
            }
        }
    }

    Ok(())
}
