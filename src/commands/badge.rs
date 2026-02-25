use anyhow::Result;
use sha2::{Digest, Sha256};

use crate::cli::{BadgeCommand, Cli};
use crate::commands::{resolve_rpc, resolve_signer};
use crate::config::Config;
use crate::contracts::CONTRACTS;
use crate::crypto;
use crate::rpc::RpcClient;

pub async fn run(cli: &Cli, cmd: &BadgeCommand) -> Result<()> {
	let config = Config::load()?;
	let network = cli.network.as_str();
	let rpc_url = resolve_rpc(cli, &config);
	let rpc = RpcClient::new(&rpc_url);
	let contracts = CONTRACTS.for_network(network);

	match cmd {
		BadgeCommand::Verify { event_id, address } => {
			verify_badge(&rpc, contracts.dob_badge.code_hash, event_id, address).await
		}
		BadgeCommand::List { address } => {
			list_badges(&rpc, contracts.dob_badge.code_hash, address).await
		}
		BadgeCommand::Mint { event_id, to } => {
			mint_badge(cli, &config, &rpc, network, event_id, to).await
		}
	}
}

async fn mint_badge(
	cli: &Cli,
	config: &Config,
	rpc: &RpcClient,
	network: &str,
	event_id: &str,
	to: &str,
) -> Result<()> {
	let signer = resolve_signer(cli, config)?;
	let issuer = signer.address().to_owned();
	let contracts = CONTRACTS.for_network(network);

	let recipient_addr: ckb_sdk::Address = to
		.parse()
		.map_err(|e| anyhow::anyhow!("invalid recipient address: {e}"))?;
	let recipient_lock: ckb_types::packed::Script = (&recipient_addr).into();

	let tx = crate::tx_builder::build_badge_mint(
		&contracts.dob_badge,
		event_id,
		to,
		recipient_lock,
		&issuer,
		None,
	)?;

	println!("Signing badge transaction...");
	let signed = signer.sign_transaction(tx).await?;

	let json_tx = ckb_jsonrpc_types::TransactionView::from(signed);
	let tx_hash = rpc.send_transaction(json_tx.inner)?;
	println!("Badge minted for event {event_id}.");
	println!("  Recipient: {to}");
	println!("  TX: {tx_hash:#x}");

	Ok(())
}

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

async fn list_badges(rpc: &RpcClient, badge_code_hash: &str, address: &str) -> Result<()> {
	let addr_hash = hex::encode(&Sha256::digest(address.as_bytes())[..20]);
	let cells = rpc.find_all_badges(badge_code_hash).await?;

	let mut count = 0u32;
	for cell in &cells {
		let args = match cell.pointer("/output/type/args").and_then(|v| v.as_str()) {
			Some(a) => a.strip_prefix("0x").unwrap_or(a),
			None => continue,
		};
		if args.len() < 80 || args[40..80] != addr_hash {
			continue;
		}

		count += 1;
		let event_hash = &args[..40];
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
