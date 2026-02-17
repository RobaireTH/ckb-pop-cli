pub mod browser;

use anyhow::Result;
use ckb_types::core::TransactionView;

use crate::cli::SignerArg;

/// A signer that can produce CKB signatures without holding private keys
/// locally.  Every implementation delegates to an external device or
/// wallet (browser, Ledger, passkey, WalletConnect).
#[async_trait::async_trait]
pub trait Signer: Send + Sync {
	/// The CKB address this signer controls.
	fn address(&self) -> &str;

	/// Sign an arbitrary message and return a hex-encoded recoverable
	/// signature (65 bytes = 130 hex chars).
	async fn sign_message(&self, message: &str) -> Result<String>;

	/// Accept an unsigned transaction, present it to the external signer
	/// for approval, and return the signed transaction ready to broadcast.
	async fn sign_transaction(&self, tx: TransactionView) -> Result<TransactionView>;
}

/// Build a signer from the method chosen on the CLI or in config.
pub fn from_method(method: &SignerArg, address: String) -> Result<Box<dyn Signer>> {
	match method {
		SignerArg::Browser => Ok(Box::new(browser::BrowserSigner::new(address))),
		other => anyhow::bail!("{other:?} signer is not yet implemented"),
	}
}
