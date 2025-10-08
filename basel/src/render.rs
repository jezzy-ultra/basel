use std::collections::BTreeMap;
use std::fs;
use std::io::Error as IoError;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use log::{info, warn};
use minijinja::{Error as JinjaError, ErrorKind as JinjaErrorKind, Value as JinjaValue};

use crate::schemes::Scheme;
use crate::templates::Loader;
use crate::upstream::{GitCache, GitInfo};
use crate::{Result, upstream};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to render template `{template}` with scheme `{scheme}`: {src}")]
    Rendering {
        template: String,
        scheme: String,
        src: JinjaError,
    },
    #[error("failed to create output directory `{path}`: {src}")]
    CreatingDirectory { path: String, src: IoError },
    #[error("failed to write rendered file `{path}`: {src}")]
    WritingFile { path: String, src: IoError },
    #[error("{0}")]
    InternalBug(String),
}

#[derive(Default)]
struct Config {
    render_swatch_names: bool,
}

impl Config {
    fn parse_bool(directive: &str, val: &str, template_name: &str) -> bool {
        match val {
            "true" => true,
            "false" => false,
            _ => {
                warn!(
                    "Invalid value `{val}` for directive `{directive}` in {template_name}: \
                     expected `true` or `false`, defaulting to false"
                );

                false
            }
        }
    }

    fn parse(directives: &IndexMap<String, String>, template_name: &str) -> Self {
        let mut render_cfg = Self::default();

        for (directive, val) in directives {
            match directive.as_str() {
                "render_swatch_names" => {
                    render_cfg.render_swatch_names =
                        Self::parse_bool(directive, val, template_name);
                }
                _ => {
                    warn!(
                        "Unknown directive `{directive}` with value `{val}` in `{template_name}`, \
                         ignoring"
                    );
                }
            }
        }

        render_cfg
    }
}

fn uses_swatch_iteration(template_name: &str) -> bool {
    template_name.contains("SWATCH")
}

fn resolve_path(
    cfg: &crate::Config,
    template_name: &str,
    scheme_name: &str,
    swatch_name: Option<&str>,
) -> Result<PathBuf> {
    let relative_path = template_name
        .strip_suffix(".jinja")
        .unwrap_or(template_name);

    let filename = Path::new(relative_path)
        .file_name()
        .ok_or_else(|| {
            Error::InternalBug(format!(
                "attempted to render to corrupted path `{relative_path}`"
            ))
        })?
        .to_string_lossy();

    let parent_dirs = Path::new(relative_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));

    if swatch_name.is_some() && !filename.contains("SWATCH") {
        log::warn!(
            "`{template_name}` has `SWATCH` in its name but doesn't use it inside the template"
        );
    }

    let output = swatch_name.map_or_else(
        || filename.replace("SCHEME", scheme_name),
        |swatch| {
            filename
                .replace("SCHEME", scheme_name)
                .replace("SWATCH", swatch)
        },
    );

    Ok(Path::new(&cfg.dirs.render)
        .join(scheme_name)
        .join(parent_dirs)
        .join(output))
}

fn file(
    template: &minijinja::Template<'_, '_>,
    scheme_name: &str,
    context: &BTreeMap<String, JinjaValue>,
    output_path: &PathBuf,
) -> Result<()> {
    if !context.contains_key("_set") {
        return Err(Error::InternalBug("context missing `_set`".to_owned()).into());
    }

    let rendered = template.render(context).map_err(|src| match src.kind() {
        JinjaErrorKind::UndefinedError => Error::InternalBug(format!("{src}")),
        _ => Error::Rendering {
            template: template.name().to_owned(),
            scheme: scheme_name.to_owned(),
            src,
        },
    })?;

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|src| Error::CreatingDirectory {
            path: parent.to_string_lossy().to_string(),
            src,
        })?;
    }

    fs::write(output_path, rendered).map_err(|src| Error::WritingFile {
        path: output_path.to_string_lossy().to_string(),
        src,
    })?;
    info!("generated {}", output_path.display());

    Ok(())
}

fn strip_prefix(path: &Path, prefix: &Path, ctx: &str) -> Option<PathBuf> {
    path.strip_prefix(prefix)
        .ok()
        .or_else(|| {
            warn!(
                "{ctx}... failed to strip prefix `{}` from path `{}`",
                prefix.display(),
                path.display()
            );

            None
        })
        .map(Path::to_path_buf)
}

fn git_info_with(
    git_cache: &mut GitCache,
    target_path: &Path,
    ctx: &str,
) -> Option<(GitInfo, PathBuf)> {
    let git_info = git_cache.get_or_detect(target_path)?;

    let rel_path = strip_prefix(
        target_path,
        &git_info.root,
        &format!("{ctx}... path not under repo root"),
    )?;

    Some((git_info, rel_path))
}

fn resolve_with_repo_path(
    git_cache: &mut GitCache,
    render_path: &Path,
    scheme_name: &str,
    repo_path: &Path,
    render_dir: &str,
) -> Option<(GitInfo, PathBuf)> {
    let prefix = Path::new(render_dir).join(scheme_name);
    let rel_path = strip_prefix(render_path, &prefix, "configuring repo_path mode")?;

    let target_path = repo_path.join(&rel_path);
    git_info_with(git_cache, &target_path, "configuring repo_path mode")
}

fn resolve_with_autodetect(
    git_cache: &mut GitCache,
    render_path: &Path,
) -> Option<(GitInfo, PathBuf)> {
    let abs_path = render_path.canonicalize().ok().or_else(|| {
        warn!(
            "auto-detect mode... failed to canonicalize render path `{}`; file may not exist yet",
            render_path.display()
        );

        None
    })?;

    git_info_with(git_cache, &abs_path, "auto-detect mode")
}

fn build_upstream(
    git_cache: &mut GitCache,
    render_path: &Path,
    scheme_name: &str,
    cfg: &crate::Config,
) -> Option<String> {
    let upstream_cfg = cfg.upstream.as_ref();

    let (git_info, rel_path) =
        if let Some(repo_path) = upstream_cfg.and_then(|u| u.repo_path.as_ref()) {
            resolve_with_repo_path(
                git_cache,
                render_path,
                scheme_name,
                repo_path,
                &cfg.dirs.render,
            )?
        } else {
            resolve_with_autodetect(git_cache, render_path)?
        };

    let pattern_override = upstream_cfg.and_then(|u| u.pattern.as_deref());
    let branch_override = upstream_cfg.and_then(|u| u.branch.as_deref());

    Some(upstream::build_url(
        &git_info,
        &rel_path,
        pattern_override,
        branch_override,
    ))
}

pub fn all(
    cfg: &crate::Config,
    templates: &Loader,
    schemes: &IndexMap<String, Scheme>,
) -> Result<()> {
    for (template_name, (template, directives)) in templates.templates_with_directives() {
        let render_cfg = Config::parse(&directives, template_name);
        let mut git_cache = GitCache::new();

        if uses_swatch_iteration(template_name) {
            for (scheme_name, scheme) in schemes {
                for swatch_name in scheme.palette.keys() {
                    let path = resolve_path(cfg, template_name, scheme_name, Some(swatch_name))?;
                    let upstream_url = build_upstream(&mut git_cache, &path, scheme_name, cfg);
                    let ctx = scheme.to_context(
                        render_cfg.render_swatch_names,
                        Some(swatch_name),
                        upstream_url.as_deref(),
                    )?;

                    file(&template, scheme_name, &ctx, &path)?;
                }
            }
        } else {
            for (scheme_name, scheme) in schemes {
                let path = resolve_path(cfg, template_name, scheme_name, None)?;
                let upstream_url = build_upstream(&mut git_cache, &path, scheme_name, cfg);
                let ctx = scheme.to_context(
                    render_cfg.render_swatch_names,
                    None,
                    upstream_url.as_deref(),
                )?;

                file(&template, scheme_name, &ctx, &path)?;
            }
        }
    }

    Ok(())
}
