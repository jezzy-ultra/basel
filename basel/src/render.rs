use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use indexmap::IndexMap;
use log::{debug, info, warn};

use crate::output::upstream::{Cache, Special};
use crate::output::{Decision, Upstream, WriteMode, format, strategy};
use crate::templates::{
    Directives, JINJA_TEMPLATE_SUFFIX, Loader, ResolvedHost, SET_TEST_OBJECT,
    SKIP_RENDERING_PREFIX, hosts,
};
use crate::{Config, Error, Result, Scheme};

mod context;
mod index;
mod objects;

use self::index::Index;
use self::objects::Color;

const SCHEME_MARKER: &str = "SCHEME";
const SWATCH_MARKER: &str = "SWATCH";
const SWATCH_VARIABLE: &str = "swatch";

#[non_exhaustive]
#[derive(Debug)]
pub(crate) struct Session {
    pub index: Index,
    pub hosts: Vec<ResolvedHost>,
    pub git_cache: Cache,
    pub write_mode: WriteMode,
    pub dry_run: bool,
}

impl Session {
    fn new(hosts: Vec<ResolvedHost>, write_mode: WriteMode, dry_run: bool) -> Result<Self> {
        Ok(Self {
            index: Index::load_or_create()?,
            hosts,
            git_cache: Cache::new(),
            write_mode,
            dry_run,
        })
    }

    fn save(self) -> Result<()> {
        if !self.dry_run {
            self.index.save()?;
        }

        Ok(())
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
) -> anyhow::Result<PathBuf> {
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
    git_cache: &mut Cache,
) -> Option<(Upstream, PathBuf)> {
    let git_info = git_cache.get_or_detect(target_path)?;

    let rel_path = strip_prefix(
        target_path,
        &git_info.root,
        &format!("{context}... path not under repo root"),
    )?;

    Some((git_info, rel_path))
}

fn resolve_with_autodetect(
    render_path: &Path,
    git_cache: &mut Cache,
) -> Option<(Upstream, PathBuf)> {
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
    session: &mut Session,
    config: &Config,
) -> Special {
    let Some((git_info, path)) = resolve_with_autodetect(render_path, &mut session.git_cache)
    else {
        return Special::default();
    };

    // investigate where replacing backslashes is needed here
    let file_path = path.to_string_lossy().replace('\\', "/");

    let branch = &git_info.branch;

    let Ok(blob) = hosts::build_blob(&git_info.url, &file_path, branch, &session.hosts) else {
        // FIXME: error handling
        let host = git_info.url.host().unwrap_or("unknown");
        warn!("failed to build blob url for domain `{host}`");
        return Special::default();
    };

    let repo = hosts::extract_repo_url(&blob).ok().flatten();

    Special {
        upstream_file: Some(blob),
        upstream_repo: repo,
    }
}

fn should_render(name: &str) -> bool {
    !name
        .split('/')
        .any(|p| p.starts_with(SKIP_RENDERING_PREFIX))
}

fn prepare(
    path: &Path,
    scheme: &Scheme,
    template_name: &str,
    template: &minijinja::Template<'_, '_>,
    directives: &Directives,
    special: &Special,
    current_swatch: Option<&str>,
) -> anyhow::Result<String> {
    let context = context::build(scheme, special, &directives.style, current_swatch)?;

    if !context.contains_key(SET_TEST_OBJECT) {
        return Err(Error::InternalBug {
            module: "render",
            reason: format!(
                "scheme `{}` context for template `{template_name}` missing `{SET_TEST_OBJECT}` \
                 template variable",
                scheme.name.as_str()
            ),
        }
        .into());
    }

    let rendered = template.render(&context).with_context(|| {
        format!(
            "rendering template `{template_name}` with scheme `{}`",
            scheme.name.as_str()
        )
    })?;

    let header = directives.make_header(path);

    Ok(format!("{header}{rendered}"))
}

fn execute(
    decision: Decision,
    path: &Path,
    output: &str,
    scheme: &Scheme,
    template: &minijinja::Template<'_, '_>,
    session: &mut Session,
) -> anyhow::Result<()> {
    match decision {
        // TODO: add interactive mode (possibly as default behavior?)
        Decision::Conflict => {
            warn!(
                "conflict: `{}` (user-modified, use `-f`/`--force` to overwrite)",
                path.display()
            );
        }
        _ if decision.should_write() => {
            if session.dry_run {
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

                fs::write(path, output)
                    .with_context(|| format!("writing file `{}`", path.display()))?;

                let entry = Index::create_entry(path, template, scheme, output)?;

                session.index.insert(entry);

                info!("generated `{}`", path.display());

                format(path)?;
            }
        }
        _ => {
            debug!("skipped `{}` ({})", path.display(), decision.log_action());
        }
    }

    Ok(())
}

fn write(
    scheme: &Scheme,
    template_name: &str,
    template: &minijinja::Template<'_, '_>,
    directives: &Directives,
    config: &Config,
    session: &mut Session,
    current_swatch: Option<&str>,
) -> anyhow::Result<()> {
    let scheme_name = scheme.name.as_str();
    let path = resolve_path(template_name, scheme_name, config, current_swatch)?;
    let special = build_upstream(scheme_name, &path, session, config);

    let output = prepare(
        &path,
        scheme,
        template_name,
        template,
        directives,
        &special,
        current_swatch,
    )?;

    let status = session.index.check(&path, scheme, template)?;
    let decision = strategy::decide(status, session.write_mode);

    execute(decision, &path, &output, scheme, template, session)?;

    Ok(())
}

pub(crate) fn apply(
    scheme: &Scheme,
    template_name: &str,
    template: &minijinja::Template<'_, '_>,
    directives: &Directives,
    config: &Config,
    session: &mut Session,
) -> Result<()> {
    apply_internal(scheme, template_name, template, directives, config, session)
        .map_err(Error::rendering)
}

fn apply_internal(
    scheme: &Scheme,
    template_name: &str,
    template: &minijinja::Template<'_, '_>,
    directives: &Directives,
    config: &Config,
    session: &mut Session,
) -> anyhow::Result<()> {
    if uses_swatch_iteration(template_name) {
        if !template.source().contains(SWATCH_VARIABLE) {
            warn!(
                "template `{template_name}` has `{SWATCH_MARKER}` in filename but doesn't use \
                 {SWATCH_VARIABLE} inside template",
            );
        }

        for swatch in &scheme.palette {
            write(
                scheme,
                template_name,
                template,
                directives,
                config,
                session,
                Some(swatch.name.as_str()),
            )?;
        }
    } else {
        write(
            scheme,
            template_name,
            template,
            directives,
            config,
            session,
            None,
        )?;
    }

    Ok(())
}

pub(crate) fn all_with(
    scheme: &Scheme,
    templates: &Loader,
    config: &Config,
    session: &mut Session,
) -> Result<()> {
    all_with_internal(scheme, templates, config, session).map_err(Error::rendering)
}

fn all_with_internal(
    scheme: &Scheme,
    templates: &Loader,
    config: &Config,
    session: &mut Session,
) -> anyhow::Result<()> {
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
            session,
        )?;
    }

    Ok(())
}

pub(crate) fn all(
    templates: &Loader,
    schemes: &IndexMap<String, Scheme>,
    config: &Config,
    write_mode: WriteMode,
    dry_run: bool,
) -> Result<()> {
    all_internal(templates, schemes, config, write_mode, dry_run).map_err(Error::rendering)
}

fn all_internal(
    templates: &Loader,
    schemes: &IndexMap<String, Scheme>,
    config: &Config,
    write_mode: WriteMode,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut session = Session::new(templates.hosts.clone(), write_mode, dry_run)?;

    for scheme_ref in schemes.values() {
        all_with(scheme_ref, templates, config, &mut session)?;
    }

    session.save()?;

    Ok(())
}
