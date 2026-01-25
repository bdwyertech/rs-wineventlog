#![cfg(windows)]

mod config;
mod eventlog;
mod output;
mod privilege;
mod xml;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{Shell, generate};
use std::io;

pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

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

    #[arg(short, long)]
    version: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(about = "List available Windows Event Log channels")]
    ListChannels,

    #[command(about = "Generate shell completions")]
    Completions {
        #[arg(help = "Shell to generate completions for")]
        shell: Shell,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Initialize logger
    if atty::is(atty::Stream::Stderr) {
        // Human-readable format for interactive use (TTY)
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .format_timestamp_millis()
            .init();
    } else {
        // JSON format (Not TTY)
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .format(|buf, record| {
                use std::io::Write;
                writeln!(
                    buf,
                    r#"{{"timestamp":"{}","level":"{}","message":"{}"}}"#,
                    chrono::Utc::now().to_rfc3339(),
                    record.level(),
                    record.args()
                )
            })
            .init();
    }

    if cli.version {
        let git_commit = built_info::GIT_COMMIT_HASH_SHORT;
        let release_ver = option_env!("BUILD_VERSION").unwrap_or(built_info::PKG_VERSION);
        let release_date = built_info::BUILT_TIME_UTC;

        println!("rs-wineventlog");
        println!("Version: {}", release_ver);
        println!("Git Commit: {}", git_commit.unwrap_or("unknown"));
        println!("Release Date: {}", release_date);
        return Ok(());
    }

    match cli.command {
        Some(Commands::Completions { shell }) => {
            let mut cmd = Cli::command();
            generate(shell, &mut cmd, "rs-wineventlog", &mut io::stdout());
        }
        Some(Commands::ListChannels) => eventlog::list_channels()?,
        None => {
            let config = config::load(cli.config)?;
            let output = output::create(config.output_file.as_deref())?;
            eventlog::monitor(&config.channels, output, cli.pretty_json, config.batch_size)?;
        }
    }

    Ok(())
}
