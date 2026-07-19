use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "mh3g-save-convert")]
struct Cli;

fn main() {
    Cli::parse();
}
