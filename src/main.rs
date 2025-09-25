#![allow(clippy::cargo_common_metadata, reason = "todo: documentation")]
#![allow(clippy::missing_docs_in_private_items, reason = "todo: documentation")]
#![allow(clippy::missing_errors_doc, reason = "todo: documentation")]
#![allow(clippy::missing_panics_doc, reason = "todo: better error handling")]
#![allow(clippy::panic_in_result_fn, reason = "todo: better error handling")]
#![allow(clippy::panic, reason = "todo: better error handling")]
#![allow(clippy::unwrap_used, reason = "todo: better error handling")]

use basel::template::Templates;
use basel::{Config, Result, render, scheme};

fn main() -> Result<()> {
    let cfg = Config::default();
    let templates = Templates::new(&cfg.template_dir)?;
    let schemes = scheme::load_all(&cfg.scheme_dir)?;

    render::all(&cfg, &templates, &schemes)?;

    Ok(())
}
