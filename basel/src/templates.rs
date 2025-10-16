use std::fs;
use std::result::Result as StdResult;

use anyhow::{Context as _, Result as AnyhowResult};
use indexmap::IndexMap;
use minijinja::{
    Environment, Error as JinjaError, ErrorKind as JinjaErrorKind, State as JinjaState, Template,
    UndefinedBehavior, Value as JinjaValue,
};
use walkdir::WalkDir;

use crate::config::Config;
use crate::directives::Directives;
use crate::{Error, Result, has_extension};

pub(crate) const SET_TEST_OBJECT: &str = "_set";
pub(crate) const JINJA_TEMPLATE_SUFFIX: &str = ".jinja";
pub(crate) const SKIP_RENDERING_PREFIX: char = '_';

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
        let role_name = value.as_str().ok_or_else(|| {
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
                            && name == role_name
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
    ) -> AnyhowResult<IndexMap<String, Directives>> {
        let mut directives_map = IndexMap::new();

        for entry in WalkDir::new(dir).into_iter().filter_map(StdResult::ok) {
            let path = entry.path();
            if has_extension(path, "jinja") {
                let name = path
                    .strip_prefix(dir)
                    .with_context(|| {
                        format!(
                            "stripping directory prefix from template path `{}`",
                            path.display()
                        )
                    })?
                    .to_string_lossy()
                    .replace('\\', "/");

                let raw_src = fs::read_to_string(path)
                    .with_context(|| format!("reading template `{}`", path.display()))?;

                let (directives, filtered) = Directives::from_template(
                    &raw_src,
                    strip_patterns,
                    &name,
                    path.to_string_lossy().as_str(),
                )
                .with_context(|| format!("parsing directives in template `{}`", path.display()))?;

                directives_map.insert(name.clone(), directives);

                env.add_template_owned(name.clone(), filtered)
                    .with_context(|| format!("compiling template `{name}`"))?;
            }
        }

        Ok(directives_map)
    }

    fn new_internal(config: &Config) -> AnyhowResult<Self> {
        let mut env = Environment::new();

        env.set_undefined_behavior(UndefinedBehavior::SemiStrict);
        env.set_trim_blocks(true);
        env.set_lstrip_blocks(true);

        env.add_test("set", Self::create_set_test);

        env.add_filter("code", |s: String| -> String { format!("`{s}`") });

        let directives = Self::load_templates_and_directives(
            &mut env,
            &config.dirs.templates,
            &config.strip_directives,
        )?;

        Ok(Self { env, directives })
    }

    pub fn new(config: &Config) -> Result<Self> {
        Self::new_internal(config).map_err(Error::template)
    }

    pub(crate) fn with_directives(
        &self,
    ) -> AnyhowResult<IndexMap<&str, (Template<'_, '_>, &Directives)>> {
        let mut templates: IndexMap<&str, (Template<'_, '_>, &Directives)> = IndexMap::new();
        for (name, t) in self.env.templates() {
            let directives = self
                .directives
                .get(name)
                .ok_or_else(|| Error::InternalBug {
                    module: "templates",
                    reason: format!(
                        "template `{name}` in jinja env but missing from directives map"
                    ),
                })?;
            templates.insert(name, (t, directives));
        }

        Ok(templates)
    }
}
