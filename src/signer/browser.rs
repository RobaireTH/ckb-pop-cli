use anyhow::{anyhow, Result};
use ckb_types::core::TransactionView;

/// Signs transactions by opening the user's browser to a page that
/// connects to a CKB wallet (JoyID, MetaMask, etc.) and sends the
/// signed result back to a temporary localhost server.
pub struct BrowserSigner {
	address: String,
}

impl BrowserSigner {
	pub fn new(address: String) -> Self {
		Self { address }
	}
}

#[async_trait::async_trait]
impl super::Signer for BrowserSigner {
	fn address(&self) -> &str {
		&self.address
	}

	async fn sign_message(&self, _message: &str) -> Result<String> {
		// TODO: Start localhost HTTP server, serve signing page, open browser,
		//       wait for callback with the signature.
		Err(anyhow!("browser message signing is not yet implemented"))
	}

	async fn sign_transaction(&self, _tx: TransactionView) -> Result<TransactionView> {
		// TODO: Serialize unsigned tx as JSON, serve to browser page,
		//       browser completes inputs/fee via CCC SDK, signs, broadcasts,
		//       posts tx_hash back to localhost callback.
		Err(anyhow!("browser transaction signing is not yet implemented"))
	}
}
