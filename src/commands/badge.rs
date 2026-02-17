use anyhow::Result;
use sha2::{Digest, Sha256};

use crate::cli::{BadgeCommand, Cli};
use crate::config::Config;
use crate::contracts::CONTRACTS;
use crate::crypto;
use crate::rpc::RpcClient;

pub async fn run(cli: &Cli, cmd: &BadgeCommand) -> Result<()> {
	let config = Config::load()?;
	let network = cli.network.as_str();
	let rpc_url = cli
		.rpc_url
		.clone()
		.unwrap_or_else(|| config.rpc_url(network).to_owned());
	let rpc = RpcClient::new(&rpc_url);
	let contracts = CONTRACTS.for_network(network);

	match cmd {
		BadgeCommand::Verify { event_id, address } => {
			verify_badge(&rpc, contracts.dob_badge.code_hash, event_id, address).await
		}
		BadgeCommand::List { address } => {
			list_badges(&rpc, contracts.dob_badge.code_hash, address).await
		}
		BadgeCommand::Mint { .. } => {
			anyhow::bail!("badge mint requires a signer — not yet implemented")
		}
	}
}

/// Check whether a specific (event, address) badge exists on-chain.
async fn verify_badge(
	rpc: &RpcClient,
	badge_code_hash: &str,
	event_id: &str,
	address: &str,
) -> Result<()> {
	let args = crypto::build_type_script_args(event_id, address);
	let args_hex = format!("0x{}", hex::encode(&args));

	let search_key = serde_json::json!({
		"script": {
			"code_hash": badge_code_hash,
			"hash_type": "type",
			"args": args_hex
		},
		"script_type": "type",
		"script_search_mode": "exact",
		"with_data": true
	});

	let page = rpc.get_cells(search_key, "asc", 1, None).await?;
	let cells = page
		.get("objects")
		.and_then(|v| v.as_array())
		.map(|a| a.len())
		.unwrap_or(0);

	if cells > 0 {
		let cell = &page["objects"][0];
		let tx = cell
			.pointer("/out_point/tx_hash")
			.and_then(|v| v.as_str())
			.unwrap_or("unknown");
		println!("Badge EXISTS for event {event_id}");
		println!("  Holder:  {address}");
		println!("  Mint tx: {tx}");
	} else {
		println!("No badge found for event {event_id}, address {address}.");
	}

	Ok(())
}

/// List all badges held by a given address.
///
/// Because the address hash occupies the *second* 32 bytes of the type
/// script args, we cannot use a prefix search to filter server-side.
/// Instead we fetch all badge cells and filter locally.
async fn list_badges(rpc: &RpcClient, badge_code_hash: &str, address: &str) -> Result<()> {
	let addr_hash = hex::encode(Sha256::digest(address.as_bytes()));
	let cells = rpc.find_all_badges(badge_code_hash).await?;

	let mut count = 0u32;
	for cell in &cells {
		let args = match cell.pointer("/output/type/args").and_then(|v| v.as_str()) {
			Some(a) => a.strip_prefix("0x").unwrap_or(a),
			None => continue,
		};
		// args is 128 hex chars (64 bytes): first half = event hash, second = address hash.
		if args.len() < 128 || &args[64..128] != addr_hash {
			continue;
		}

		count += 1;
		let event_hash = &args[..64];
		let tx = cell
			.pointer("/out_point/tx_hash")
			.and_then(|v| v.as_str())
			.unwrap_or("unknown");
		println!("#{count}  event_hash={event_hash}  tx={tx}");
	}

	if count == 0 {
		println!("No badges found for address {address}.");
	} else {
		println!("\n{count} badge(s) total.");
	}

	Ok(())
}
