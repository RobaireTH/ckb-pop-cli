use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

// -- Event IDs --

/// Compute a deterministic event ID from the creator's address, a unix
/// timestamp, and a random nonce.  Result is a 64-character hex string.
pub fn compute_event_id(creator_address: &str, timestamp_secs: i64, nonce: &str) -> String {
	let mut h = Sha256::new();
	h.update(creator_address.as_bytes());
	h.update(timestamp_secs.to_le_bytes());
	h.update(nonce.as_bytes());
	hex::encode(h.finalize())
}

// -- Type-script argument helpers --

/// Build the 64-byte args used by both dob-badge and event-anchor type
/// scripts: `SHA256(primary) || SHA256(secondary)`.
pub fn build_type_script_args(primary: &str, secondary: &str) -> Vec<u8> {
	let mut out = Vec::with_capacity(64);
	out.extend_from_slice(&sha256(primary.as_bytes()));
	out.extend_from_slice(&sha256(secondary.as_bytes()));
	out
}

// -- QR payload --

/// The three-field payload encoded in every attendance QR code.
#[derive(Debug, Clone, PartialEq)]
pub struct QrPayload {
	pub event_id: String,
	pub timestamp: i64,
	pub hmac: String,
}

impl QrPayload {
	/// Parse the pipe-delimited QR string: `event_id|timestamp|hmac`.
	pub fn parse(data: &str) -> Option<Self> {
		let mut parts = data.splitn(3, '|');
		let event_id = parts.next()?.to_owned();
		let timestamp: i64 = parts.next()?.parse().ok()?;
		let hmac = parts.next()?.to_owned();
		if event_id.is_empty() || hmac.is_empty() {
			return None;
		}
		Some(Self { event_id, timestamp, hmac })
	}

	/// Encode back to the pipe-delimited format.
	#[allow(dead_code)]
	pub fn encode(&self) -> String {
		format!("{}|{}|{}", self.event_id, self.timestamp, self.hmac)
	}
}

// -- Attendance window secrets --

/// Derive the shared secret for a window from the event ID, window start
/// timestamp, and the creator's signature over the window message.
pub fn derive_window_secret(event_id: &str, window_start: i64, creator_sig: &str) -> [u8; 32] {
	let mut h = Sha256::new();
	h.update(event_id.as_bytes());
	h.update(window_start.to_le_bytes());
	h.update(creator_sig.as_bytes());
	h.finalize().into()
}

/// Produce the 16-hex-character HMAC that goes into each rotating QR code.
pub fn generate_qr_hmac(window_secret: &[u8; 32], timestamp: i64) -> String {
	let mut mac =
		HmacSha256::new_from_slice(window_secret).expect("HMAC-SHA256 accepts any key length");
	mac.update(&timestamp.to_le_bytes());
	let full = hex::encode(mac.finalize().into_bytes());
	full[..16].to_string()
}

/// Verify a QR HMAC against the window secret and timestamp.
#[allow(dead_code)]
pub fn verify_qr_hmac(window_secret: &[u8; 32], timestamp: i64, expected: &str) -> bool {
	generate_qr_hmac(window_secret, timestamp) == expected
}

// -- Cell data builders --

/// Build the 34-byte binary cell data for a dob-badge output:
/// `[version: u8 | flags: u8 | content_hash: 32 bytes]`.
pub fn build_badge_cell_data(event_id: &str, issuer: &str, proof_hash: Option<&str>) -> Vec<u8> {
	let content = match proof_hash {
		Some(ph) => serde_json::json!({
			"protocol": "ckb-pop",
			"version": 1,
			"event_id": event_id,
			"issuer": issuer,
			"proof_hash": ph,
		}),
		None => serde_json::json!({
			"protocol": "ckb-pop",
			"version": 1,
			"event_id": event_id,
			"issuer": issuer,
		}),
	};
	let content_hash = sha256(serde_json::to_string(&content).unwrap().as_bytes());

	let mut data = Vec::with_capacity(34);
	data.push(0x01); // version
	data.push(0x01); // flags: has_metadata
	data.extend_from_slice(&content_hash);
	data
}

/// Build JSON cell data for an event-anchor output.
pub fn build_anchor_cell_data(
	event_id: &str,
	creator_address: &str,
	metadata_hash: Option<&str>,
) -> Vec<u8> {
	let obj = match metadata_hash {
		Some(mh) => serde_json::json!({
			"event_id": event_id,
			"creator_address": creator_address,
			"metadata_hash": mh,
		}),
		None => serde_json::json!({
			"event_id": event_id,
			"creator_address": creator_address,
		}),
	};
	serde_json::to_vec(&obj).unwrap()
}

// -- Signed message formats --

/// The message an attendee signs to prove they scanned the QR.
pub fn attendance_message(event_id: &str, qr_timestamp: i64, attendee_address: &str) -> String {
	format!("CKB-PoP|{event_id}|{qr_timestamp}|{attendee_address}")
}

/// The message an event creator signs to open an attendance window.
pub fn window_message(event_id: &str, window_start: i64, window_end: Option<i64>) -> String {
	let end_part = match window_end {
		Some(ts) => ts.to_string(),
		None => "open".into(),
	};
	format!("CKB-PoP-Window|{event_id}|{window_start}|{end_part}")
}

// -- Utility --

fn sha256(data: &[u8]) -> [u8; 32] {
	Sha256::digest(data).into()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn event_id_is_deterministic() {
		let a = compute_event_id("ckt1qtest", 1_700_000_000, "nonce1");
		let b = compute_event_id("ckt1qtest", 1_700_000_000, "nonce1");
		assert_eq!(a, b);
		assert_eq!(a.len(), 64);
	}

	#[test]
	fn event_id_changes_with_inputs() {
		let a = compute_event_id("ckt1qtest", 1_700_000_000, "nonce1");
		let b = compute_event_id("ckt1qtest", 1_700_000_001, "nonce1");
		let c = compute_event_id("ckt1qtest", 1_700_000_000, "nonce2");
		assert_ne!(a, b);
		assert_ne!(a, c);
	}

	#[test]
	fn type_script_args_are_64_bytes() {
		let args = build_type_script_args("event123", "address456");
		assert_eq!(args.len(), 64);
	}

	#[test]
	fn qr_payload_roundtrip() {
		let original = QrPayload {
			event_id: "abc123".into(),
			timestamp: 1_700_000_000,
			hmac: "deadbeef01234567".into(),
		};
		let encoded = original.encode();
		let parsed = QrPayload::parse(&encoded).unwrap();
		assert_eq!(parsed, original);
	}

	#[test]
	fn qr_payload_rejects_garbage() {
		assert!(QrPayload::parse("").is_none());
		assert!(QrPayload::parse("only|two").is_none());
		assert!(QrPayload::parse("a|notanumber|c").is_none());
		assert!(QrPayload::parse("|123|hmac").is_none());
	}

	#[test]
	fn hmac_roundtrip() {
		let secret = derive_window_secret("evt1", 1_700_000_000, "sig123");
		let hmac = generate_qr_hmac(&secret, 1_700_000_030);
		assert_eq!(hmac.len(), 16);
		assert!(verify_qr_hmac(&secret, 1_700_000_030, &hmac));
		assert!(!verify_qr_hmac(&secret, 1_700_000_031, &hmac));
	}

	#[test]
	fn badge_cell_data_layout() {
		let data = build_badge_cell_data("evt1", "ckt1qissuer", None);
		assert_eq!(data.len(), 34);
		assert_eq!(data[0], 0x01, "version byte");
		assert_eq!(data[1], 0x01, "flags byte");
	}

	#[test]
	fn attendance_message_format() {
		let msg = attendance_message("EVT001", 1_700_000_000, "ckt1qaddr");
		assert_eq!(msg, "CKB-PoP|EVT001|1700000000|ckt1qaddr");
	}

	#[test]
	fn window_message_open_ended() {
		let msg = window_message("EVT001", 1_700_000_000, None);
		assert_eq!(msg, "CKB-PoP-Window|EVT001|1700000000|open");
	}

	#[test]
	fn window_message_bounded() {
		let msg = window_message("EVT001", 1_700_000_000, Some(1_700_003_600));
		assert_eq!(msg, "CKB-PoP-Window|EVT001|1700000000|1700003600");
	}
}
