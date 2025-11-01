use std::result::Result as StdResult;

use git_url_parse::GitUrl;
use git_url_parse::types::provider::GenericProvider;
use globset::{Glob, GlobMatcher};
use regex::Regex;
use url::Url;

use crate::config::Host;

type Result<T> = StdResult<T, Error>;

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("host `{domain}` missing required field `{field}`")]
    MissingRequired { field: String, domain: String },

    // TODO: improve error message
    #[error("url `{url}` doesn't match expected format")]
    UnrecognizedPattern { url: String },

    #[error("no host found for `{domain}`")]
    NoMatchingHost { domain: String },

    #[error("no domain found in url `{url}`")]
    NoDomain { url: String },

    #[error("failed to parse url `{url}`: {src}")]
    ParsingUrl { url: String, src: url::ParseError },

    #[error("error parsing pattern for host `{domain}`: {src}")]
    ParsingGlob { domain: String, src: globset::Error },
}

#[derive(Debug, Clone)]
pub(crate) struct Resolved {
    domain: String,
    blob_path: String,
    raw_path: String,
    branch: Option<String>,
    matcher: GlobMatcher,
}

impl Host {
    fn resolve(self) -> Result<Resolved> {
        let matcher = Glob::new(&self.domain)
            .map_err(|src| Error::ParsingGlob {
                domain: self.domain.clone(),
                src,
            })?
            .compile_matcher();

        Ok(Resolved {
            domain: self.domain.clone(),
            blob_path: self.blob_path.ok_or_else(|| Error::MissingRequired {
                field: "blob_path".to_owned(),
                domain: self.domain.clone(),
            })?,
            raw_path: self.raw_path.ok_or_else(|| Error::MissingRequired {
                field: "raw_path".to_owned(),
                domain: self.domain.clone(),
            })?,
            branch: self.branch,
            matcher,
        })
    }
}

#[derive(Debug)]
struct Components<'a> {
    domain: &'a str,
    owner: &'a str,
    repo: &'a str,
    rev: &'a str,
    file: &'a str,
}

pub(crate) fn resolve(merged: &[Host]) -> Result<Vec<Resolved>> {
    let mut resolved: Vec<Resolved> = merged
        .iter()
        .map(|h| h.clone().resolve())
        .collect::<Result<Vec<Resolved>>>()?;

    resolved.sort_by(|a, b| {
        let a_spec = calculate_specificity(&a.domain);
        let b_spec = calculate_specificity(&b.domain);

        b_spec.cmp(&a_spec)
    });

    Ok(resolved)
}

pub(crate) fn resolve_blob(url: &str, hosts: &[Resolved]) -> Result<String> {
    let parsed = Url::parse(url).map_err(|src| Error::ParsingUrl {
        url: url.to_owned(),
        src,
    })?;

    let domain = parsed.host_str().ok_or_else(|| Error::NoDomain {
        url: url.to_owned(),
    })?;

    let host = find_matching(domain, hosts).ok_or_else(|| Error::NoMatchingHost {
        domain: domain.to_owned(),
    })?;

    let components = extract_components(&parsed, url)?;

    let raw = apply(&host.raw_path, &components);

    Ok(format!("https://{raw}"))
}

pub(crate) fn build_blob(
    url: &GitUrl,
    path: &str,
    rev: &str,
    hosts: &[Resolved],
) -> Result<String> {
    let domain = url.host().ok_or_else(|| Error::NoDomain {
        url: url.to_string(),
    })?;

    let host = find_matching(domain, hosts).ok_or_else(|| Error::NoMatchingHost {
        domain: domain.to_owned(),
    })?;

    let provider =
        url.provider_info::<GenericProvider>()
            .map_err(|_| Error::UnrecognizedPattern {
                url: url.to_string(),
            })?;

    let owner = provider.owner();
    let repo = provider.repo();

    let blob = host
        .blob_path
        .replace("{domain}", domain)
        .replace("{owner}", owner)
        .replace("{repo}", repo)
        .replace("{rev}", rev)
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
        components.domain, components.owner, components.repo
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

fn find_matching<'a>(domain: &str, hosts: &'a [Resolved]) -> Option<&'a Resolved> {
    for host in hosts {
        if host.domain == domain {
            return Some(host);
        }
    }

    hosts.iter().find(|&host| host.matcher.is_match(domain))
}

fn extract_components<'a>(url: &'a Url, original: &str) -> Result<Components<'a>> {
    let path = url.path();

    // verify correctness and robustness
    let re = Regex::new(
        "^/(?P<owner>[^/]+)/(?P<repo>[^/]+)(?:/(?:-|src/branch))?/(?:blob|raw)/(?P<rev>[^/]+)/(?\
         P<file>.+)$",
    )
    .expect("regex should be valid");

    let caps = re
        .captures(path)
        .ok_or_else(|| Error::UnrecognizedPattern {
            url: original.to_owned(),
        })?;

    Ok(Components {
        domain: url.host_str().expect("already validated by regex").as_str(),
        owner: caps
            .name("owner")
            .expect("already validated by regex")
            .as_str(),
        repo: caps
            .name("repo")
            .expect("already validated by regex")
            .as_str(),
        rev: caps
            .name("rev")
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
        .replace("{domain}", components.domain)
        .replace("{owner}", components.owner)
        .replace("{repo}", components.repo)
        .replace("{rev}", components.rev)
        .replace("{file}", components.file)
}
