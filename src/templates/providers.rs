use std::result::Result as StdResult;

use git_url_parse::GitUrl;
use git_url_parse::types::provider::GenericProvider;
use globset::{Glob, GlobMatcher};
use regex::Regex;
use url::Url;

use crate::config::Provider;

type Result<T> = StdResult<T, Error>;

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("provider `{host}` missing required field `{field}`")]
    MissingRequired { field: String, host: String },

    // TODO: improve error message
    #[error("url `{url}` doesn't match expected format")]
    UnrecognizedPattern { url: String },

    #[error("no provider found for `{host}`")]
    NoneFound { host: String },

    #[error("no host found in url `{url}`")]
    Nohost { url: String },

    #[error("failed to parse url `{url}`: {src}")]
    ParsingUrl { url: String, src: url::ParseError },

    #[error("error parsing pattern for provider `{host}`: {src}")]
    ParsingGlob { host: String, src: globset::Error },
}

#[derive(Debug, Clone)]
pub(crate) struct Resolved {
    host: String,
    blob_path: String,
    raw_path: String,
    branch: Option<String>,
    matcher: GlobMatcher,
}

impl Provider {
    fn resolve(self) -> Result<Resolved> {
        let matcher = Glob::new(&self.host)
            .map_err(|src| Error::ParsingGlob {
                host: self.host.clone(),
                src,
            })?
            .compile_matcher();

        Ok(Resolved {
            host: self.host.clone(),
            blob_path: self.blob_path.ok_or_else(|| Error::MissingRequired {
                field: "blob_path".to_owned(),
                host: self.host.clone(),
            })?,
            raw_path: self.raw_path.ok_or_else(|| Error::MissingRequired {
                field: "raw_path".to_owned(),
                host: self.host.clone(),
            })?,
            branch: self.branch,
            matcher,
        })
    }
}

#[derive(Debug)]
struct Components<'a> {
    host: &'a str,
    owner: &'a str,
    repo: &'a str,
    ref_: &'a str,
    file: &'a str,
}

pub(crate) fn resolve(merged: &[Provider]) -> Result<Vec<Resolved>> {
    let mut resolved: Vec<Resolved> = merged
        .iter()
        .map(|h| h.clone().resolve())
        .collect::<Result<Vec<Resolved>>>()?;

    resolved.sort_by(|a, b| {
        let a_spec = calculate_specificity(&a.host);
        let b_spec = calculate_specificity(&b.host);

        b_spec.cmp(&a_spec)
    });

    Ok(resolved)
}

pub(crate) fn resolve_blob(url: &str, providers: &[Resolved]) -> Result<String> {
    let parsed = Url::parse(url).map_err(|src| Error::ParsingUrl {
        url: url.to_owned(),
        src,
    })?;

    let host = parsed.host_str().ok_or_else(|| Error::Nohost {
        url: url.to_owned(),
    })?;

    let provider = find_matching(host, providers).ok_or_else(|| Error::NoneFound {
        host: host.to_owned(),
    })?;

    let components = extract_components(&parsed, url)?;

    let raw = apply(&provider.raw_path, &components);

    Ok(format!("https://{raw}"))
}

pub(crate) fn build_blob(
    url: &GitUrl,
    path: &str,
    ref_: &str,
    providers: &[Resolved],
) -> Result<String> {
    let host = url.host().ok_or_else(|| Error::Nohost {
        url: url.to_string(),
    })?;

    let resolved = find_matching(host, providers).ok_or_else(|| Error::NoneFound {
        host: host.to_owned(),
    })?;

    let provider =
        url.provider_info::<GenericProvider>()
            .map_err(|_src| Error::UnrecognizedPattern {
                url: url.to_string(),
            })?;

    let owner = provider.owner();
    let repo = provider.repo();

    let blob = resolved
        .blob_path
        .replace("{host}", host)
        .replace("{owner}", owner)
        .replace("{repo}", repo)
        .replace("{ref}", ref_)
        .replace("{file}", path);

    Ok(format!("https://{blob}"))
}

pub(crate) fn extract_repo_url(blob: &str) -> Result<Option<String>> {
    let parsed = Url::parse(blob).map_err(|src| Error::ParsingUrl {
        url: blob.to_owned(),
        src,
    })?;

    let components = extract_components(&parsed, blob)?;

    Ok(Some(format!(
        "https://{}/{}/{}",
        components.host, components.owner, components.repo
    )))
}

// TODO: refactor to use constants (and iterate over them?)
fn calculate_specificity(pattern: &str) -> (usize, usize, usize) {
    let is_exact = !pattern.contains('*') && !pattern.contains('?') && !pattern.contains('{');

    if is_exact {
        let labels = pattern.split('.').count();

        return (labels, usize::MAX, 0);
    }

    let labels = pattern.split('.').count();

    let literals = pattern
        .chars()
        .filter(|&c| !matches!(c, '*' | '?' | '{' | '}' | ','))
        .count();

    let wildcards =
        pattern.matches('*').count() + pattern.matches('?').count() + pattern.matches('{').count();

    (labels, literals, wildcards)
}

fn find_matching<'a>(host: &str, providers: &'a [Resolved]) -> Option<&'a Resolved> {
    for provider in providers {
        if provider.host == host {
            return Some(provider);
        }
    }

    providers.iter().find(|&p| p.matcher.is_match(host))
}

fn extract_components<'a>(url: &'a Url, original: &str) -> Result<Components<'a>> {
    let path = url.path();

    // TODO: verify correctness and robustness
    let re = Regex::new(
        "^/(?P<owner>[^/]+)/(?P<repo>[^/]+)(?:/(?:-|src/branch))?/(?:blob|raw)/(?P<ref>[^/]+)/(?\
         P<file>.+)$",
    )
    .expect("regex should be valid");

    let caps = re
        .captures(path)
        .ok_or_else(|| Error::UnrecognizedPattern {
            url: original.to_owned(),
        })?;

    Ok(Components {
        host: url.host_str().expect("already validated by regex").as_str(),
        owner: caps
            .name("owner")
            .expect("already validated by regex")
            .as_str(),
        repo: caps
            .name("repo")
            .expect("already validated by regex")
            .as_str(),
        ref_: caps
            .name("ref")
            .expect("already validated by regex")
            .as_str(),
        file: caps
            .name("file")
            .expect("already validated by regex")
            .as_str(),
    })
}

fn apply(pattern: &str, components: &Components<'_>) -> String {
    pattern
        .replace("{host}", components.host)
        .replace("{owner}", components.owner)
        .replace("{repo}", components.repo)
        .replace("{ref}", components.ref_)
        .replace("{file}", components.file)
}
