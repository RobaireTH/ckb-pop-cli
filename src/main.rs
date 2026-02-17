use anyhow::Result;
use clap::Parser;

mod cli;
mod commands;
mod config;
mod contracts;
mod crypto;
mod rpc;
mod signer;

use cli::{Cli, Command};

#[tokio::main]
async fn main() -> Result<()> {
	let cli = Cli::parse();

	match &cli.command {
		Command::Signer { command } => commands::signer::run(command).await,
		Command::Event { command } => commands::event::run(&cli, command).await,
		Command::Attend { qr_data: _ } => {
			anyhow::bail!("attend pipeline requires a signer — not yet implemented")
		}
		Command::Badge { command } => commands::badge::run(&cli, command).await,
		Command::Tx { command } => commands::tx::run(&cli, command).await,
	}
}
