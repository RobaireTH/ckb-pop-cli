use anyhow::Result;

use crate::cli::{SignerArg, SignerCommand};
use crate::config::{Config, SignerMethod};
use crate::signer::browser;

pub async fn run(cmd: &SignerCommand) -> Result<()> {
	match cmd {
		SignerCommand::Set { method } => set_method(method),
		SignerCommand::Connect => connect().await,
		SignerCommand::Status => show_status(),
	}
}

fn set_method(method: &SignerArg) -> Result<()> {
	let sm = match method {
		SignerArg::Browser => SignerMethod::Browser,
		SignerArg::Ledger => SignerMethod::Ledger,
		SignerArg::Passkey => SignerMethod::Passkey,
		SignerArg::Walletconnect => SignerMethod::Walletconnect,
	};
	let label = format!("{sm:?}").to_lowercase();

	let mut config = Config::load()?;
	config.signer.method = Some(sm);
	config.save()?;
	println!("Signer method set to: {label}");
	Ok(())
}

async fn connect() -> Result<()> {
	let config = Config::load()?;
	let method = config.signer.method.as_ref().ok_or_else(|| {
		anyhow::anyhow!("No signer method set. Run: ckb-pop signer set --method <method>")
	})?;

	let address = match method {
		SignerMethod::Browser => {
			println!("Opening browser to connect wallet...");
			browser::connect_wallet(&config.network.default).await?
		}
		other => anyhow::bail!("{other:?} connect is not yet implemented"),
	};

	println!("Connected: {address}");

	let mut config = config;
	config.signer.address = Some(address);
	config.save()?;
	println!("Address saved to config.");

	Ok(())
}

fn show_status() -> Result<()> {
	let config = Config::load()?;

	let method = config
		.signer
		.method
		.as_ref()
		.map(|m| format!("{m:?}").to_lowercase())
		.unwrap_or_else(|| "not set".into());

	let address = config
		.signer
		.address
		.as_deref()
		.unwrap_or("not connected");

	println!("Signer");
	println!("  Method:  {method}");
	println!("  Address: {address}");
	println!("  Network: {}", config.network.default);
	println!("  RPC:     {}", config.rpc_url(&config.network.default));
	Ok(())
}
