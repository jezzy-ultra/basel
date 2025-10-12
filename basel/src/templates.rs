use std::fs;
use std::io::Error as IoError;
use std::result::Result as StdResult;

use indexmap::IndexMap;
use minijinja::{
    Environment, Error as JinjaError, ErrorKind as JinjaErrorKind, State as JinjaState, Template,
    UndefinedBehavior, Value as JinjaValue,
};
use walkdir::WalkDir;

use crate::directives::{Directives, Error as DirectiveError};
use crate::{Config, Result, has_extension};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("directive error: {0}")]
    Directive(#[from] DirectiveError),
    #[error("failed to read template `{path}`: {src}")]
    ReadingFile { path: String, src: IoError },
    #[error("template compilation failed for `{path}`: {src}")]
    Compiling { path: String, src: JinjaError },
    #[error("{0}")]
    InternalBug(String),
}

#[derive(Debug)]
pub struct Loader {
    env: Environment<'static>,
    directives: IndexMap<String, Directives>,
}

impl Loader {
    fn create_set_test(
        state: &JinjaState<'_, '_>,
        value: &JinjaValue,
    ) -> StdResult<bool, JinjaError> {
        let slot_name = value.as_str().ok_or_else(|| {
            JinjaError::new(
                JinjaErrorKind::InvalidOperation,
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
                JinjaErrorKind::UndefinedError,
                "`_set` missing from context",
            )),
        }
    }

    fn load_templates_and_directives(
        env: &mut Environment<'static>,
        dir: &str,
        strip_patterns: &[Vec<String>],
    ) -> Result<IndexMap<String, Directives>> {
        let mut directives_map = IndexMap::new();

        for entry in WalkDir::new(dir).into_iter().filter_map(StdResult::ok) {
            let path = entry.path();
            if has_extension(path, "jinja") {
                let name = path
                    .strip_prefix(dir)
                    .map_err(|_src| {
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

                let (directives, filtered) = Directives::from_template(
                    &raw_src,
                    strip_patterns,
                    &name,
                    path.to_string_lossy().as_str(),
                )
                .map_err(|_err| {
                    Error::Directive(DirectiveError::ParsingDirective {
                        directive: name.clone(),
                        path: path.to_string_lossy().to_string(),
                        reason: "failed to read directives".to_owned(),
                    })
                })?;

                directives_map.insert(name.clone(), directives);

                env.add_template_owned(name.clone(), filtered)
                    .map_err(|src| Error::Compiling { path: name, src })?;
            }
        }

        Ok(directives_map)
    }

    pub fn new(config: &Config) -> Result<Self> {
        let mut env = Environment::new();

        env.set_undefined_behavior(UndefinedBehavior::SemiStrict);
        env.set_trim_blocks(true);
        env.set_lstrip_blocks(true);

        env.add_test("set", Self::create_set_test);

        env.add_filter("code", |s: String| -> String { format!("`{}`", s) });

        let directives = Self::load_templates_and_directives(
            &mut env,
            &config.dirs.templates,
            &config.strip_directives,
        )?;

        Ok(Self { env, directives })
    }

    pub(crate) fn templates_with_directives(
        &self,
    ) -> Result<IndexMap<&str, (Template<'_, '_>, &Directives)>> {
        let mut templates: IndexMap<&str, (Template<'_, '_>, &Directives)> = IndexMap::new();
        for (name, t) in self.env.templates() {
            let directives = self.directives.get(name).ok_or_else(|| {
                Error::InternalBug(format!(
                    "template `{name}` in jinja env but missing from directives map"
                ))
            })?;
            templates.insert(name, (t, directives));
        }

        Ok(templates)
    }
}
