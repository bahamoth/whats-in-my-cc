mod cli;

use clap::Parser;

fn main() {
    let _cli = cli::Cli::parse();
    // Subcommand handlers will be wired in later tasks.
}
