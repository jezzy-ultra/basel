use std::process;

fn main() {
    let res = cargo_run_bin::cli::run();

    if let Err(res) = res {
        eprintln!("\x1b[31m{}\x1b[0m", format_args!("run-bin failed: {res}"));
        process::exit(1);
    }
}
