#![allow(missing_docs, reason = "TODO: add docs")]
#![allow(clippy::missing_docs_in_private_items, reason = "TODO: add docs")]
#![feature(supertrait_item_shadowing)]
#![feature(non_exhaustive_omitted_patterns_lint)]
#![feature(must_not_suspend)]
#![feature(multiple_supertrait_upcastable)]
#![feature(strict_provenance_lints)]

use basel::{Result, cli};

fn main() -> Result<()> {
    cli::run()
}
