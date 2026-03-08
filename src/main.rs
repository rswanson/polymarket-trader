mod cli;
mod signer;

use clap::Parser;
use cli::Cli;

fn main() {
    let _cli = Cli::parse();
    println!("parsed CLI args successfully");
}
