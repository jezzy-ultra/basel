use std::fs;
use std::io::Error as IoError;
use std::result::Result as StdResult;

use indexmap::IndexMap;
use minijinja::{
    Environment, Error as JinjaError, Template, UndefinedBehavior, Value as JinjaValue,
};
use walkdir::WalkDir;

use crate::Result;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to read template `{path}`: {src}")]
    ReadingFile { path: String, src: IoError },
    #[error("template compilation failed for `{path}`: {src}")]
    Compiling { path: String, src: JinjaError },
    #[error("invalid directive `{directive}` in `{path}`: {reason}")]
    ParsingDirective {
        directive: String,
        path: String,
        reason: String,
    },
    #[error("{0}")]
    InternalBug(String),
}

#[derive(Debug)]
pub struct Loader {
    env: Environment<'static>,
    directives: IndexMap<String, IndexMap<String, String>>,
}

impl Loader {
    fn create_set_test(
        state: &minijinja::State<'_, '_>,
        val: &JinjaValue,
    ) -> StdResult<bool, JinjaError> {
        let slot_name = val.as_str().ok_or_else(|| {
            JinjaError::new(
                minijinja::ErrorKind::InvalidOperation,
                "`set` test requires a string argument",
            )
        })?;

        match state.lookup("_set") {
            Some(set) => {
                if let Ok(items) = set.try_iter() {
                    for item in items {
                        if let Some(name) = item.as_str()
                            && name == slot_name
                        {
                            return Ok(true);
                        }
                    }
                }
                Ok(false)
            }
            None => Err(JinjaError::new(
                minijinja::ErrorKind::UndefinedError,
                "`_set` missing from context",
            )),
        }
    }

    fn parse_directives(content: &str) -> IndexMap<String, String> {
        let mut directives = IndexMap::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(part) = trimmed.strip_prefix("#basel:")
                && let Some((key, val)) = part.trim().split_once('=')
            {
                directives.insert(key.trim().to_owned(), val.trim().to_owned());
            }
        }

        directives
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

        lines.get(start..end).unwrap_or(&[]).join("\n")
    }

    fn is_directive(line: &str, ignore_directives: Vec<Vec<String>>) -> bool {
        if line.starts_with("#basel:") {
            return true;
        }

        for pattern in ignore_directives {
            if pattern.iter().all(|part| line.contains(part)) {
                return true;
            }
        }

        false
    }

    fn filter_directives(content: &str, ignore_directives: &[Vec<String>]) -> String {
        content
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                !Self::is_directive(trimmed, ignore_directives.to_vec())
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn load_templates_and_directives(
        env: &mut Environment<'static>,
        dir: &str,
        ignore_directives: &[Vec<String>],
    ) -> Result<IndexMap<String, IndexMap<String, String>>> {
        let mut all_directives = IndexMap::new();

        for entry in WalkDir::new(dir).into_iter().filter_map(StdResult::ok) {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("jinja") {
                let name = path
                    .strip_prefix(dir)
                    .map_err(|_source| {
                        Error::InternalBug(format!(
                            "attempted to load template with corrupted path `{}`",
                            path.display()
                        ))
                    })?
                    .to_string_lossy()
                    .replace('\\', "/");
                let raw_src = fs::read_to_string(path).map_err(|src| Error::ReadingFile {
                    path: path.to_string_lossy().to_string(),
                    src,
                })?;

                let directives = Self::parse_directives(&raw_src);
                all_directives.insert(name.clone(), directives);

                let filtered =
                    Self::trim_top_bottom(&Self::filter_directives(&raw_src, ignore_directives));
                env.add_template_owned(name.clone(), filtered)
                    .map_err(|src| Error::Compiling { path: name, src })?;
            }
        }

        Ok(all_directives)
    }

    pub fn new(template_dir: &str, ignored_directives: &[Vec<String>]) -> Result<Self> {
        let mut env = Environment::new();

        env.set_undefined_behavior(UndefinedBehavior::SemiStrict);
        env.set_trim_blocks(true);
        env.set_lstrip_blocks(true);

        env.add_test("set", Self::create_set_test);

        let directives =
            Self::load_templates_and_directives(&mut env, template_dir, ignored_directives)?;

        Ok(Self { env, directives })
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
}
