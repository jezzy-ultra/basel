use std::path::{Path, PathBuf};
use std::result::Result as StdResult;

use git_url_parse::{GitUrl, GitUrlParseError};
use git2::Repository;
use indexmap::IndexMap;
use log::warn;

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("failed to parse git url `{url}`: {src}")]
    Parsing { url: String, src: GitUrlParseError },
    #[error("failed to get the host in git url `{0}`")]
    Unsupported(String),
}

pub(crate) type Result<T> = StdResult<T, Error>;

#[derive(Debug, Clone)]
pub(crate) struct GitInfo {
    root: PathBuf,
    remote_url: String,
    remote_host: String,
    default_branch: String,
}

impl GitInfo {
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    #[expect(unused, reason = "getter for api")]
    pub(crate) fn remote_url(&self) -> &str {
        &self.remote_url
    }

    #[expect(unused, reason = "getter for api")]
    pub(crate) fn remote_host(&self) -> &str {
        &self.remote_host
    }

    #[expect(unused, reason = "getter for api")]
    pub(crate) fn default_branch(&self) -> &str {
        &self.default_branch
    }

    fn normalize(url: &str) -> Result<(String, String)> {
        let parsed = GitUrl::parse(url).map_err(|src| Error::Parsing {
            url: url.to_owned(),
            src,
        })?;

        let host = parsed
            .host()
            .ok_or_else(|| Error::Unsupported(url.to_owned()))?;

        let path = parsed
            .path()
            .trim_start_matches('/')
            .trim_end_matches(".git");

        let full_url = format!("https://{host}/{path}");

        Ok((full_url, host.to_owned()))
    }

    fn get_remote_url(repo: &Repository) -> Option<(String, String)> {
        let remote = if let Ok(remote) = repo.find_remote("origin") {
            remote
        } else {
            let remotes = repo.remotes().ok()?;
            let name = remotes.get(0)?;
            repo.find_remote(name).ok()?
        };

        let raw_url = remote.url()?;
        match Self::normalize(raw_url) {
            Ok((url, host)) => Some((url, host)),
            Err(e) => {
                warn!("failed to normalize git url `{raw_url}`: {e}");
                None
            }
        }
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

    fn from_repo(repo: &Repository, root: PathBuf) -> Option<Self> {
        let (remote_url, remote_host) = Self::get_remote_url(repo)?;
        let default_branch = Self::detect_default_branch(repo);

        Some(Self {
            root,
            remote_url,
            remote_host,
            default_branch,
        })
    }
}

fn infer_url_pattern(remote: &str) -> &'static str {
    let github_style = "{base}/blob/{branch}/{file}";
    let gitlab_style = "{base}/-/blob/{branch}/{file}";
    let gitea_style = "{base}/src/branch/{branch}/{file}";
    let bitbucket_style = "{base}/src/{branch}/{file}";

    match remote {
        "github.com" => github_style,
        "gitlab.com" => gitlab_style,
        "codeberg.org" => gitea_style,
        "bitbucket.org" => bitbucket_style,
        host => {
            if host.ends_with(".gitlab.com") || host.contains("gitlab.") {
                gitlab_style
            } else if host.contains("gitea") {
                gitea_style
            } else {
                github_style
            }
        }
    }
}

#[must_use]
pub(crate) fn build_url(
    git_info: &GitInfo,
    rel_path: &Path,
    pattern_override: Option<&str>,
    branch_override: Option<&str>,
) -> String {
    let file_path = rel_path.to_string_lossy().replace('\\', "/");

    let branch = branch_override.unwrap_or(&git_info.default_branch);
    let pattern = pattern_override.unwrap_or_else(|| infer_url_pattern(&git_info.remote_host));

    #[expect(
        clippy::literal_string_with_formatting_args,
        reason = "false positive on `{branch}`"
    )]
    let url = pattern
        .replace("{base}", &git_info.remote_url)
        .replace("{branch}", branch)
        .replace("{file}", &file_path);

    url
}

#[must_use]
pub(crate) fn extract_base_url(full_url: &str) -> Option<String> {
    let separators = ["/blob/", "/-/blob/", "/src/", "/src/branch/"];
    for separator in &separators {
        if let Some(pos) = full_url.find(separator) {
            return Some(full_url[..pos].to_owned());
        }
    }

    None
}

#[derive(Debug, Default)]
pub(crate) struct GitCache(IndexMap<PathBuf, Option<GitInfo>>);

impl GitCache {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self(IndexMap::new())
    }

    pub(crate) fn get_or_detect(&mut self, render_path: &Path) -> Option<GitInfo> {
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

        let info = GitInfo::from_repo(&repo, root.clone());
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
    pub upstream_file: Option<String>,
    pub upstream_repo: Option<String>,
}
