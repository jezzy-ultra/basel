use std::path::{Path, PathBuf};
use std::result::Result as StdResult;

use git_url_parse::{GitUrl, GitUrlParseError};
use git2::Repository;
use indexmap::IndexMap;
use log::{info, warn};

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("no git remotes found")]
    NoRemote,

    #[error("failed to parse git url (invalid utf-8)")]
    InvalidUrl,

    #[error("failed to fetch git remotes: {src}")]
    FetchingRemotes { src: git2::Error },

    #[error("failed to fetch git remote `{remote}`: {src}")]
    FetchingUrl { remote: String, src: git2::Error },

    #[error("failed to parse git remote: {src}")]
    ParsingRemote { src: git2::Error },

    #[error("failed to parse git url `{url}`: {src}")]
    ParsingUrl { url: String, src: GitUrlParseError },
}

pub(crate) type Result<T> = StdResult<T, Error>;

#[derive(Debug, Clone)]
pub(crate) struct Upstream {
    pub root: PathBuf,
    pub url: GitUrl,
    pub branch: String,
}

impl Upstream {
    fn parse(remote: &str) -> Result<GitUrl> {
        let parsed = GitUrl::parse(remote).map_err(|src| Error::ParsingUrl {
            url: remote.to_owned(),
            src,
        })?;

        Ok(parsed)
    }

    fn get_remote_url(repo: &Repository) -> Result<GitUrl> {
        let remote = repo.find_remote("origin").or_else(|_| {
            let remotes = repo
                .remotes()
                .map_err(|src| Error::FetchingRemotes { src })?;

            let name = remotes.get(0).ok_or(Error::NoRemote)?;

            // TODO: add hint about setting branch in basel config
            info!("`origin` not found, defaulting to first found branch: `{name}`");

            repo.find_remote(name).map_err(|src| Error::FetchingUrl {
                remote: name.to_owned(),
                src,
            })
        })?;

        let raw_url = remote.url().ok_or(Error::InvalidUrl)?;

        Self::parse(raw_url)
    }

    fn detect_default_branch(repo: &Repository) -> String {
        if let Ok(reference) = repo.find_reference("refs/remotes/origin/HEAD")
            && let Some(target) = reference.symbolic_target()
            && let Some(branch) = target.strip_prefix("refs/remotes/origin/")
        {
            return branch.to_owned();
        }

        "main".to_owned()
    }

    fn from_repo(repo: &Repository, root: PathBuf) -> Result<Self> {
        let url = Self::get_remote_url(repo)?;
        let branch = Self::detect_default_branch(repo);

        Ok(Self { root, url, branch })
    }
}

#[derive(Debug, Default)]
pub(crate) struct Cache(IndexMap<PathBuf, Option<Upstream>>);

impl Cache {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self(IndexMap::new())
    }

    pub(crate) fn get_or_detect(&mut self, render_path: &Path) -> Option<Upstream> {
        let Ok(repo) = Repository::discover(render_path) else {
            warn!(
                "failed to discover git repo from path `{}`",
                render_path.display()
            );
            return None;
        };

        let Some(root) = repo.workdir() else {
            warn!("git repo has no working dir (bare repo?)");
            return None;
        };

        let root = root.to_path_buf();

        if let Some(cached) = self.0.get(&root) {
            return cached.clone();
        }

        let info = Upstream::from_repo(&repo, root.clone()).ok();

        if info.is_none() {
            warn!("failed to extract info from repo at `{}`", root.display());
        }

        self.0.insert(root, info.clone());

        info
    }
}

#[non_exhaustive]
#[derive(Debug, Default, Clone)]
pub(crate) struct Special {
    pub upstream_repo: Option<String>,
    pub upstream_file: Option<String>,
}
