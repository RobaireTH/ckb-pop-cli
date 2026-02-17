use anyhow::Result;
use sha2::{Digest, Sha256};

use crate::cli::{Cli, EventCommand};
use crate::config::Config;
use crate::contracts::CONTRACTS;
use crate::rpc::RpcClient;

pub async fn run(cli: &Cli, cmd: &EventCommand) -> Result<()> {
	let config = Config::load()?;
	let network = cli.network.as_str();
	let rpc_url = cli
		.rpc_url
		.clone()
		.unwrap_or_else(|| config.rpc_url(network).to_owned());
	let rpc = RpcClient::new(&rpc_url);
	let contracts = CONTRACTS.for_network(network);

	match cmd {
		EventCommand::Show { event_id } => {
			show_event(&rpc, contracts.event_anchor.code_hash, event_id).await
		}
		EventCommand::List { creator } => {
			list_events(&rpc, contracts.event_anchor.code_hash, creator.as_deref()).await
		}
		EventCommand::Create { .. } | EventCommand::Window { .. } => {
			anyhow::bail!("this command requires a signer — not yet implemented")
		}
	}
}

/// Display the on-chain anchor data for a single event.
async fn show_event(rpc: &RpcClient, anchor_code_hash: &str, event_id: &str) -> Result<()> {
	let cells = rpc.find_event_anchors(anchor_code_hash, event_id).await?;

	if cells.is_empty() {
		println!("No event anchor found for ID: {event_id}");
		return Ok(());
	}

	for cell in &cells {
		if let Some(json) = decode_cell_data(cell) {
			println!("{}", serde_json::to_string_pretty(&json)?);
		}
		if let Some(tx) = cell.pointer("/out_point/tx_hash").and_then(|v| v.as_str()) {
			println!("Anchor tx: {tx}");
		}
	}

	Ok(())
}

/// List all event anchors, optionally filtered by creator address.
async fn list_events(
	rpc: &RpcClient,
	anchor_code_hash: &str,
	creator: Option<&str>,
) -> Result<()> {
	let cells = rpc.find_all_event_anchors(anchor_code_hash).await?;

	let creator_hash = creator.map(|a| hex::encode(Sha256::digest(a.as_bytes())));

	let mut count = 0u32;
	for cell in &cells {
		// Optionally filter by creator hash (second 32 bytes of args).
		if let Some(ref ch) = creator_hash {
			let args = cell
				.pointer("/output/type/args")
				.and_then(|v| v.as_str())
				.unwrap_or("");
			let args = args.strip_prefix("0x").unwrap_or(args);
			if args.len() >= 128 && &args[64..128] != ch.as_str() {
				continue;
			}
		}

		count += 1;
		print!("#{count}");
		if let Some(json) = decode_cell_data(cell) {
			if let Some(id) = json.get("event_id").and_then(|v| v.as_str()) {
				print!("  id={id}");
			}
		}
		if let Some(tx) = cell.pointer("/out_point/tx_hash").and_then(|v| v.as_str()) {
			print!("  tx={tx}");
		}
		println!();
	}

	if count == 0 {
		println!("No events found.");
	} else {
		println!("\n{count} event(s) total.");
	}

	Ok(())
}

/// Try to decode the hex cell data as JSON.
fn decode_cell_data(cell: &serde_json::Value) -> Option<serde_json::Value> {
	let hex_data = cell.pointer("/output_data").and_then(|v| v.as_str())?;
	let raw = hex::decode(hex_data.strip_prefix("0x").unwrap_or(hex_data)).ok()?;
	serde_json::from_slice(&raw).ok()
}
