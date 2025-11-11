//! Caches external binary crates in the local workspace instead of globally.
//!
//! Thin wrapper for **[cargo-run-bin]** invoked with `cargo bin` (or `cargo
//! b`), with the addition that shims are enabled and generated automatically
//! (and loaded by **[direnv]** if it's available) for using workspace-level CLI
//! tools without prefixing the command with `cargo bin`.
//!
//! # Examples
//!
//! Install or update the binary crates defined under
//! `[workspace.metadata.bin]` in `Cargo.toml`:
//!
//! ```shell
//! cargo bin --install
//! # or
//! cargo bin -i
//! ```
//!
//! Update `.cargo/config.toml` after adding new crates:
//!
//! ```shell
//! cargo bin --sync-aliases
//! # or
//! cargo bin -s
//! ```
//!
//! Once installed, use CLI tools directly, e.g.:
//!
//! ```shell
//! # you can use
//! dx serve
//! # instead of
//! cargo bin dx serve
//! ```
//!
//! [cargo-run-bin]: https://github.com/dustinblackman/cargo-run-bin
//! [direnv]: https://github.com/direnv/direnv

use std::process;

use cargo_run_bin::{cli, shims};

fn main() {
    let res = cli::run();

    if let Err(res) = res {
        eprintln!("\x1b[31m{}\x1b[0m", format_args!("run-bin failed: {res}"));
        process::exit(1);
    }

    // TODO: fix lazy implementation of always syncing shims on every invocation?
    let _ = shims::sync();
}
