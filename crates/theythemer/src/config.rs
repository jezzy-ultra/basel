use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::result::Result as StdResult;
use std::{env, fs, io};

use indexmap::IndexMap;
use log::debug;
use serde::Deserialize;

const FILENAME: &str = "theythemer.toml";

type Result<T> = StdResult<T, Error>;

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("failed to find `{FILENAME}` in `{cwd}` or any parent directory")]
    NoProjectRoot { cwd: String },

    #[error("failed to read `{FILENAME}`: {src}")]
    Reading { src: io::Error },

    #[error("failed to parse `{FILENAME}`: {src}")]
    Parsing { src: Box<toml::de::Error> },

    #[error("failed to expand path `{path}`: {src}")]
    ExpandingPath {
        path: String,
        src: shellexpand::LookupError<env::VarError>,
    },

    #[error("failed to move from `{cwd}` to project root `{root}`: {src}")]
    ChangingDir {
        cwd: String,
        root: String,
        src: io::Error,
    },
}

#[non_exhaustive]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct Config {
    pub strip_directives: Vec<Vec<String>>,
    pub dirs: Dirs,

    #[serde(rename(serialize = "host"))]
    pub hosts: Vec<Host>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // TODO: figure out a design where defaults can be extended by the user instead of
            // completely overridden
            strip_directives: vec![vec!["#:tombi".to_owned()]],

            dirs: Dirs::default(),
            hosts: default_hosts(),
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Host {
    pub domain: String,
    pub blob_path: Option<String>,
    pub raw_path: Option<String>,
    pub branch: Option<String>,
}

impl Host {
    #[must_use]
    pub fn merge_with(&self, default: &Self) -> Self {
        Self {
            domain: self.domain.clone(),
            blob_path: self.blob_path.clone().or_else(|| default.blob_path.clone()),
            raw_path: self.raw_path.clone().or_else(|| default.raw_path.clone()),
            branch: self.branch.clone().or_else(|| default.branch.clone()),
        }
    }
}

pub(crate) fn load() -> Result<Config> {
    let cwd = env::current_dir().map_err(|src| Error::Reading { src })?;

    let project_root = find_project_root(&cwd)?;

    debug!("using project root `{}`", project_root.display());

    let config_path = project_root.join(FILENAME);
    let content = fs::read_to_string(&config_path).map_err(|src| Error::Reading { src })?;

    env::set_current_dir(&project_root).map_err(|src| Error::ChangingDir {
        cwd: cwd.display().to_string(),
        root: project_root.display().to_string(),
        src,
    })?;

    let mut config = parse(content.as_str())?;

    let project_root: &Path = &project_root;

    config.dirs.schemes = expand_and_resolve(&config.dirs.schemes, project_root)?;
    config.dirs.templates = expand_and_resolve(&config.dirs.templates, project_root)?;
    config.dirs.render = expand_and_resolve(&config.dirs.render, project_root)?;

    config.hosts = merge_hosts_with_defaults(&config.hosts);

    Ok(config)
}

fn default_hosts() -> Vec<Host> {
    vec![
        Host {
            domain: "github.com".to_owned(),
            blob_path: Some("{domain}/{owner}/{repo}/blob/{rev}/{file}".to_owned()),
            raw_path: Some("raw.githubusercontent.com/{owner}/{repo}/{rev}/{file}".to_owned()),
            branch: None,
        },
        Host {
            domain: "gitlab.com".to_owned(),
            blob_path: Some("{domain}/{owner}/{repo}/-/blob/{rev}/{file}".to_owned()),
            raw_path: Some("{domain}/{owner}/{repo}/-/raw/{rev}/{file}".to_owned()),
            branch: None,
        },
        Host {
            domain: "codeberg.org".to_owned(),
            blob_path: Some("{domain}/{owner}/{repo}/src/branch/{rev}/{file}".to_owned()),
            raw_path: Some("{domain}/{owner}/{repo}/raw/branch/{rev}/{file}".to_owned()),
            branch: None,
        },
        Host {
            domain: "bitbucket.org".to_owned(),
            blob_path: Some("{domain}/{owner}/{repo}/src/{rev}/{file}".to_owned()),
            raw_path: Some("{domain}/{owner}/{repo}/raw/{rev}/{file}".to_owned()),
            branch: None,
        },
    ]
}

fn merge_hosts_with_defaults(user_hosts: &[Host]) -> Vec<Host> {
    let mut hosts: IndexMap<String, Host> = default_hosts()
        .into_iter()
        .map(|h| (h.domain.clone(), h))
        .collect();

    for user_host in user_hosts {
        if let Some(default) = hosts.get(&user_host.domain) {
            let merged = user_host.merge_with(default);

            hosts.insert(merged.domain.clone(), merged);
        } else {
            hosts.insert(user_host.domain.clone(), user_host.clone());
        }
    }

    hosts.into_values().collect()
}

fn find_project_root(cwd: &Path) -> Result<PathBuf> {
    cwd.ancestors()
        .find(|dir| dir.join(FILENAME).exists())
        .map(PathBuf::from)
        .ok_or_else(|| Error::NoProjectRoot {
            cwd: cwd.display().to_string(),
        })
}

fn parse(content: &str) -> Result<Config> {
    if content.trim().is_empty() {
        return Ok(Config::default());
    }

    toml::from_str(content).map_err(|src| Error::Parsing { src: Box::new(src) })
}

fn expand_and_resolve(path: &str, project_root: &Path) -> Result<String> {
    shellexpand::full(path)
        .map(Cow::into_owned)
        .map_err(|src| Error::ExpandingPath {
            path: path.to_owned(),
            src,
        })
        .map(|expanded| {
            if Path::new(&expanded).is_absolute() {
                expanded
            } else {
                project_root.join(expanded).display().to_string()
            }
        })
}
