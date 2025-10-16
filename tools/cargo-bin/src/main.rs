#![feature(more_qualified_paths)]
#![feature(stmt_expr_attributes)]
#![feature(str_as_str)]
#![feature(unqualified_local_imports)]
#![feature(supertrait_item_shadowing)]
#![feature(non_exhaustive_omitted_patterns_lint)]
#![feature(must_not_suspend)]
#![feature(multiple_supertrait_upcastable)]
#![feature(strict_provenance_lints)]
#![allow(missing_docs, reason = "todo: better documentation")]
#![allow(clippy::missing_docs_in_private_items, reason = "todo: documentation")]
#![allow(clippy::missing_errors_doc, reason = "todo: documentation")]
#![allow(clippy::redundant_pub_crate, reason = "a fuckton of false positives")]

use std::process;

fn main() {
    let res = cargo_run_bin::cli::run();

    // Only reached if run-bin code fails, otherwise process exits early from within
    // binary::run.
    if let Err(res) = res {
        eprintln!("\x1b[31m{}\x1b[0m", format_args!("run-bin failed: {res}"));
        process::exit(1);
    }
}
