use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
	name = "ckb-pop",
	about = "Keyless CLI for the PoP protocol on Nervos CKB.",
	version
)]
pub struct Cli {
	/// Network to connect to.
	#[arg(long, default_value = "testnet", global = true)]
	pub network: Network,

	/// Override RPC endpoint URL.
	#[arg(long, global = true)]
	pub rpc_url: Option<String>,

	/// Override signing method.
	#[arg(long, global = true)]
	pub signer: Option<SignerArg>,

	/// Override active CKB address.
	#[arg(long, global = true)]
	pub address: Option<String>,

	#[command(subcommand)]
	pub command: Command,
}

#[derive(Clone, ValueEnum)]
pub enum Network {
	Testnet,
	Mainnet,
}

impl Network {
	pub fn as_str(&self) -> &str {
		match self {
			Self::Testnet => "testnet",
			Self::Mainnet => "mainnet",
		}
	}
}

#[derive(Clone, ValueEnum)]
pub enum SignerArg {
	Browser,
	Ledger,
	Passkey,
	Walletconnect,
}

#[derive(Subcommand)]
pub enum Command {
	/// Manage external signer configuration.
	Signer {
		#[command(subcommand)]
		command: SignerCommand,
	},

	/// Create and query events.
	Event {
		#[command(subcommand)]
		command: EventCommand,
	},

	/// Scan QR, verify attendance, and mint a badge in one step.
	Attend {
		/// QR code data in the format event_id|timestamp|hmac.
		qr_data: String,
	},

	/// Mint and query soulbound badges.
	Badge {
		#[command(subcommand)]
		command: BadgeCommand,
	},

	/// Check transaction status on-chain.
	Tx {
		#[command(subcommand)]
		command: TxCommand,
	},
}

// -- Signer subcommands --

#[derive(Subcommand)]
pub enum SignerCommand {
	/// Set the default signing method.
	Set {
		/// Signing method to use.
		#[arg(long)]
		method: SignerArg,
	},

	/// Authenticate with an external wallet and store the address.
	Connect,

	/// Show current signer configuration.
	Status,
}

// -- Event subcommands --

#[derive(Subcommand)]
pub enum EventCommand {
	/// Create a new event and anchor it on-chain.
	Create {
		/// Event name.
		#[arg(long)]
		name: String,

		/// Event description.
		#[arg(long)]
		description: String,

		/// URL for the event image.
		#[arg(long)]
		image_url: Option<String>,

		/// Event location.
		#[arg(long)]
		location: Option<String>,

		/// Start time (ISO 8601).
		#[arg(long)]
		start: Option<String>,

		/// End time (ISO 8601).
		#[arg(long)]
		end: Option<String>,
	},

	/// List events visible on-chain.
	List {
		/// Filter by creator address.
		#[arg(long)]
		creator: Option<String>,
	},

	/// Show details of an on-chain event anchor.
	Show {
		/// Event ID (64-character hex string).
		event_id: String,
	},

	/// Open an attendance window and display rotating QR codes.
	Window {
		/// Event ID (64-character hex string).
		event_id: String,

		/// Window duration in minutes. Use 0 for open-ended.
		#[arg(long, default_value = "60")]
		duration: u64,
	},
}

// -- Badge subcommands --

#[derive(Subcommand)]
pub enum BadgeCommand {
	/// Mint a soulbound badge for an attendee (organizer action).
	Mint {
		/// Event ID (64-character hex string).
		event_id: String,

		/// Recipient CKB address.
		#[arg(long)]
		to: String,
	},

	/// List badges held by an address.
	List {
		/// CKB address to query.
		#[arg(long)]
		address: String,
	},

	/// Verify whether a badge exists on-chain.
	Verify {
		/// Event ID (64-character hex string).
		event_id: String,

		/// Holder CKB address.
		address: String,
	},
}

// -- Tx subcommands --

#[derive(Subcommand)]
pub enum TxCommand {
	/// Check confirmation status of a transaction.
	Status {
		/// Transaction hash (0x-prefixed).
		tx_hash: String,
	},
}
