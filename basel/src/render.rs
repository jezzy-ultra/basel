use std::collections::BTreeMap;
use std::fs;
use std::io::Error as IoError;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use log::{info, warn};
use minijinja::{Error as JinjaError, ErrorKind as JinjaErrorKind, Value as JinjaValue};

use crate::directives::Directives;
use crate::format::format;
use crate::schemes::Scheme;
use crate::templates::Loader;
use crate::upstream::{GitCache, GitInfo};
use crate::{
    Config as BaselConfig, Result, SCHEME_MARKER, SKIP_RENDERING_PREFIX, SWATCH_MARKER, Special,
    TEMPLATE_SUFFIX, upstream,
};

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

fn uses_swatch_iteration(template_name: &str) -> bool {
    template_name.contains(SWATCH_MARKER)
}

fn resolve_path(
    template_name: &str,
    scheme_name: &str,
    config: &BaselConfig,
    swatch_name: Option<&str>,
) -> Result<PathBuf> {
    let relative_path = template_name
        .strip_suffix(TEMPLATE_SUFFIX)
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

    let output = swatch_name.map_or_else(
        || filename.replace(SCHEME_MARKER, scheme_name),
        |swatch| {
            filename
                .replace(SCHEME_MARKER, scheme_name)
                .replace(SWATCH_MARKER, swatch)
        },
    );

    Ok(Path::new(&config.dirs.render)
        .join(scheme_name)
        .join(parent_dirs)
        .join(output))
}

fn file(
    context: &BTreeMap<String, JinjaValue>,
    template: &minijinja::Template<'_, '_>,
    scheme_name: &str,
    render_path: &PathBuf,
    directives: &Directives,
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

    let header = directives.make_header(render_path);
    let output = format!("{header}{rendered}");

    if let Some(parent) = render_path.parent() {
        fs::create_dir_all(parent).map_err(|src| Error::CreatingDirectory {
            path: parent.to_string_lossy().to_string(),
            src,
        })?;
    }

    fs::write(render_path, output).map_err(|src| Error::WritingFile {
        path: render_path.to_string_lossy().to_string(),
        src,
    })?;
    info!("generated {}", render_path.display());

    format(render_path)?;

    Ok(())
}

fn strip_prefix(path: &Path, prefix: &Path, context: &str) -> Option<PathBuf> {
    path.strip_prefix(prefix)
        .ok()
        .or_else(|| {
            warn!(
                "{context}... failed to strip prefix `{}` from path `{}`",
                prefix.display(),
                path.display()
            );

            None
        })
        .map(Path::to_path_buf)
}

fn git_info_with(
    target_path: &Path,
    context: &str,
    git_cache: &mut GitCache,
) -> Option<(GitInfo, PathBuf)> {
    let git_info = git_cache.get_or_detect(target_path)?;

    let rel_path = strip_prefix(
        target_path,
        git_info.root(),
        &format!("{context}... path not under repo root"),
    )?;

    Some((git_info, rel_path))
}

fn resolve_with_repo_path(
    repo_path: &Path,
    scheme_name: &str,
    render_dir: &str,
    render_path: &Path,
    git_cache: &mut GitCache,
) -> Option<(GitInfo, PathBuf)> {
    let prefix = Path::new(render_dir).join(scheme_name);
    let rel_path = strip_prefix(render_path, &prefix, "configuring repo_path mode")?;

    let target_path = repo_path.join(&rel_path);
    git_info_with(&target_path, "configuring repo_path mode", git_cache)
}

fn resolve_with_autodetect(
    render_path: &Path,
    git_cache: &mut GitCache,
) -> Option<(GitInfo, PathBuf)> {
    let abs_path = render_path.canonicalize().ok().or_else(|| {
        warn!(
            "auto-detect mode... failed to canonicalize render path `{}`; file may not exist yet",
            render_path.display()
        );

        None
    })?;

    git_info_with(&abs_path, "auto-detect mode", git_cache)
}

fn build_upstream(
    scheme_name: &str,
    render_path: &Path,
    git_cache: &mut GitCache,
    config: &BaselConfig,
) -> Special {
    let upstream_cfg = config.upstream.as_ref();

    let Some((git_info, rel_path)) =
        (if let Some(repo_path) = upstream_cfg.and_then(|u| u.repo_path.as_ref()) {
            resolve_with_repo_path(
                repo_path,
                scheme_name,
                &config.dirs.render,
                render_path,
                git_cache,
            )
        } else {
            resolve_with_autodetect(render_path, git_cache)
        })
    else {
        return Special::default();
    };

    let pattern_override = upstream_cfg.and_then(|u| u.pattern.as_deref());
    let branch_override = upstream_cfg.and_then(|u| u.branch.as_deref());

    let url = upstream::build_url(&git_info, &rel_path, pattern_override, branch_override);
    let repo = upstream::extract_base_url(&url);

    Special {
        upstream_file: Some(url),
        upstream_repo: repo,
    }
}

struct RenderContext<'a> {
    scheme: &'a Scheme,
    scheme_name: &'a str,
    template_name: &'a str,
    directives: &'a Directives,
    config: &'a BaselConfig,
    git_cache: &'a mut GitCache,
    current_swatch: Option<&'a str>,
}

fn render_one(
    template: &minijinja::Template<'_, '_>,
    render_ctx: &mut RenderContext<'_>,
) -> Result<()> {
    let path = resolve_path(
        render_ctx.template_name,
        render_ctx.scheme_name,
        render_ctx.config,
        render_ctx.current_swatch,
    )?;

    let special = build_upstream(
        render_ctx.scheme_name,
        &path,
        render_ctx.git_cache,
        render_ctx.config,
    );

    let ctx = render_ctx.scheme.to_context(
        render_ctx.directives.config().color_format(),
        render_ctx.directives.config().text_format(),
        &special,
        render_ctx.current_swatch,
    )?;

    file(
        &ctx,
        template,
        render_ctx.scheme_name,
        &path,
        render_ctx.directives,
    )
}

fn should_render(name: &str) -> bool {
    !name
        .split('/')
        .any(|p| p.starts_with(SKIP_RENDERING_PREFIX))
}

pub fn render(
    templates: &Loader,
    schemes: &IndexMap<String, Scheme>,
    config: &BaselConfig,
) -> Result<()> {
    let mut git_cache = GitCache::new();

    for (template_name, (template, directives)) in templates.templates_with_directives()? {
        if should_render(template_name) {
            if uses_swatch_iteration(template_name) {
                if !template.source().contains(SWATCH_MARKER) {
                    warn!(
                        "template `{template_name}` has `{SWATCH_MARKER}` in filename but doesn't \
                         use it inside template"
                    );
                }

                for (scheme_name, scheme) in schemes {
                    for swatch in &scheme.palette {
                        render_one(&template, &mut RenderContext {
                            scheme,
                            scheme_name,
                            template_name,
                            directives,
                            config,
                            git_cache: &mut git_cache,
                            current_swatch: Some(swatch.name().as_str()),
                        })?;
                    }
                }
            } else {
                for (scheme_name, scheme) in schemes {
                    render_one(&template, &mut RenderContext {
                        scheme,
                        scheme_name,
                        template_name,
                        directives,
                        config,
                        git_cache: &mut git_cache,
                        current_swatch: None,
                    })?;
                }
            }
        }
    }

    Ok(())
}
