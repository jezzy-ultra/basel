#![allow(missing_docs, reason = "TODO: add docs")]
#![allow(clippy::missing_docs_in_private_items, reason = "TODO: add docs")]
#![feature(unqualified_local_imports)]
#![feature(supertrait_item_shadowing)]
#![feature(non_exhaustive_omitted_patterns_lint)]
#![feature(must_not_suspend)]
#![feature(multiple_supertrait_upcastable)]
#![feature(strict_provenance_lints)]

use std::path::PathBuf;

use basel::templates::Loader;
use basel::{Config, Result, Upstream, render, schemes};

fn main() -> Result<()> {
    env_logger::init();

    let expanded = shellexpand::tilde("~/src/cutiepro");
    let repo_path = PathBuf::from(expanded.as_ref());
    let cfg = Config {
        upstream: Some(Upstream {
            repo_path: Some(repo_path),
            ..Default::default()
        }),
        ..Default::default()
    };

    let templates = Loader::new(&cfg.dirs.templates, &cfg.ignored_directives)?;
    let schemes = schemes::load_all(&cfg.dirs.schemes)?;

    render::all(&cfg, &templates, &schemes)?;

    Ok(())
}
