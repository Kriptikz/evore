//! Configuration for the crank program

use clap::{Parser, Subcommand};
use solana_sdk::{pubkey::Pubkey, signature::Keypair};
use std::path::PathBuf;

/// Evore Autodeploy Crank
#[derive(Parser, Debug, Clone)]
#[command(name = "evore-crank")]
#[command(about = "Automated deployer crank for Evore", long_about = None)]
pub struct Config {
    /// Subcommand to run
    #[command(subcommand)]
    pub command: Option<Command>,
    
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
    
    /// Poll interval in milliseconds
    #[arg(long, env = "POLL_INTERVAL_MS", default_value = "400")]
    pub poll_interval_ms: u64,
    
    /// [LEGACY] Address Lookup Table for manual LUT commands (show-lut, deactivate-lut, close-lut)
    /// Not needed for 'run' - the crank auto-discovers and creates LUTs as needed
    #[arg(long, env = "LUT_ADDRESS")]
    pub lut_address: Option<Pubkey>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    /// Run the main crank loop (auto-discovers/creates LUTs)
    Run,
    /// Run the new pipeline architecture (experimental)
    Pipeline,
    /// Send a test transaction to verify connectivity
    Test,
    /// Show deployer accounts we manage and their LUT status
    List,
    /// Update expected fees for all deployers (as deploy_authority)
    SetExpectedFees {
        /// Expected BPS fee (0 = accept any)
        #[arg(long, default_value = "0")]
        expected_bps_fee: u64,
        /// Expected flat fee in lamports (0 = accept any)
        #[arg(long, default_value = "5000")]
        expected_flat_fee: u64,
    },
    /// [LEGACY] Create a new Address Lookup Table (LUT) manually
    CreateLut,
    /// [LEGACY] Extend LUT with static shared accounts manually
    ExtendLut,
    /// [LEGACY] Show LUT contents (requires LUT_ADDRESS)
    ShowLut,
    /// [LEGACY] Deactivate LUT (requires LUT_ADDRESS, ~512 slot cooldown)
    DeactivateLut,
    /// [LEGACY] Close LUT and reclaim rent (requires LUT_ADDRESS)
    CloseLut,
    /// Deactivate LUTs that don't match current required accounts (wrong automation address, etc)
    DeactivateUnused,
    /// Show deactivating LUTs status and close any that are ready
    CleanupDeactivated,
    /// Check all Evore program accounts
    CheckAccounts,
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
    /// Percentage fee in basis points (1000 = 10%, 500 = 5%)
    pub bps_fee: u64,
    /// Flat fee in lamports (added on top of bps_fee)
    pub flat_fee: u64,
    /// Expected bps_fee set by deploy_authority (0 = accept any)
    pub expected_bps_fee: u64,
    /// Expected flat_fee set by deploy_authority (0 = accept any)
    pub expected_flat_fee: u64,
    /// Maximum lamports to deploy per round (0 = unlimited)
    pub max_per_round: u64,
}
