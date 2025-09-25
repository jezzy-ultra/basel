use std::fs;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;

use crate::scheme::Scheme;
use crate::template::Templates;
use crate::{Config, Error, Result};

fn extract_port_name(path: &str) -> Result<&str> {
    path.split('/').next().ok_or_else(|| {
        Error::Template(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("Invalid template path: {path}"),
        ))
    })
}

fn resolve_filename(template_name: &str, scheme_name: &str) -> String {
    let filename = template_name
        .split('/')
        .next_back()
        .unwrap_or(template_name);

    let base = filename.strip_suffix(".jinja").unwrap_or(filename);

    base.replace("SCHEME", scheme_name)
}

fn resolve_output_path(config: &Config, template_name: &str, scheme_name: &str) -> PathBuf {
    let port = extract_port_name(template_name).unwrap();
    let filename = resolve_filename(template_name, scheme_name);

    Path::new(&config.output_dir)
        .join(scheme_name)
        .join(port)
        .join(filename)
}

pub fn all(
    config: &Config,
    templates: &Templates,
    schemes: &IndexMap<String, Scheme>,
) -> Result<()> {
    for (template_name, (template, directives)) in templates.templates_with_directives() {
        for (scheme_name, scheme) in schemes {
            let path = resolve_output_path(config, template_name, scheme_name);
            let context = scheme.create_context(&directives);
            let rendered = template.render(context)?;

            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }

            fs::write(&path, rendered)?;
            println!("Generated: {}", path.display());
        }
    }

    Ok(())
}
