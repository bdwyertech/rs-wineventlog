#![cfg(windows)]

mod config;
mod eventlog;
mod output;
mod privilege;
mod xml;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "rs-wineventlog")]
#[command(about = "Windows Event Log monitor and exporter", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(short, long)]
    pub config: Option<String>,

    #[arg(short, long)]
    pub pretty_json: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    ListChannels,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::ListChannels) => eventlog::list_channels()?,
        None => {
            let config = config::load(cli.config)?;
            let output = output::create(config.output_file.as_deref())?;
            eventlog::monitor(&config.channels, output, cli.pretty_json)?;
        }
    }

    Ok(())
}
