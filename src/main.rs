use anyhow::Result;
use clap::Parser;

mod cli;
mod commands;
mod config;
mod contracts;
mod crypto;
mod rpc;
mod signer;

#[tokio::main]
async fn main() -> Result<()> {
	let _cli = cli::Cli::parse();
	Ok(())
}
