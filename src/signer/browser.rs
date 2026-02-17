use std::io::Write as _;

use anyhow::{anyhow, Result};
use ckb_types::core::TransactionView;
use ckb_types::prelude::IntoTransactionView;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

/// Signs transactions by opening the user's browser to a localhost page
/// that loads the CCC SDK and connects to the user's wallet.
pub struct BrowserSigner {
	address: String,
	network: String,
}

impl BrowserSigner {
	pub fn new(address: String, network: String) -> Self {
		Self { address, network }
	}
}

#[async_trait::async_trait]
impl super::Signer for BrowserSigner {
	fn address(&self) -> &str {
		&self.address
	}

	async fn sign_message(&self, message: &str) -> Result<String> {
		let request = serde_json::json!({
			"action": "sign_message",
			"network": self.network,
			"message": message,
		});
		let result = run_browser_session(&request).await?;
		result["signature"]
			.as_str()
			.map(String::from)
			.ok_or_else(|| anyhow!("browser did not return a signature"))
	}

	async fn sign_transaction(&self, tx: TransactionView) -> Result<TransactionView> {
		let json_tx = ckb_jsonrpc_types::TransactionView::from(tx);
		let request = serde_json::json!({
			"action": "sign_transaction",
			"network": self.network,
			"transaction": json_tx.inner,
		});
		let result = run_browser_session(&request).await?;

		let signed_json: ckb_jsonrpc_types::Transaction =
			serde_json::from_value(result["transaction"].clone())
				.map_err(|e| anyhow!("failed to parse signed transaction: {e}"))?;

		let packed: ckb_types::packed::Transaction = signed_json.into();
		Ok(packed.into_view())
	}
}

/// Open a browser to connect a wallet and return the CKB address.
/// Used by `signer connect` before any signer instance exists.
pub async fn connect_wallet(network: &str) -> Result<String> {
	let request = serde_json::json!({
		"action": "connect",
		"network": network,
	});
	let result = run_browser_session(&request).await?;
	result["address"]
		.as_str()
		.map(String::from)
		.ok_or_else(|| anyhow!("browser did not return an address"))
}

// ---------------------------------------------------------------------------
// Localhost HTTP server that serves the signing page and waits for a callback.
// ---------------------------------------------------------------------------

/// Bind a TCP listener on a random high port.
async fn bind_listener() -> Result<TcpListener> {
	// Try a few random ports in the ephemeral range.
	for _ in 0..10 {
		let port = 17500 + (rand::random::<u16>() % 100);
		if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)).await {
			return Ok(listener);
		}
	}
	// Last resort: let the OS pick.
	Ok(TcpListener::bind("127.0.0.1:0").await?)
}

/// Start the localhost server, open the browser, and wait for the callback.
async fn run_browser_session(request: &serde_json::Value) -> Result<serde_json::Value> {
	let listener = bind_listener().await?;
	let port = listener.local_addr()?.port();
	let url = format!("http://127.0.0.1:{port}");

	let request_json = serde_json::to_string(request)?;
	let html = build_signing_page(port);

	let (tx, rx) = oneshot::channel::<serde_json::Value>();
	let tx_cell = std::sync::Mutex::new(Some(tx));

	eprintln!("Opening browser at {url} ...");
	if opener::open(&url).is_err() {
		eprintln!("Could not open browser automatically.");
		eprintln!("Please visit: {url}");
	}

	// Serve requests until we get the callback.
	loop {
		let (mut stream, _) = listener.accept().await?;
		let mut buf = vec![0u8; 8192];
		let n = stream.read(&mut buf).await?;
		let raw = String::from_utf8_lossy(&buf[..n]);

		if raw.starts_with("GET /request") {
			let resp = http_response(200, "application/json", request_json.as_bytes());
			stream.write_all(&resp).await?;
		} else if raw.starts_with("POST /callback") {
			// Extract the JSON body after the blank line.
			let body = raw
				.find("\r\n\r\n")
				.map(|i| &raw[i + 4..])
				.unwrap_or("");
			let value: serde_json::Value = serde_json::from_str(body)
				.map_err(|e| anyhow!("invalid callback JSON: {e}"))?;

			let resp = http_response(200, "text/plain", b"ok");
			stream.write_all(&resp).await?;

			if let Some(sender) = tx_cell.lock().unwrap().take() {
				let _ = sender.send(value);
			}
			break;
		} else if raw.starts_with("GET") {
			// Serve the signing page for any other GET (including GET /).
			let resp = http_response(200, "text/html", html.as_bytes());
			stream.write_all(&resp).await?;
		} else {
			let resp = http_response(404, "text/plain", b"not found");
			stream.write_all(&resp).await?;
		}
	}

	rx.await
		.map_err(|_| anyhow!("browser session was cancelled"))
		.and_then(|v| {
			if let Some(err) = v["error"].as_str() {
				Err(anyhow!("wallet error: {err}"))
			} else {
				Ok(v)
			}
		})
}

fn http_response(status: u16, content_type: &str, body: &[u8]) -> Vec<u8> {
	let reason = match status {
		200 => "OK",
		404 => "Not Found",
		_ => "Error",
	};
	let mut resp = Vec::new();
	write!(
		resp,
		"HTTP/1.1 {status} {reason}\r\n\
		 Content-Type: {content_type}\r\n\
		 Content-Length: {}\r\n\
		 Access-Control-Allow-Origin: *\r\n\
		 Connection: close\r\n\
		 \r\n",
		body.len()
	)
	.unwrap();
	resp.extend_from_slice(body);
	resp
}

// ---------------------------------------------------------------------------
// Embedded HTML signing page.
// ---------------------------------------------------------------------------

fn build_signing_page(port: u16) -> String {
	format!(
		r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>ckb-pop – wallet signing</title>
<style>
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}
  body {{ font-family: system-ui, sans-serif; background: #0d1117; color: #c9d1d9;
         display: flex; align-items: center; justify-content: center; min-height: 100vh; }}
  .card {{ background: #161b22; border: 1px solid #30363d; border-radius: 12px;
           padding: 2rem; max-width: 420px; width: 100%; text-align: center; }}
  h1 {{ font-size: 1.4rem; margin-bottom: 1rem; }}
  #status {{ margin: 1rem 0; color: #8b949e; min-height: 1.5rem; }}
  .success {{ color: #3fb950 !important; }}
  .error {{ color: #f85149 !important; }}
  button {{ background: #238636; color: #fff; border: none; border-radius: 6px;
           padding: 0.6rem 1.5rem; font-size: 1rem; cursor: pointer; }}
  button:hover {{ background: #2ea043; }}
  button:disabled {{ opacity: 0.5; cursor: not-allowed; }}
</style>
</head>
<body>
<div class="card">
  <h1>ckb-pop</h1>
  <p id="status">Loading CCC SDK...</p>
  <div id="connector-host"></div>
</div>

<script type="module">
const PORT = {port};
const BASE = `http://127.0.0.1:${{PORT}}`;
const status = document.getElementById("status");

function setStatus(msg, cls) {{
  status.textContent = msg;
  status.className = cls || "";
}}

async function main() {{
  // Dynamically import CCC SDK.
  const {{ ccc }} = await import("https://esm.sh/@ckb-ccc/ccc@1.1.10");
  await import("https://esm.sh/@ckb-ccc/connector@1.1.10");

  // Fetch the signing request from the CLI server.
  const req = await fetch(`${{BASE}}/request`).then(r => r.json());

  // Create the right client for the network.
  const client = req.network === "mainnet"
    ? new ccc.ClientPublicMainnet()
    : new ccc.ClientPublicTestnet();

  // Set up the wallet connector.
  const connector = document.createElement("ccc-connector");
  connector.client = client;
  connector.name = "ckb-pop";
  document.getElementById("connector-host").appendChild(connector);

  setStatus("Connect your wallet to continue.");

  // Auto-open the wallet selection modal.
  await new Promise(r => setTimeout(r, 300));
  connector.isOpen = true;
  if (connector.requestUpdate) connector.requestUpdate();

  // Wait for a wallet connection.
  const signer = await new Promise((resolve) => {{
    const check = () => {{
      const s = connector.signer?.signer ?? connector.signer;
      if (s) resolve(s);
    }};
    connector.addEventListener("connected", check);
    // Also poll in case the event fires before our listener.
    const timer = setInterval(() => {{
      check();
      if (connector.signer) clearInterval(timer);
    }}, 500);
  }});

  setStatus("Wallet connected. Processing...");

  try {{
    let result;

    if (req.action === "connect") {{
      const addr = await signer.getRecommendedAddress();
      result = {{ address: addr }};
    }}
    else if (req.action === "sign_message") {{
      const sig = await signer.signMessage(req.message);
      result = {{ signature: sig.signature || sig }};
    }}
    else if (req.action === "sign_transaction") {{
      // Reconstruct a CCC Transaction from the CLI's unsigned JSON.
      const tx = ccc.Transaction.from(req.transaction);

      // Let CCC fill in inputs and fees from the connected wallet.
      await tx.completeInputsByCapacity(signer);
      await tx.completeFeeBy(signer, 1000);

      // Sign without broadcasting — the CLI will broadcast.
      const signed = await signer.signTransaction(tx);

      // Serialize for transport back to the CLI.
      const json = JSON.parse(JSON.stringify(signed, (_, v) =>
        typeof v === "bigint" ? "0x" + v.toString(16) : v
      ));
      result = {{ transaction: json }};
    }}
    else {{
      throw new Error("Unknown action: " + req.action);
    }}

    // Send the result back to the CLI server.
    await fetch(`${{BASE}}/callback`, {{
      method: "POST",
      headers: {{ "Content-Type": "application/json" }},
      body: JSON.stringify(result),
    }});

    setStatus("Done! You can close this tab.", "success");
  }} catch (err) {{
    setStatus("Error: " + (err.message || err), "error");
    // Report the error so the CLI doesn't hang forever.
    await fetch(`${{BASE}}/callback`, {{
      method: "POST",
      headers: {{ "Content-Type": "application/json" }},
      body: JSON.stringify({{ error: err.message || String(err) }}),
    }}).catch(() => {{}});
  }}
}}

main().catch(err => setStatus("Fatal: " + err.message, "error"));
</script>
</body>
</html>"##
	)
}
