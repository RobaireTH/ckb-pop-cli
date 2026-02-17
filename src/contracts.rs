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
			// Mainnet contracts are not yet deployed.
			"mainnet" => unimplemented!("mainnet contracts are not deployed yet"),
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
				"0x40ca450088affcbc5f2d8a06545566717106e99caad306776704aab9f3127934",
			deploy_out_index: 0,
			data_hash: "0x6e550910a640a41f21614d97d1d7b8c1830cbf11cce5c868c76a6fd0f25ba7a9",
		},
		event_anchor: ContractInfo {
			code_hash: "0xd565d738ad5ac99addddc59fd3af5e0d54469dc9834cf766260c7e0d23c70b37",
			deploy_tx_hash:
				"0x40ca450088affcbc5f2d8a06545566717106e99caad306776704aab9f3127934",
			deploy_out_index: 1,
			data_hash: "0x24dfb1d2a7aca1e967c405e60017204459f3f8fe80e2c21683c4288ad4f5befb",
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
