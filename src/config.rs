use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
	pub network: NetworkConfig,
	pub signer: SignerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
	pub default: String,
	pub testnet_rpc: String,
	pub mainnet_rpc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignerConfig {
	pub method: Option<SignerMethod>,
	pub address: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignerMethod {
	Browser,
	Ledger,
	Passkey,
	Walletconnect,
}

impl Default for Config {
	fn default() -> Self {
		Self {
			network: NetworkConfig {
				default: "testnet".into(),
				testnet_rpc: "https://testnet.ckb.dev/rpc".into(),
				mainnet_rpc: "https://mainnet.ckb.dev/rpc".into(),
			},
			signer: SignerConfig {
				method: None,
				address: None,
			},
		}
	}
}

impl Config {
	/// Directory where CLI state is stored (~/.ckb-pop/).
	pub fn dir() -> PathBuf {
		dirs::home_dir()
			.expect("could not determine home directory")
			.join(".ckb-pop")
	}

	/// Path to the config file.
	pub fn path() -> PathBuf {
		Self::dir().join("config.toml")
	}

	/// Load config from disk, falling back to defaults if no file exists.
	pub fn load() -> anyhow::Result<Self> {
		let path = Self::path();
		if path.exists() {
			let content = std::fs::read_to_string(&path)?;
			Ok(toml::from_str(&content)?)
		} else {
			Ok(Self::default())
		}
	}

	/// Persist the current config to disk, creating the directory if needed.
	pub fn save(&self) -> anyhow::Result<()> {
		let path = Self::path();
		if let Some(parent) = path.parent() {
			std::fs::create_dir_all(parent)?;
		}
		std::fs::write(&path, toml::to_string_pretty(self)?)?;
		Ok(())
	}

	/// Return the RPC URL for the given network name.
	pub fn rpc_url(&self, network: &str) -> &str {
		match network {
			"mainnet" => &self.network.mainnet_rpc,
			_ => &self.network.testnet_rpc,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn defaults_are_sensible() {
		let c = Config::default();
		assert_eq!(c.network.default, "testnet");
		assert_eq!(c.network.testnet_rpc, "https://testnet.ckb.dev/rpc");
		assert_eq!(c.network.mainnet_rpc, "https://mainnet.ckb.dev/rpc");
		assert!(c.signer.method.is_none());
		assert!(c.signer.address.is_none());
	}

	#[test]
	fn toml_roundtrip() {
		let mut c = Config::default();
		c.signer.method = Some(SignerMethod::Browser);
		c.signer.address = Some("ckt1qtest".into());

		let serialized = toml::to_string_pretty(&c).unwrap();
		let parsed: Config = toml::from_str(&serialized).unwrap();

		assert_eq!(parsed.signer.method, Some(SignerMethod::Browser));
		assert_eq!(parsed.signer.address.as_deref(), Some("ckt1qtest"));
	}

	#[test]
	fn rpc_url_selection() {
		let c = Config::default();
		assert_eq!(c.rpc_url("testnet"), "https://testnet.ckb.dev/rpc");
		assert_eq!(c.rpc_url("mainnet"), "https://mainnet.ckb.dev/rpc");
		// Unknown network falls back to testnet.
		assert_eq!(c.rpc_url("devnet"), "https://testnet.ckb.dev/rpc");
	}
}
