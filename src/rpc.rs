use anyhow::{anyhow, Result};
use ckb_jsonrpc_types as json;
use ckb_sdk::rpc::CkbRpcClient;
use serde_json::{json as json_val, Value};
use sha2::{Digest, Sha256};

/// Thin wrapper around the CKB RPC node and its built-in indexer.
///
/// Most queries go through the ckb-sdk client.  For the indexer's
/// `get_cells` call we use raw JSON-RPC because ckb-sdk does not
/// expose the `script_search_mode: "prefix"` parameter that is
/// essential for searching by partial type-script args.
pub struct RpcClient {
	sdk: CkbRpcClient,
	url: String,
	http: reqwest::Client,
}

impl RpcClient {
	pub fn new(url: &str) -> Self {
		Self {
			sdk: CkbRpcClient::new(url),
			url: url.to_owned(),
			http: reqwest::Client::new(),
		}
	}

	/// Access the underlying ckb-sdk client for operations that it
	/// handles well (sending transactions, fetching blocks, etc.).
	pub fn sdk(&self) -> &CkbRpcClient {
		&self.sdk
	}

	// -- Standard RPC helpers --

	pub fn get_tip_block_number(&self) -> Result<u64> {
		Ok(self.sdk.get_tip_block_number()?.into())
	}

	pub fn get_transaction(
		&self,
		tx_hash: &str,
	) -> Result<Option<json::TransactionWithStatusResponse>> {
		let h256 = parse_h256(tx_hash)?;
		Ok(self.sdk.get_transaction(h256)?)
	}

	pub fn send_transaction(&self, tx: json::Transaction) -> Result<ckb_types::H256> {
		let hash = self
			.sdk
			.send_transaction(tx, Some(json::OutputsValidator::Passthrough))?;
		Ok(hash)
	}

	// -- Indexer queries with prefix support --

	/// Run a single paginated `get_cells` call against the indexer.
	pub async fn get_cells(
		&self,
		search_key: Value,
		order: &str,
		limit: u64,
		after_cursor: Option<&str>,
	) -> Result<Value> {
		let cursor = after_cursor
			.map(|s| Value::String(s.to_owned()))
			.unwrap_or(Value::Null);

		let body = json_val!({
			"id": 1,
			"jsonrpc": "2.0",
			"method": "get_cells",
			"params": [search_key, order, format!("0x{limit:x}"), cursor]
		});

		let resp: Value = self.http.post(&self.url).json(&body).send().await?.json().await?;

		resp.get("result").cloned().ok_or_else(|| {
			let err = resp.get("error").cloned().unwrap_or(Value::Null);
			anyhow!("get_cells RPC error: {err}")
		})
	}

	/// Collect all pages from a `get_cells` query into a single vec.
	pub async fn get_all_cells(&self, search_key: Value) -> Result<Vec<Value>> {
		let mut all = Vec::new();
		let mut cursor: Option<String> = None;

		loop {
			let page = self
				.get_cells(search_key.clone(), "asc", 100, cursor.as_deref())
				.await?;

			let objects = page
				.get("objects")
				.and_then(Value::as_array)
				.cloned()
				.unwrap_or_default();

			if objects.is_empty() {
				break;
			}

			all.extend(objects);

			cursor = page
				.get("last_cursor")
				.and_then(Value::as_str)
				.map(str::to_owned);

			if cursor.is_none() {
				break;
			}
		}

		Ok(all)
	}

	// -- PoP-specific search helpers --

	/// Find all badge cells minted for a given event (prefix match on
	/// the first 32 bytes of type-script args = SHA256(event_id)).
	pub async fn find_badges_for_event(
		&self,
		badge_code_hash: &str,
		event_id: &str,
	) -> Result<Vec<Value>> {
		let event_hash = hex::encode(Sha256::digest(event_id.as_bytes()));
		self.get_all_cells(type_prefix_search(badge_code_hash, &event_hash))
			.await
	}

	/// Find all badge cells across all events (empty prefix on args).
	pub async fn find_all_badges(&self, badge_code_hash: &str) -> Result<Vec<Value>> {
		self.get_all_cells(type_prefix_search(badge_code_hash, ""))
			.await
	}

	/// Find event-anchor cells for a given event ID.
	pub async fn find_event_anchors(
		&self,
		anchor_code_hash: &str,
		event_id: &str,
	) -> Result<Vec<Value>> {
		let event_hash = hex::encode(Sha256::digest(event_id.as_bytes()));
		self.get_all_cells(type_prefix_search(anchor_code_hash, &event_hash))
			.await
	}

	/// Find all event-anchor cells (every event).
	pub async fn find_all_event_anchors(&self, anchor_code_hash: &str) -> Result<Vec<Value>> {
		self.get_all_cells(type_prefix_search(anchor_code_hash, ""))
			.await
	}
}

// -- Private helpers --

/// Build a search key that matches cells whose type script has the given
/// code hash and whose args start with `hex_prefix`.
fn type_prefix_search(code_hash: &str, hex_prefix: &str) -> Value {
	json_val!({
		"script": {
			"code_hash": code_hash,
			"hash_type": "type",
			"args": format!("0x{hex_prefix}")
		},
		"script_type": "type",
		"script_search_mode": "prefix",
		"with_data": true
	})
}

fn parse_h256(hex_str: &str) -> Result<ckb_types::H256> {
	let clean = hex_str.strip_prefix("0x").unwrap_or(hex_str);
	clean
		.parse()
		.map_err(|e| anyhow!("invalid 256-bit hash: {e}"))
}
