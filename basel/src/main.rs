#![feature(supertrait_item_shadowing)]
#![feature(non_exhaustive_omitted_patterns_lint)]
#![feature(must_not_suspend)]
#![feature(multiple_supertrait_upcastable)]
#![feature(strict_provenance_lints)]
#![allow(missing_docs, reason = "TODO: add docs")]
#![allow(clippy::missing_errors_doc, reason = "TODO: add docs")]

use basel::{Result, cli};

fn main() -> Result<()> {
    cli::run()
}
