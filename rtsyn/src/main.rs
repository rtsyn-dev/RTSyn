mod commands;
mod handlers;
mod output;

use clap::Parser;
use commands::Cli;
use handlers::handle_command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    handle_command(cli.command)
}
