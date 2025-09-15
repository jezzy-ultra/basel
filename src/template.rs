use std::{fs, result};

use indexmap::IndexMap;
use minijinja::{Environment, Template, UndefinedBehavior};
use walkdir::WalkDir;

use crate::Result;

pub struct Templates {
    env: Environment<'static>,
}

impl Templates {
    pub fn new(tmpl_dir: &str) -> Result<Self> {
        let mut env = Environment::new();
        env.set_undefined_behavior(UndefinedBehavior::SemiStrict);
        Self::load_templates(&mut env, tmpl_dir)?;

        Ok(Self { env })
    }

    pub fn templates(&self) -> IndexMap<&str, Template<'_, '_>> {
        self.env.templates().collect()
    }

    pub fn get(&self, tmpl: &str) -> Result<Template<'_, '_>> {
        Ok(self.env.get_template(tmpl)?)
    }

    fn load_templates(env: &mut Environment<'static>, dir: &str) -> Result<()> {
        for entry in WalkDir::new(dir)
            .into_iter()
            .filter_entry(|e| !crate::is_hidden(e))
            .filter_map(result::Result::ok)
        {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("basel") {
                let name = path
                    .strip_prefix(dir)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                let src = fs::read_to_string(path)?;
                env.add_template_owned(name, src)?;
            }
        }

        Ok(())
    }
}
