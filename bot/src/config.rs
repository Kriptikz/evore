//! Bot Configuration - Structs for multi-bot configuration
//!
//! Supports loading from TOML config file with per-bot keypair paths.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Deployment strategy for a bot
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeployStrategy {
    /// Expected Value calculation
    EV,
    /// Percentage-based deployment
    Percentage,
    /// Manual square selection
    Manual,
}

impl Default for DeployStrategy {
    fn default() -> Self {
        DeployStrategy::EV
    }
}

/// Strategy-specific parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum StrategyParams {
    /// EV strategy parameters
    EV {
        /// Maximum amount to deploy per square (lamports)
        max_per_square: u64,
        /// Minimum bet threshold (lamports)
        min_bet: u64,
        /// Value of 1 ORE in lamports (for EV calculation)
        ore_value: u64,
    },
    /// Percentage strategy parameters
    Percentage {
        /// Percentage in basis points (1000 = 10%)
        percentage: u64,
        /// Number of squares to deploy to
        squares_count: u64,
    },
    /// Manual strategy parameters
    Manual {
        /// Exact amounts to deploy per square
        amounts: [u64; 25],
    },
}

impl Default for StrategyParams {
    fn default() -> Self {
        StrategyParams::EV {
            max_per_square: 100_000_000, // 0.1 SOL
            min_bet: 10_000,
            ore_value: 800_000_000, // 0.8 SOL per ORE
        }
    }
}

/// Configuration for a single bot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotConfig {
    /// Unique name for logging/display
    pub name: String,
    
    /// Auth ID for this bot's managed miner
    pub auth_id: u64,
    
    /// Deployment strategy
    #[serde(default)]
    pub strategy: DeployStrategy,
    
    /// When to start deploying (slots before end)
    #[serde(default = "default_slots_left")]
    pub slots_left: u64,
    
    /// Bankroll for this bot (lamports)
    pub bankroll: u64,
    
    /// Number of deploy transactions to send (default 4)
    #[serde(default = "default_attempts")]
    pub attempts: u64,
    
    /// Priority fee in micro-lamports per CU (default 5000 = ~0.000007 SOL @ 1.4M CU)
    #[serde(default = "default_priority_fee")]
    pub priority_fee: u64,
    
    /// Jito tip in lamports (default 200_000 = 0.0002 SOL, 0 to disable)
    #[serde(default = "default_jito_tip")]
    pub jito_tip: u64,
    
    /// Whether bot starts in paused state (default false)
    #[serde(default)]
    pub paused_on_startup: bool,
    
    /// Strategy-specific parameters
    #[serde(default)]
    pub strategy_params: StrategyParams,
    
    /// Path to signer keypair (optional, falls back to defaults)
    pub signer_path: Option<PathBuf>,
    
    /// Path to manager keypair (optional, falls back to defaults)
    pub manager_path: Option<PathBuf>,
}

fn default_slots_left() -> u64 {
    2
}

fn default_attempts() -> u64 {
    4
}

fn default_priority_fee() -> u64 {
    5000  // ~0.000007 SOL @ 1.4M CU (above Helius SWQOS minimum)
}

fn default_jito_tip() -> u64 {
    200_000  // 0.0002 SOL - minimum for Helius Sender
}

impl BotConfig {
    /// Create a new EV bot config with defaults
    pub fn new_ev(
        name: impl Into<String>,
        auth_id: u64,
        bankroll: u64,
        max_per_square: u64,
        min_bet: u64,
        ore_value: u64,
    ) -> Self {
        Self {
            name: name.into(),
            auth_id,
            strategy: DeployStrategy::EV,
            slots_left: 2,
            bankroll,
            attempts: 4,
            priority_fee: 5000,
            jito_tip: 200_000,
            paused_on_startup: false,
            strategy_params: StrategyParams::EV {
                max_per_square,
                min_bet,
                ore_value,
            },
            signer_path: None,
            manager_path: None,
        }
    }

    /// Get manager pubkey from loaded keypair (if available)
    /// Note: Actual keypair loading happens elsewhere
    pub fn get_display_name(&self) -> String {
        format!("{} (auth_id={})", self.name, self.auth_id)
    }
}

/// Manage command configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ManageConfig {
    /// Path to directory containing signer keypair files (*.json)
    pub signers_path: Option<PathBuf>,
    
    /// Secondary/legacy program ID for claim-only operations
    pub secondary_program_id: Option<String>,
}

impl ManageConfig {
    /// Check if config is valid (has signers_path)
    pub fn is_valid(&self) -> bool {
        self.signers_path.is_some()
    }
}

/// Top-level configuration with defaults and bot list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Default signer keypair path
    #[serde(default = "default_signer_path")]
    pub default_signer_path: PathBuf,
    
    /// Default manager keypair path
    #[serde(default = "default_manager_path")]
    pub default_manager_path: PathBuf,
    
    /// List of bots to run
    #[serde(default)]
    pub bots: Vec<BotConfig>,
    
    /// Manage command configuration
    #[serde(default)]
    pub manage: ManageConfig,
}

fn default_signer_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config/solana/id.json")
}

fn default_manager_path() -> PathBuf {
    PathBuf::from("./manager.json")
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_signer_path: default_signer_path(),
            default_manager_path: default_manager_path(),
            bots: Vec::new(),
            manage: ManageConfig::default(),
        }
    }
}

impl Config {
    /// Load config from TOML file
    pub fn load(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Get the signer path for a bot (falls back to default)
    pub fn get_signer_path(&self, bot: &BotConfig) -> PathBuf {
        bot.signer_path
            .clone()
            .unwrap_or_else(|| self.default_signer_path.clone())
    }

    /// Get the manager path for a bot (falls back to default)
    pub fn get_manager_path(&self, bot: &BotConfig) -> PathBuf {
        bot.manager_path
            .clone()
            .unwrap_or_else(|| self.default_manager_path.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bot_config_new_ev() {
        let config = BotConfig::new_ev("test-bot", 1, 100_000_000, 50_000_000, 10_000, 800_000_000);
        assert_eq!(config.name, "test-bot");
        assert_eq!(config.auth_id, 1);
        assert_eq!(config.strategy, DeployStrategy::EV);
    }

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert!(config.bots.is_empty());
    }

    #[test]
    fn test_strategy_params_serialize() {
        let params = StrategyParams::EV {
            max_per_square: 100_000_000,
            min_bet: 10_000,
            ore_value: 800_000_000,
        };
        let serialized = toml::to_string(&params).unwrap();
        assert!(serialized.contains("max_per_square"));
    }
}
