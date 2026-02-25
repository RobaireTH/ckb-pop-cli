use std::io::Write as _;

use anyhow::Result;
use sha2::{Digest, Sha256};

use crate::cli::{Cli, EventCommand};
use crate::commands::{resolve_rpc, resolve_signer};
use crate::config::Config;
use crate::contracts::CONTRACTS;
use crate::crypto;
use crate::rpc::RpcClient;

/// Backend URL for the ckb-pop.xyz event registry.
const BACKEND_URL: &str = "https://ckb-pop-backend.fly.dev/api";

/// Public URL for viewing and managing events.
const FRONTEND_URL: &str = "https://ckb-pop.xyz";

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

	// Show the creator address up front so users can verify it matches
	// the wallet they will connect on ckb-pop.xyz.
	println!("Creator address: {address}");
	println!("Tip: connect this same address on ckb-pop.xyz to see this event in My Events.");
	println!();

	// Step 1: Sign the event-creation proof.
	// The backend and website both use this message format to authenticate
	// the creator before assigning a canonical event ID.
	let nonce = gen_uuid_v4();
	let create_msg = format!("CKB-PoP-CreateEvent|{nonce}");
	println!("Signing event creation proof...");
	let creator_sig = signer.sign_message(&create_msg).await?;

	// Step 2: Register with the backend to get the canonical event ID.
	let metadata_body = serde_json::json!({
		"name": name,
		"description": description,
		"image_url": image_url,
		"location": location,
		"start_time": start,
		"end_time": end,
	});
	let body = serde_json::json!({
		"creator_address": address,
		"creator_signature": creator_sig,
		"nonce": nonce,
		"metadata": metadata_body,
	});

	let http = reqwest::Client::new();
	let resp = http
		.post(format!("{BACKEND_URL}/events/create"))
		.json(&body)
		.send()
		.await?;

	if !resp.status().is_success() {
		let err: serde_json::Value = resp.json().await.unwrap_or_default();
		anyhow::bail!(
			"backend rejected event creation: {}",
			err.get("error").and_then(|v| v.as_str()).unwrap_or("unknown error")
		);
	}

	let result: serde_json::Value = resp.json().await?;
	let event_id = result["event_id"]
		.as_str()
		.ok_or_else(|| anyhow::anyhow!("backend did not return event_id"))?
		.to_owned();

	// Step 3: Hash the metadata for the on-chain anchor cell.
	// Use the same 4-key JSON structure the website uses so the hash matches.
	let meta_for_hash = serde_json::json!({
		"name": name,
		"date": start,
		"location": location,
		"description": description,
	});
	let metadata_hash = hex::encode(Sha256::digest(
		serde_json::to_string(&meta_for_hash)?.as_bytes(),
	));

	// Step 4: Parse the creator's lock script from their address.
	let ckb_addr: ckb_sdk::Address = address
		.parse()
		.map_err(|e| anyhow::anyhow!("invalid CKB address: {e}"))?;
	let creator_lock: ckb_types::packed::Script = (&ckb_addr).into();

	// Step 5: Build and sign the on-chain anchor transaction.
	let tx = crate::tx_builder::build_event_anchor(
		&contracts.event_anchor,
		&event_id,
		&address,
		creator_lock,
		Some(&metadata_hash),
	)?;

	println!("Signing transaction...");
	let signed = signer.sign_transaction(tx).await?;

	let json_tx = ckb_jsonrpc_types::TransactionView::from(signed);
	let tx_hash = rpc.send_transaction(json_tx.inner)?;
	let tx_hash_str = format!("{tx_hash:#x}");

	println!("Event ID:  {event_id}");
	println!("Anchor TX: {tx_hash_str}");
	println!("View at:   {FRONTEND_URL}/events/{event_id}");
	println!();

	// Step 6: Wait for the anchor TX to be committed on-chain, then tell
	// the backend so it records the tx hash and shows the event as fully
	// activated.  The event is already live in the backend registry; this
	// step just adds on-chain proof.
	if await_tx_confirmation(&http, &tx_hash_str).await {
		activate_event_on_backend(&http, &event_id, &tx_hash_str).await;
	} else {
		println!("The event is live in the backend.  Run this command once the");
		println!("TX is committed to store the anchor proof:");
		println!("  curl -s -X POST {BACKEND_URL}/events/{event_id}/activate \\");
		println!("       -H 'Content-Type: application/json' \\");
		println!("       -d '{{\"tx_hash\":\"{tx_hash_str}\"}}'");
	}

	Ok(())
}

/// Poll the backend tx-status endpoint until the anchor TX is committed
/// on-chain.  Returns true on confirmation, false after ~90 s timeout.
async fn await_tx_confirmation(http: &reqwest::Client, tx_hash: &str) -> bool {
	print!("Waiting for anchor TX confirmation");
	let _ = std::io::stdout().flush();

	// Six attempts at 15-second intervals = 90 s total.
	for i in 0..6u8 {
		tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;
		let url = format!("{BACKEND_URL}/tx/{tx_hash}");
		if let Ok(resp) = http.get(&url).send().await {
			if let Ok(data) = resp.json::<serde_json::Value>().await {
				if data["confirmed"].as_bool().unwrap_or(false) {
					println!(" confirmed.");
					return true;
				}
			}
		}
		if i < 5 {
			print!(".");
			let _ = std::io::stdout().flush();
		}
	}
	println!();
	false
}

/// POST the anchor TX hash to the backend activate endpoint so it records
/// on-chain proof.  This is idempotent and non-fatal if it fails.
async fn activate_event_on_backend(http: &reqwest::Client, event_id: &str, tx_hash: &str) {
	let body = serde_json::json!({ "tx_hash": tx_hash });
	match http
		.post(format!("{BACKEND_URL}/events/{event_id}/activate"))
		.json(&body)
		.send()
		.await
	{
		Ok(resp) if resp.status().is_success() => {
			println!("Backend record updated with anchor TX hash.");
		}
		Ok(resp) => {
			let err: serde_json::Value = resp.json().await.unwrap_or_default();
			println!(
				"Note: backend activation returned an error: {}",
				err.get("error").and_then(|v| v.as_str()).unwrap_or("unknown")
			);
		}
		Err(e) => {
			println!("Note: could not reach backend to record anchor TX: {e}");
		}
	}
}

/// Generate a random UUID v4 string without pulling in a uuid crate.
fn gen_uuid_v4() -> String {
	let b: [u8; 16] = rand::random();
	format!(
		"{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
		b[0], b[1], b[2], b[3],
		b[4], b[5],
		(b[6] & 0x0f) | 0x40, b[7],   // version 4
		(b[8] & 0x3f) | 0x80, b[9],   // variant 1
		b[10], b[11], b[12], b[13], b[14], b[15],
	)
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
	let creator_hash = creator.map(|a| hex::encode(&Sha256::digest(a.as_bytes())[..20]));

	let mut count = 0u32;
	for cell in &cells {
		if let Some(ref ch) = creator_hash {
			let args = cell
				.pointer("/output/type/args")
				.and_then(|v| v.as_str())
				.unwrap_or("");
			let args = args.strip_prefix("0x").unwrap_or(args);
			if args.len() >= 80 && args[40..80] != *ch {
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
