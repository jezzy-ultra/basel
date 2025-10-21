use std::path::PathBuf;
use std::result::Result as StdResult;
use std::{fs, io};

use log::debug;
use serde::Deserialize;

const CONFIG_FILE: &str = "basel.toml";

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]

pub(crate) enum Error {
    #[error("failed to read `{CONFIG_FILE}`: {src}")]
    Reading { src: io::Error },
    #[error("failed to parse `{CONFIG_FILE}`: {src}")]
    Parsing { src: Box<toml::de::Error> },
    #[error("failed to expand path `{path}`: {src}")]
    Expanding {
        path: String,
        src: shellexpand::LookupError<std::env::VarError>,
    },
}

type Result<T> = StdResult<T, Error>;

#[non_exhaustive]
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Dirs {
    pub schemes: String,
    pub templates: String,
    pub render: String,
}

impl Default for Dirs {
    fn default() -> Self {
        Self {
            schemes: "schemes".to_owned(),
            templates: "templates".to_owned(),
            render: "render".to_owned(),
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Upstream {
    pub repo_path: Option<PathBuf>,
    pub pattern: Option<String>,
    pub branch: Option<String>,
}

#[non_exhaustive]
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub dirs: Dirs,
    pub upstream: Option<Upstream>,
    pub strip_directives: Vec<Vec<String>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            dirs: Dirs::default(),
            upstream: None,
            strip_directives: vec![vec![
                "#:tombi".to_owned(),
                "lint.disabled".to_owned(),
                "=".to_owned(),
                "true".to_owned(),
            ]],
        }
    }
}

fn read_config() -> Result<Option<String>> {
    match fs::read_to_string(CONFIG_FILE) {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            debug!("no `{CONFIG_FILE}` found, using defaults");

            Ok(None)
        }
        Err(src) => Err(Error::Reading { src }),
    }
}

fn parse_config(content: Option<&str>) -> Result<Config> {
    match content {
        Some(s) if !s.trim().is_empty() => {
            toml::from_str(s).map_err(|src| Error::Parsing { src: Box::new(src) })
        }
        _ => Ok(Config::default()),
    }
}

fn expand_path(path: &str) -> Result<String> {
    Ok(shellexpand::full(path)
        .map_err(|src| Error::Expanding {
            path: path.to_owned(),
            src,
        })?
        .into_owned())
}

fn expand_paths(config: &mut Config) -> Result<()> {
    config.dirs.schemes = expand_path(&config.dirs.schemes)?;

    config.dirs.templates = expand_path(&config.dirs.templates)?;

    config.dirs.render = expand_path(&config.dirs.render)?;

    if let Some(upstream) = &mut config.upstream
        && let Some(repo_path) = &upstream.repo_path
    {
        let expanded = expand_path(&repo_path.to_string_lossy())?;

        upstream.repo_path = Some(PathBuf::from(expanded));
    }

    Ok(())
}

pub(crate) fn load() -> Result<Config> {
    let mut cfg = read_config().and_then(|opt| parse_config(opt.as_deref()))?;

    expand_paths(&mut cfg).map(|()| cfg)
}
