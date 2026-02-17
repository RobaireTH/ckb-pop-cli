use anyhow::Result;
use sha2::{Digest, Sha256};

use crate::cli::{Cli, EventCommand};
use crate::commands::{resolve_rpc, resolve_signer};
use crate::config::Config;
use crate::contracts::CONTRACTS;
use crate::crypto;
use crate::rpc::RpcClient;

pub async fn run(cli: &Cli, cmd: &EventCommand) -> Result<()> {
	let config = Config::load()?;
	let network = cli.network.as_str();
	let rpc_url = resolve_rpc(cli, &config);
	let rpc = RpcClient::new(&rpc_url);
	let contracts = CONTRACTS.for_network(network);

	match cmd {
		EventCommand::Show { event_id } => {
			show_event(&rpc, contracts.event_anchor.code_hash, event_id).await
		}
		EventCommand::List { creator } => {
			list_events(&rpc, contracts.event_anchor.code_hash, creator.as_deref()).await
		}
		EventCommand::Create {
			name,
			description,
			image_url,
			location,
			start,
			end,
		} => {
			create_event(
				cli, &config, &rpc, network, name, description,
				image_url.as_deref(),
				location.as_deref(),
				start.as_deref(),
				end.as_deref(),
			)
			.await
		}
		EventCommand::Window {
			event_id,
			duration,
		} => open_window(cli, &config, event_id, *duration).await,
	}
}

#[allow(clippy::too_many_arguments)]
async fn create_event(
	cli: &Cli,
	config: &Config,
	rpc: &RpcClient,
	network: &str,
	name: &str,
	description: &str,
	image_url: Option<&str>,
	location: Option<&str>,
	start: Option<&str>,
	end: Option<&str>,
) -> Result<()> {
	let signer = resolve_signer(cli, config)?;
	let address = signer.address().to_owned();
	let contracts = CONTRACTS.for_network(network);

	// Generate deterministic event ID.
	let timestamp = chrono::Utc::now().timestamp();
	let nonce = hex::encode(rand::random::<[u8; 16]>());
	let event_id = crypto::compute_event_id(&address, timestamp, &nonce);

	// Hash the metadata for the anchor cell.
	let metadata = serde_json::json!({
		"name": name,
		"description": description,
		"image_url": image_url,
		"location": location,
		"start_time": start,
		"end_time": end,
	});
	let metadata_hash = hex::encode(Sha256::digest(
		serde_json::to_string(&metadata)?.as_bytes(),
	));

	// Parse the creator's lock script from their address.
	let ckb_addr: ckb_sdk::Address = address
		.parse()
		.map_err(|e| anyhow::anyhow!("invalid CKB address: {e}"))?;
	let creator_lock: ckb_types::packed::Script = (&ckb_addr).into();

	// Build the unsigned transaction.
	let tx = crate::tx_builder::build_event_anchor(
		&contracts.event_anchor,
		&event_id,
		&address,
		creator_lock,
		Some(&metadata_hash),
	)?;

	println!("Event ID: {event_id}");
	println!("Signing transaction...");

	let signed = signer.sign_transaction(tx).await?;

	let json_tx = ckb_jsonrpc_types::TransactionView::from(signed);
	let tx_hash = rpc.send_transaction(json_tx.inner)?;
	println!("Event anchored on-chain.");
	println!("TX: {tx_hash:#x}");

	Ok(())
}

/// Open an attendance window: sign the window message, then display
/// rotating QR codes in the terminal until the window expires or the
/// user interrupts with Ctrl-C.
async fn open_window(
	cli: &Cli,
	config: &Config,
	event_id: &str,
	duration_minutes: u64,
) -> Result<()> {
	let signer = resolve_signer(cli, config)?;
	let window_start = chrono::Utc::now().timestamp();
	let window_end = if duration_minutes > 0 {
		Some(window_start + (duration_minutes as i64) * 60)
	} else {
		None
	};

	let msg = crypto::window_message(event_id, window_start, window_end);
	println!("Signing window proof...");
	let creator_sig = signer.sign_message(&msg).await?;

	let window_secret = crypto::derive_window_secret(event_id, window_start, &creator_sig);

	println!("Attendance window open!");
	if let Some(end) = window_end {
		let mins = (end - window_start) / 60;
		println!("Duration: {mins} minutes.");
	} else {
		println!("Duration: open-ended (Ctrl-C to close).");
	}
	println!();

	loop {
		let now = chrono::Utc::now().timestamp();
		if let Some(end) = window_end {
			if now >= end {
				println!("Window expired.");
				break;
			}
		}

		// Align to 30-second intervals.
		let qr_ts = now - (now % 30);
		let hmac = crypto::generate_qr_hmac(&window_secret, qr_ts);
		let qr_data = format!("{event_id}|{qr_ts}|{hmac}");

		// Clear screen and render QR.
		print!("\x1B[2J\x1B[H");
		let code = qrcode::QrCode::new(&qr_data)?;
		let rendered = code
			.render::<char>()
			.quiet_zone(false)
			.module_dimensions(2, 1)
			.build();
		println!("{rendered}");
		println!();
		println!("QR data: {qr_data}");
		println!("Refreshes in {}s...", 30 - (now % 30));

		tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
	}

	Ok(())
}

// -- Read-only helpers (unchanged) --

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

async fn list_events(
	rpc: &RpcClient,
	anchor_code_hash: &str,
	creator: Option<&str>,
) -> Result<()> {
	let cells = rpc.find_all_event_anchors(anchor_code_hash).await?;
	let creator_hash = creator.map(|a| hex::encode(Sha256::digest(a.as_bytes())));

	let mut count = 0u32;
	for cell in &cells {
		if let Some(ref ch) = creator_hash {
			let args = cell
				.pointer("/output/type/args")
				.and_then(|v| v.as_str())
				.unwrap_or("");
			let args = args.strip_prefix("0x").unwrap_or(args);
			if args.len() >= 128 && args[64..128] != *ch {
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

fn decode_cell_data(cell: &serde_json::Value) -> Option<serde_json::Value> {
	let hex_data = cell.pointer("/output_data").and_then(|v| v.as_str())?;
	let raw = hex::decode(hex_data.strip_prefix("0x").unwrap_or(hex_data)).ok()?;
	serde_json::from_slice(&raw).ok()
}
