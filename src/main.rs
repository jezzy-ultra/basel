use std::{fs, io};

use basel::scheme::Scheme;
use basel::{Config, scheme};
use minijinja::{Environment, UndefinedBehavior};
use walkdir::{DirEntry, WalkDir};

fn main() {
    let cfg: Config = Default::default();
    let env = create_template_env(&cfg.template_dir).unwrap();
    let tmpl = env.get_template("kitty/[[scheme]].conf.basel").unwrap();
    let schemes = scheme::load_all(&cfg.scheme_dir);

    for s in schemes {
        println!("{}", tmpl.render(s).unwrap());
    }
}

pub fn create_template_env(root: &str) -> io::Result<Environment<'static>> {
    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::SemiStrict);

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !is_hidden(e))
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("basel") {
            let name = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let src = fs::read_to_string(&path)?;
            env.add_template_owned(name, src);
        }
    }

    Ok(env)
}

fn is_hidden(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with("."))
        .unwrap_or(false)
}
