pub mod attend;
pub mod badge;
pub mod event;
pub mod signer;
pub mod tx;

use anyhow::Result;

use crate::cli::{Cli, SignerArg};
use crate::config::Config;

/// Resolve the RPC URL from CLI flag or config.
pub fn resolve_rpc(cli: &Cli, config: &Config) -> String {
	cli.rpc_url
		.clone()
		.unwrap_or_else(|| config.rpc_url(cli.network.as_str()).to_owned())
}

/// Build a signer from CLI flags + config, failing if neither is set.
pub fn resolve_signer(
	cli: &Cli,
	config: &Config,
) -> Result<Box<dyn crate::signer::Signer>> {
	let method: SignerArg = match &cli.signer {
		Some(m) => m.clone(),
		None => match &config.signer.method {
			Some(crate::config::SignerMethod::Browser) => SignerArg::Browser,
			Some(crate::config::SignerMethod::Ledger) => SignerArg::Ledger,
			Some(crate::config::SignerMethod::Passkey) => SignerArg::Passkey,
			Some(crate::config::SignerMethod::Walletconnect) => SignerArg::Walletconnect,
			None => anyhow::bail!(
				"No signer configured. Run: ckb-pop signer set --method <method>"
			),
		},
	};

	let address = cli
		.address
		.as_deref()
		.or(config.signer.address.as_deref())
		.ok_or_else(|| {
			anyhow::anyhow!("No address configured. Run: ckb-pop signer connect")
		})?;

	let network = cli.network.as_str();
	crate::signer::from_method(&method, address.to_owned(), network)
}
