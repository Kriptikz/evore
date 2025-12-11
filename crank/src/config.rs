//! Configuration for the crank program

use clap::Parser;
use solana_sdk::{pubkey::Pubkey, signature::Keypair};
use std::path::PathBuf;

/// Evore Autodeploy Crank
#[derive(Parser, Debug, Clone)]
#[command(name = "evore-crank")]
#[command(about = "Automated deployer crank for Evore", long_about = None)]
pub struct Config {
    /// RPC URL
    #[arg(long, env = "RPC_URL", default_value = "https://api.mainnet-beta.solana.com")]
    pub rpc_url: String,
    
    /// Deploy authority keypair path
    #[arg(long, env = "DEPLOY_AUTHORITY_KEYPAIR")]
    pub keypair_path: PathBuf,
    
    /// Database path
    #[arg(long, env = "DATABASE_PATH", default_value = "crank.db")]
    pub db_path: PathBuf,
    
    /// Priority fee in microlamports per compute unit
    #[arg(long, env = "PRIORITY_FEE", default_value = "100000")]
    pub priority_fee: u64,
    
    /// Jito tip in lamports
    #[arg(long, env = "JITO_TIP", default_value = "200000")]
    pub jito_tip: u64,
    
    /// Enable Jito bundle sending
    #[arg(long, env = "USE_JITO", default_value = "true")]
    pub use_jito: bool,
    
    /// Helius API key (for fast sender)
    #[arg(long, env = "HELIUS_API_KEY")]
    pub helius_api_key: Option<String>,
    
    /// Poll interval in milliseconds
    #[arg(long, env = "POLL_INTERVAL_MS", default_value = "400")]
    pub poll_interval_ms: u64,
}

impl Config {
    /// Load the deploy authority keypair from the configured path
    pub fn load_keypair(&self) -> Result<Keypair, Box<dyn std::error::Error>> {
        let keypair_data = std::fs::read_to_string(&self.keypair_path)?;
        let keypair_bytes: Vec<u8> = serde_json::from_str(&keypair_data)?;
        Ok(Keypair::from_bytes(&keypair_bytes)?)
    }
}

/// Information about a deployer the crank is managing
#[derive(Debug, Clone)]
pub struct DeployerInfo {
    /// The deployer PDA address
    pub deployer_address: Pubkey,
    /// The manager account address
    pub manager_address: Pubkey,
    /// The autodeploy_balance PDA address
    pub autodeploy_balance_address: Pubkey,
    /// Fee in basis points
    pub fee_bps: u64,
}
