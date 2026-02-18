//! Integration tests that hit the CKB testnet RPC.
//!
//! These are marked `#[ignore]` by default because they require network
//! access. Run them explicitly with:
//!
//!   cargo test --test integration -- --ignored

use ckb_pop_cli::contracts::CONTRACTS;
use ckb_pop_cli::rpc::RpcClient;
use ckb_pop_cli::signer::Signer as _;
use sha2::{Digest, Sha256};

const TESTNET_RPC: &str = "https://testnet.ckb.dev/rpc";

#[test]
#[ignore]
fn tip_block_number_is_positive() {
	let rpc = RpcClient::new(TESTNET_RPC);
	let tip = rpc.get_tip_block_number().expect("failed to fetch tip");
	assert!(tip > 0, "tip block number should be positive, got {tip}");
}

#[test]
#[ignore]
fn contract_deploy_tx_exists() {
	let rpc = RpcClient::new(TESTNET_RPC);
	let contracts = CONTRACTS.for_network("testnet");

	let result = rpc
		.get_transaction(contracts.dob_badge.deploy_tx_hash)
		.expect("RPC call failed");

	assert!(
		result.is_some(),
		"deploy tx {} should exist on testnet",
		contracts.dob_badge.deploy_tx_hash
	);
}

#[tokio::test]
#[ignore]
async fn indexer_get_cells_returns_valid_response() {
	let rpc = RpcClient::new(TESTNET_RPC);
	let contracts = CONTRACTS.for_network("testnet");

	// Search for any badge cells (empty prefix = match all).
	let search_key = serde_json::json!({
		"script": {
			"code_hash": contracts.dob_badge.code_hash,
			"hash_type": "type",
			"args": "0x"
		},
		"script_type": "type",
		"script_search_mode": "prefix",
		"with_data": true
	});

	let page = rpc
		.get_cells(search_key, "asc", 1, None)
		.await
		.expect("get_cells failed");

	// The response should have an "objects" array, even if empty.
	assert!(
		page.get("objects").is_some(),
		"response should contain 'objects' field"
	);
}

#[tokio::test]
#[ignore]
async fn find_all_event_anchors_does_not_error() {
	let rpc = RpcClient::new(TESTNET_RPC);
	let contracts = CONTRACTS.for_network("testnet");

	// This should not panic or return an RPC error, even if no
	// events have been created yet.
	let cells = rpc
		.find_all_event_anchors(contracts.event_anchor.code_hash)
		.await
		.expect("find_all_event_anchors failed");

	// cells may be empty on a fresh testnet, that's fine.
	println!("found {} event anchor(s)", cells.len());
}

/// Full proof-of-presence flow: event creation → attendance window → badge mint.
///
/// Requires `~/.ckb-pop/config.toml` with `address` and `method = "browser"` set.
/// Each of the four signing steps opens a browser tab for wallet approval.
///
/// Run with:
///   cargo test --test integration -- event_creation_and_badge_mint_e2e --ignored --nocapture
#[tokio::test]
#[ignore]
async fn event_creation_and_badge_mint_e2e() {
	let config = ckb_pop_cli::config::Config::load()
		.expect("failed to load config from ~/.ckb-pop/config.toml");
	let address = config
		.signer
		.address
		.as_deref()
		.expect("config must have [signer] address set")
		.to_owned();
	let network = config.network.default.clone();
	let rpc_url = config.rpc_url(&network).to_owned();
	let rpc = RpcClient::new(&rpc_url);
	let contracts = CONTRACTS.for_network(&network);
	let signer =
		ckb_pop_cli::signer::browser::BrowserSigner::new(address.clone(), network.clone());

	// -- Step 1: Create the event anchor --

	let timestamp = chrono::Utc::now().timestamp();
	let nonce = hex::encode(rand::random::<[u8; 16]>());
	let event_id = ckb_pop_cli::crypto::compute_event_id(&address, timestamp, &nonce);
	println!("Event ID: {event_id}");

	let ckb_addr: ckb_sdk::Address = address.parse().expect("invalid CKB address in config");
	let creator_lock: ckb_types::packed::Script = (&ckb_addr).into();

	let anchor_tx = ckb_pop_cli::tx_builder::build_event_anchor(
		&contracts.event_anchor,
		&event_id,
		&address,
		creator_lock.clone(),
		None,
	)
	.expect("failed to build event anchor tx");

	println!("Signing event anchor transaction (browser 1/4)...");
	let signed_anchor = signer
		.sign_transaction(anchor_tx)
		.await
		.expect("failed to sign anchor tx");

	let json_anchor_tx = ckb_jsonrpc_types::TransactionView::from(signed_anchor);
	let anchor_tx_hash = rpc
		.send_transaction(json_anchor_tx.inner)
		.expect("failed to send anchor tx");
	let anchor_hash_str = format!("{anchor_tx_hash:#x}");
	println!("Anchor TX:  {anchor_hash_str}");
	println!("Explorer:   https://pudge.explorer.nervos.org/transaction/{anchor_hash_str}");

	let anchor_status = rpc
		.get_transaction(&anchor_hash_str)
		.expect("get_transaction RPC failed");
	assert!(anchor_status.is_some(), "anchor tx should be accepted into the mempool");

	// -- Step 2: Open the attendance window --

	let window_start = chrono::Utc::now().timestamp();
	let window_msg = ckb_pop_cli::crypto::window_message(&event_id, window_start, None);
	println!("Signing window message (browser 2/4)...");
	let creator_sig = signer
		.sign_message(&window_msg)
		.await
		.expect("failed to sign window message");

	let window_secret =
		ckb_pop_cli::crypto::derive_window_secret(&event_id, window_start, &creator_sig);

	// Align the QR timestamp to a 30-second interval, matching the CLI convention.
	let now = chrono::Utc::now().timestamp();
	let qr_ts = now - (now % 30);
	let qr_hmac = ckb_pop_cli::crypto::generate_qr_hmac(&window_secret, qr_ts);
	let qr_data = format!("{event_id}|{qr_ts}|{qr_hmac}");
	println!("QR payload: {qr_data}");

	// -- Step 3: Prove attendance --

	let attend_msg = ckb_pop_cli::crypto::attendance_message(&event_id, qr_ts, &address);
	println!("Signing attendance message (browser 3/4)...");
	let attend_sig = signer
		.sign_message(&attend_msg)
		.await
		.expect("failed to sign attendance message");
	let proof_hash = hex::encode(Sha256::digest(attend_sig.as_bytes()));

	// -- Step 4: Mint the badge --

	let badge_tx = ckb_pop_cli::tx_builder::build_badge_mint(
		&contracts.dob_badge,
		&event_id,
		&address,
		creator_lock,
		&address,
		Some(&proof_hash),
	)
	.expect("failed to build badge mint tx");

	println!("Signing badge mint transaction (browser 4/4)...");
	let signed_badge = signer
		.sign_transaction(badge_tx)
		.await
		.expect("failed to sign badge mint tx");

	let json_badge_tx = ckb_jsonrpc_types::TransactionView::from(signed_badge);
	let badge_tx_hash = rpc
		.send_transaction(json_badge_tx.inner)
		.expect("failed to send badge mint tx");
	let badge_hash_str = format!("{badge_tx_hash:#x}");
	println!("Badge TX:   {badge_hash_str}");
	println!("Explorer:   https://pudge.explorer.nervos.org/transaction/{badge_hash_str}");

	let badge_status = rpc
		.get_transaction(&badge_hash_str)
		.expect("get_transaction RPC failed");
	assert!(badge_status.is_some(), "badge tx should be accepted into the mempool");

	// -- Poll the indexer until the badge cell is visible (up to 90 seconds) --

	let mut found = false;
	for attempt in 1..=18u32 {
		println!("Polling indexer for badge (attempt {attempt}/18)...");
		let badges = rpc
			.find_badges_for_event(contracts.dob_badge.code_hash, &event_id)
			.await
			.expect("find_badges_for_event failed");
		if !badges.is_empty() {
			println!("Badge found in indexer after {}s.", attempt * 5);
			found = true;
			break;
		}
		tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
	}

	assert!(found, "badge was not indexed within 90 seconds");
}
