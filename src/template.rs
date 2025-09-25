use std::{fs, result};

use indexmap::IndexMap;
use minijinja::{Environment, Template, UndefinedBehavior};
use walkdir::WalkDir;

use crate::Result;

pub struct Templates {
    env: Environment<'static>,
    directives: IndexMap<String, IndexMap<String, String>>,
}

impl Templates {
    pub fn new(template_dir: &str) -> Result<Self> {
        let mut env = Environment::new();
        env.set_undefined_behavior(UndefinedBehavior::SemiStrict);
        env.set_trim_blocks(true);
        env.set_lstrip_blocks(true);

        let directives = Self::load_templates_and_directives(&mut env, template_dir)?;

        Ok(Self { env, directives })
    }

    fn parse_directives(content: &str) -> IndexMap<String, String> {
        let mut directives = IndexMap::new();
        for line in content.lines() {
            let line = line.trim();
            if let Some(part) = line.strip_prefix("#basel:")
                && let Some((key, val)) = part.trim().split_once('=')
            {
                directives.insert(key.trim().to_owned(), val.trim().to_owned());
            }
        }

        directives
    }

    pub fn templates_with_directives(
        &self,
    ) -> IndexMap<&str, (Template<'_, '_>, IndexMap<String, String>)> {
        let mut templates = IndexMap::new();
        for (name, t) in self.env.templates() {
            let directives = self
                .directives
                .get(name)
                .cloned()
                .unwrap_or_else(IndexMap::new);
            templates.insert(name, (t, directives.clone()));
        }

        templates
    }

    fn is_directive(line: &str) -> bool {
        if line.starts_with("#basel:") {
            return true;
        }

        if line.starts_with("#:") {
            return true;
        }

        false
    }

    fn filter_directives(content: &str) -> String {
        content
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                !Self::is_directive(trimmed)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn trim_top_bottom(content: &str) -> String {
        let lines: Vec<&str> = content.lines().collect();

        if lines.is_empty() {
            return String::new();
        }

        let start = lines
            .iter()
            .position(|line| !line.trim().is_empty())
            .unwrap_or(0);
        let end = lines
            .iter()
            .rposition(|line| !line.trim().is_empty())
            .unwrap_or_else(|| lines.len().saturating_sub(1))
            + 1;

        if start >= end {
            return String::new();
        }

        lines[start..end].join("\n")
    }

    fn load_templates_and_directives(
        env: &mut Environment<'static>,
        dir: &str,
    ) -> Result<IndexMap<String, IndexMap<String, String>>> {
        let mut all_directives = IndexMap::new();

        for entry in WalkDir::new(dir)
            .into_iter()
            .filter_entry(|e| !crate::is_hidden(e))
            .filter_map(result::Result::ok)
        {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("jinja") {
                let name = path
                    .strip_prefix(dir)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                let raw_src = fs::read_to_string(path)?;

                let directives = Self::parse_directives(&raw_src);
                all_directives.insert(name.clone(), directives);

                let filtered = Self::trim_top_bottom(&Self::filter_directives(&raw_src));
                env.add_template_owned(name, filtered)?;
            }
        }

        Ok(all_directives)
    }
}
