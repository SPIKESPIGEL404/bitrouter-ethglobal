//! `bitrouter` CLI/TUI entry point. Filled in by Phase 5.

use clap::Parser;

/// BitRouter — an LLM API router.
#[derive(Parser)]
#[command(name = "bitrouter", version, about)]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
    println!(
        "bitrouter {} — CLI/TUI lands in Phase 5",
        bitrouter::VERSION
    );
}
