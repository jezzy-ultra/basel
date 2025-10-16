use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result as AnyhowResult};
use indexmap::IndexMap;
use log::{debug, info, warn};
use minijinja::Template as JinjaTemplate;

use crate::config::Config;
use crate::directives::Directives;
use crate::format::format;
use crate::manifest::{FileStatus, Manifest};
use crate::schemes::Scheme;
use crate::templates::Loader;
use crate::upstream::{GitCache, GitInfo};
use crate::{
    Error, JINJA_TEMPLATE_SUFFIX, Result, SCHEME_MARKER, SET_TEST_OBJECT, SKIP_RENDERING_PREFIX,
    SWATCH_MARKER, SWATCH_VARIABLE, Special, upstream,
};

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    Smart,
    Skip,
    Force,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decision {
    Create,
    Recreate,
    Update,
    Overwrite,
    Skip,
    Conflict,
}

impl Decision {
    const fn should_write(self) -> bool {
        use Decision::{Conflict, Create, Overwrite, Recreate, Skip, Update};

        match self {
            Create | Recreate | Update | Overwrite => true,
            Skip | Conflict => false,
        }
    }

    const fn log_action(self) -> &'static str {
        use Decision::{Conflict, Create, Overwrite, Recreate, Skip, Update};

        match self {
            Create => "creating",
            Recreate => "recreating",
            Update => "updating",
            Overwrite => "overwriting",
            Skip => "skipped",
            Conflict => "conflict",
        }
    }
}

const fn decide_write(status: FileStatus, mode: WriteMode) -> Decision {
    use Decision::{Conflict, Create, Overwrite, Recreate, Skip, Update};
    use FileStatus::{NotTracked, Tracked};

    match (status, mode) {
        (NotTracked, _) => Create,
        (
            Tracked {
                file_exists: false, ..
            },
            _,
        ) => Recreate,
        (
            Tracked {
                user_modified: true,
                ..
            },
            WriteMode::Force,
        ) => Overwrite,
        (
            Tracked {
                user_modified: true,
                ..
            },
            WriteMode::Smart,
        ) => Conflict,
        (
            Tracked {
                user_modified: false,
                template_changed: false,
                scheme_changed: false,
                ..
            },
            _,
        )
        | (Tracked { .. }, WriteMode::Skip) => Skip,
        (
            Tracked {
                user_modified: false,
                ..
            },
            _,
        ) => Update,
    }
}

fn uses_swatch_iteration(template_name: &str) -> bool {
    template_name.contains(SWATCH_MARKER)
}

fn resolve_path(
    template_name: &str,
    scheme_name: &str,
    config: &Config,
    swatch_name: Option<&str>,
) -> AnyhowResult<PathBuf> {
    let relative_path = template_name
        .strip_suffix(JINJA_TEMPLATE_SUFFIX)
        .unwrap_or(template_name);

    let filename = Path::new(relative_path)
        .file_name()
        .ok_or_else(|| Error::InternalBug {
            module: "render",
            reason: format!("attempted to render to corrupted path `{relative_path}`"),
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
    config: &Config,
) -> Special {
    let Some(upstream_cfg) = &config.upstream else {
        return Special::default();
    };

    let Some((git_info, rel_path)) = (if let Some(repo_path) = upstream_cfg.repo_path.as_deref() {
        resolve_with_repo_path(
            repo_path,
            scheme_name,
            &config.dirs.render,
            render_path,
            git_cache,
        )
    } else {
        resolve_with_autodetect(render_path, git_cache)
    }) else {
        return Special::default();
    };

    let pattern_override = upstream_cfg.pattern.as_deref();
    let branch_override = upstream_cfg.branch.as_deref();

    let url = upstream::build_url(&git_info, &rel_path, pattern_override, branch_override);
    let repo = upstream::extract_base_url(&url);

    Special {
        upstream_file: Some(url),
        upstream_repo: repo,
    }
}

fn should_render(name: &str) -> bool {
    !name
        .split('/')
        .any(|p| p.starts_with(SKIP_RENDERING_PREFIX))
}

#[non_exhaustive]
#[derive(Debug)]
pub struct Context {
    pub manifest: Manifest,
    pub git_cache: GitCache,
    pub write_mode: WriteMode,
    pub dry_run: bool,
}

impl Context {
    pub fn new(write_mode: WriteMode, dry_run: bool) -> Result<Self> {
        Ok(Self {
            manifest: Manifest::load_or_create()?,
            git_cache: GitCache::new(),
            write_mode,
            dry_run,
        })
    }

    pub fn save(self) -> Result<()> {
        if !self.dry_run {
            self.manifest.save()?;
        }

        Ok(())
    }
}

// TODO: break up big function by extracting helper(s)
fn render_single(
    scheme: &Scheme,
    template_name: &str,
    template: &JinjaTemplate<'_, '_>,
    directives: &Directives,
    config: &Config,
    context: &mut Context,
    current_swatch: Option<&str>,
) -> AnyhowResult<()> {
    let scheme_name = scheme.name.as_str();

    let path = resolve_path(template_name, scheme_name, config, current_swatch)?;

    let special = build_upstream(scheme_name, &path, &mut context.git_cache, config);

    let scheme_ctx = scheme.to_context(
        directives.config.color_format,
        directives.config.text_format,
        &special,
        current_swatch,
    )?;

    if !scheme_ctx.contains_key(SET_TEST_OBJECT) {
        return Err(Error::InternalBug {
            module: "render",
            reason: format!(
                "scheme `{scheme_name}` context for template `{template_name}` missing \
                 `{SET_TEST_OBJECT}` template variable"
            ),
        }
        .into());
    }

    let rendered = template.render(&scheme_ctx).with_context(|| {
        format!("rendering template `{template_name}` with scheme `{scheme_name}`")
    })?;

    let header = directives.make_header(&path);
    let output = format!("{header}{rendered}");

    let status = context.manifest.check_file(&path, template, scheme)?;
    let decision = decide_write(status, context.write_mode);

    match decision {
        // TODO: add interactive mode (possibly as default behavior?)
        Decision::Conflict => {
            warn!(
                "conflict: `{}` (user-modified, use `-f`/`--force` to overwrite)",
                path.display()
            );
        }
        _ if decision.should_write() => {
            if context.dry_run {
                info!(
                    "would write `{}` ({})",
                    path.display(),
                    decision.log_action()
                );
            } else {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("writing file `{}`", path.display()))?;
                }

                fs::write(&path, &output)
                    .with_context(|| format!("writing file `{}`", path.display()))?;

                let entry = Manifest::make_entry(path.clone(), template, scheme, &output)?;
                context.manifest.insert(entry);

                info!("generated `{}`", path.display());

                format(&path)?;
            }
        }
        _ => {
            debug!("skipped `{}` ({})", path.display(), decision.log_action());
        }
    }

    Ok(())
}

fn apply_internal(
    scheme: &Scheme,
    template_name: &str,
    template: &JinjaTemplate<'_, '_>,
    directives: &Directives,
    config: &Config,
    context: &mut Context,
) -> AnyhowResult<()> {
    if uses_swatch_iteration(template_name) {
        if !template.source().contains(SWATCH_VARIABLE) {
            warn!(
                "template `{template_name}` has `{SWATCH_MARKER}` in filename but doesn't use \
                 {SWATCH_VARIABLE} inside template",
            );
        }

        for swatch in &scheme.palette {
            render_single(
                scheme,
                template_name,
                template,
                directives,
                config,
                context,
                Some(swatch.name.as_str()),
            )?;
        }
    } else {
        render_single(
            scheme,
            template_name,
            template,
            directives,
            config,
            context,
            None,
        )?;
    }

    Ok(())
}

pub fn apply(
    scheme: &Scheme,
    template_name: &str,
    template: &JinjaTemplate<'_, '_>,
    directives: &Directives,
    config: &Config,
    context: &mut Context,
) -> Result<()> {
    apply_internal(scheme, template_name, template, directives, config, context)
        .map_err(Error::rendering)
}

fn scheme_internal(
    scheme: &Scheme,
    templates: &Loader,
    config: &Config,
    context: &mut Context,
) -> AnyhowResult<()> {
    for (template_name, (template, directives)) in templates.with_directives()? {
        if !should_render(template_name) {
            continue;
        }

        apply(
            scheme,
            template_name,
            &template,
            directives,
            config,
            context,
        )?;
    }

    Ok(())
}

pub fn scheme(
    scheme: &Scheme,
    templates: &Loader,
    config: &Config,
    context: &mut Context,
) -> Result<()> {
    scheme_internal(scheme, templates, config, context).map_err(Error::rendering)
}

fn all_internal(
    templates: &Loader,
    schemes: &IndexMap<String, Scheme>,
    config: &Config,
    write_mode: WriteMode,
    dry_run: bool,
) -> AnyhowResult<()> {
    let mut ctx = Context::new(write_mode, dry_run)?;

    for scheme_ref in schemes.values() {
        scheme(scheme_ref, templates, config, &mut ctx)?;
    }

    ctx.save()?;

    Ok(())
}

pub fn all(
    templates: &Loader,
    schemes: &IndexMap<String, Scheme>,
    config: &Config,
    write_mode: WriteMode,
    dry_run: bool,
) -> Result<()> {
    all_internal(templates, schemes, config, write_mode, dry_run).map_err(Error::rendering)
}
