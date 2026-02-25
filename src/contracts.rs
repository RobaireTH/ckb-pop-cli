/// Metadata for a deployed on-chain script.
#[allow(dead_code)]
pub struct ContractInfo {
	/// Type-ID code hash (0x-prefixed, 66 chars).
	pub code_hash: &'static str,
	/// Transaction hash where the script binary was deployed.
	pub deploy_tx_hash: &'static str,
	/// Output index within the deploy transaction.
	pub deploy_out_index: u32,
	/// Data hash of the compiled script binary.
	pub data_hash: &'static str,
}

/// The two PoP protocol scripts for a given network.
pub struct NetworkContracts {
	pub dob_badge: ContractInfo,
	pub event_anchor: ContractInfo,
}

/// All known contract deployments, keyed by network.
pub struct Contracts {
	testnet: NetworkContracts,
}

impl Contracts {
	pub fn for_network(&self, network: &str) -> &NetworkContracts {
		match network {
			"mainnet" => {
				eprintln!("Error: Mainnet contracts are not deployed yet. Use --network testnet.");
				std::process::exit(1);
			}
			_ => &self.testnet,
		}
	}
}

/// Global registry of deployed contract addresses.
pub static CONTRACTS: Contracts = Contracts {
	testnet: NetworkContracts {
		dob_badge: ContractInfo {
			code_hash: "0xb36ed7616c4c87c0779a6c1238e78a84ea68a2638173f25ed140650e0454fbb9",
			deploy_tx_hash:
				"0x9ae36ae06c449d704bc20af5c455c32a220f73249b5b95a15e8a1e352848fda9",
			deploy_out_index: 0,
			data_hash: "0x3da692e19366c26dace65eaa1d6517ca9e4f555cb78a608bfb41d0ea4c5c468b",
		},
		event_anchor: ContractInfo {
			code_hash: "0xd565d738ad5ac99addddc59fd3af5e0d54469dc9834cf766260c7e0d23c70b37",
			deploy_tx_hash:
				"0x9ae36ae06c449d704bc20af5c455c32a220f73249b5b95a15e8a1e352848fda9",
			deploy_out_index: 1,
			data_hash: "0xde6f3d1814ec3bf5aceaf8fe754f9c82affc4de9f277aa6519b5ad52e892807b",
		},
	},
};

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn testnet_code_hashes_are_valid_hex() {
		let c = CONTRACTS.for_network("testnet");
		for info in [&c.dob_badge, &c.event_anchor] {
			let hex = info.code_hash.strip_prefix("0x").unwrap();
			assert_eq!(hex.len(), 64, "code_hash should be 32 bytes");
			assert!(hex::decode(hex).is_ok(), "code_hash should be valid hex");
		}
	}

	#[test]
	fn both_contracts_share_deploy_tx() {
		let c = CONTRACTS.for_network("testnet");
		assert_eq!(c.dob_badge.deploy_tx_hash, c.event_anchor.deploy_tx_hash);
		assert_eq!(c.dob_badge.deploy_out_index, 0);
		assert_eq!(c.event_anchor.deploy_out_index, 1);
	}
}
