//! Integration tests that hit the CKB testnet RPC.
//!
//! These are marked `#[ignore]` by default because they require network
//! access. Run them explicitly with:
//!
//!   cargo test --test integration -- --ignored

use ckb_pop_cli::contracts::CONTRACTS;
use ckb_pop_cli::rpc::RpcClient;

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
