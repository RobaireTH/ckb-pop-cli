use anyhow::Result;
use sha2::{Digest, Sha256};

use crate::cli::Cli;
use crate::commands::{resolve_rpc, resolve_signer};
use crate::config::Config;
use crate::contracts::CONTRACTS;
use crate::crypto::{self, QrPayload};
use crate::rpc::RpcClient;

/// Full attendance pipeline: parse QR -> verify freshness -> sign
/// attendance proof -> mint badge -> broadcast.
pub async fn run(cli: &Cli, qr_data: &str) -> Result<()> {
	let config = Config::load()?;
	let network = cli.network.as_str();
	let rpc_url = resolve_rpc(cli, &config);
	let rpc = RpcClient::new(&rpc_url);
	let contracts = CONTRACTS.for_network(network);

	// 1. Parse QR payload.
	let qr = QrPayload::parse(qr_data).ok_or_else(|| {
		anyhow::anyhow!("Invalid QR data. Expected format: event_id|timestamp|hmac")
	})?;
	println!("Event:  {}", qr.event_id);
	println!("QR ts:  {}", qr.timestamp);

	// 2. Check freshness (must be within 60 seconds).
	let now = chrono::Utc::now().timestamp();
	let age = now - qr.timestamp;
	if !(0..=60).contains(&age) {
		anyhow::bail!("QR code expired ({age}s old, maximum is 60s).");
	}

	// 3. Resolve signer and address.
	let signer = resolve_signer(cli, &config)?;
	let address = signer.address().to_owned();

	// 4. Sign the attendance proof message.
	let msg = crypto::attendance_message(&qr.event_id, qr.timestamp, &address);
	println!("Signing attendance proof...");
	let sig = signer.sign_message(&msg).await?;
	let proof_hash = hex::encode(Sha256::digest(sig.as_bytes()));

	// 5. Build the badge mint transaction.
	let recipient_addr: ckb_sdk::Address = address
		.parse()
		.map_err(|e| anyhow::anyhow!("invalid address: {e}"))?;
	let recipient_lock: ckb_types::packed::Script = (&recipient_addr).into();

	let tx = crate::tx_builder::build_badge_mint(
		&contracts.dob_badge,
		&qr.event_id,
		&address,
		recipient_lock,
		&address,
		Some(&proof_hash),
	)?;

	// 6. Sign and broadcast.
	println!("Signing badge transaction...");
	let signed = signer.sign_transaction(tx).await?;

	let json_tx = ckb_jsonrpc_types::TransactionView::from(signed);
	let tx_hash = rpc.send_transaction(json_tx.inner)?;

	println!("Attendance recorded and badge minted!");
	println!("  TX: {tx_hash:#x}");

	Ok(())
}
