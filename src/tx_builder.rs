use anyhow::{anyhow, Result};
use ckb_types::{
	bytes::Bytes,
	core::{Capacity, TransactionBuilder},
	packed::{CellDep, CellOutput, OutPoint, Script},
	prelude::*,
	H256,
};

use crate::contracts::ContractInfo;
use crate::crypto;

/// Build an unsigned transaction that creates an event-anchor cell.
///
/// The caller is responsible for adding inputs, balancing capacity,
/// and signing — this only produces the output side of the transaction
/// plus the required cell dep.
pub fn build_event_anchor(
	contract: &ContractInfo,
	event_id: &str,
	creator_address: &str,
	creator_lock: Script,
	metadata_hash: Option<&str>,
) -> Result<ckb_types::core::TransactionView> {
	let args = crypto::build_type_script_args(event_id, creator_address);
	let type_script = type_script_from(contract, args)?;
	let cell_data = crypto::build_anchor_cell_data(event_id, creator_address, metadata_hash);
	let cell_dep = cell_dep_for(contract)?;

	let data_bytes = Bytes::from(cell_data);

	let output = CellOutput::new_builder()
		.lock(creator_lock)
		.type_(Some(type_script).pack())
		.build();
	let output = set_min_capacity(output, data_bytes.len());

	Ok(TransactionBuilder::default()
		.output(output)
		.output_data(data_bytes.pack())
		.cell_dep(cell_dep)
		.build())
}

/// Build an unsigned transaction that creates a dob-badge cell.
pub fn build_badge_mint(
	contract: &ContractInfo,
	event_id: &str,
	recipient_address: &str,
	recipient_lock: Script,
	issuer_address: &str,
	proof_hash: Option<&str>,
) -> Result<ckb_types::core::TransactionView> {
	let args = crypto::build_type_script_args(event_id, recipient_address);
	let type_script = type_script_from(contract, args)?;
	let cell_data = crypto::build_badge_cell_data(event_id, issuer_address, proof_hash);
	let cell_dep = cell_dep_for(contract)?;

	let data_bytes = Bytes::from(cell_data);

	let output = CellOutput::new_builder()
		.lock(recipient_lock)
		.type_(Some(type_script).pack())
		.build();
	let output = set_min_capacity(output, data_bytes.len());

	Ok(TransactionBuilder::default()
		.output(output)
		.output_data(data_bytes.pack())
		.cell_dep(cell_dep)
		.build())
}

// -- Helpers --

/// Compute the minimum CKB capacity a cell needs and set it on the output.
/// Formula: (8 + occupied_bytes) * 1 CKB, where occupied_bytes includes the
/// lock script, type script, and output data.
fn set_min_capacity(output: CellOutput, data_len: usize) -> CellOutput {
	let occupied = output
		.occupied_capacity(Capacity::bytes(data_len).unwrap())
		.unwrap();
	output
		.as_builder()
		.capacity(occupied.pack())
		.build()
}

fn type_script_from(contract: &ContractInfo, args: Vec<u8>) -> Result<Script> {
	let code_hash = parse_h256(contract.code_hash)?;
	Ok(Script::new_builder()
		.code_hash(code_hash.pack())
		.hash_type(ckb_types::core::ScriptHashType::Type)
		.args(Bytes::from(args).pack())
		.build())
}

fn cell_dep_for(contract: &ContractInfo) -> Result<CellDep> {
	let tx_hash = parse_h256(contract.deploy_tx_hash)?;
	let out_point = OutPoint::new(tx_hash.pack(), contract.deploy_out_index);
	Ok(CellDep::new_builder().out_point(out_point).build())
}

fn parse_h256(s: &str) -> Result<H256> {
	s.strip_prefix("0x")
		.unwrap_or(s)
		.parse()
		.map_err(|e| anyhow!("invalid 256-bit hash: {e}"))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::contracts::CONTRACTS;

	/// Dummy lock script for testing.
	fn dummy_lock() -> Script {
		Script::new_builder()
			.code_hash([0u8; 32].pack())
			.hash_type(ckb_types::core::ScriptHashType::Data)
			.args(Bytes::from(vec![0u8; 20]).pack())
			.build()
	}

	#[test]
	fn event_anchor_tx_has_one_output() {
		let c = CONTRACTS.for_network("testnet");
		let tx = build_event_anchor(
			&c.event_anchor,
			"test_event",
			"ckt1qtest",
			dummy_lock(),
			None,
		)
		.unwrap();

		assert_eq!(tx.outputs().len(), 1);
		assert_eq!(tx.outputs_data().len(), 1);
		assert_eq!(tx.cell_deps().len(), 1);
		assert!(tx.outputs().get(0).unwrap().type_().to_opt().is_some());
	}

	#[test]
	fn badge_mint_tx_has_one_output() {
		let c = CONTRACTS.for_network("testnet");
		let tx = build_badge_mint(
			&c.dob_badge,
			"test_event",
			"ckt1qrecipient",
			dummy_lock(),
			"ckt1qissuer",
			None,
		)
		.unwrap();

		assert_eq!(tx.outputs().len(), 1);
		assert_eq!(tx.cell_deps().len(), 1);

		// Cell data should be 34 bytes (version + flags + content hash).
		let data: Vec<u8> = tx.outputs_data().get(0).unwrap().raw_data().to_vec();
		assert_eq!(data.len(), 34);
		assert_eq!(data[0], 0x01);
	}

	#[test]
	fn type_script_args_match_crypto_module() {
		let c = CONTRACTS.for_network("testnet");
		let tx = build_event_anchor(
			&c.event_anchor,
			"myevent",
			"myaddr",
			dummy_lock(),
			None,
		)
		.unwrap();

		let output = tx.outputs().get(0).unwrap();
		let type_script = output.type_().to_opt().unwrap();
		let args: Vec<u8> = type_script.args().raw_data().to_vec();

		let expected = crypto::build_type_script_args("myevent", "myaddr");
		assert_eq!(args, expected);
	}
}
