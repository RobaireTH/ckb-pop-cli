use anyhow::Result;

use crate::cli::{Cli, TxCommand};
use crate::commands::resolve_rpc;
use crate::config::Config;
use crate::rpc::RpcClient;

pub async fn run(cli: &Cli, cmd: &TxCommand) -> Result<()> {
	let config = Config::load()?;
	let rpc_url = resolve_rpc(cli, &config);
	let rpc = RpcClient::new(&rpc_url);

	match cmd {
		TxCommand::Status { tx_hash } => {
			let result = rpc.get_transaction(tx_hash)?;
			match result {
				Some(info) => {
					let status = info.tx_status.status;
					println!("Transaction: {tx_hash}");
					println!("Status:      {status:?}");
					if let Some(bh) = info.tx_status.block_hash {
						println!("Block:       {bh:#x}");
					}
				}
				None => println!("Transaction not found: {tx_hash}"),
			}
			Ok(())
		}
	}
}
