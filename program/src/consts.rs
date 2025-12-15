use solana_program::pubkey;
use steel::Pubkey;

pub const MANAGED_MINER_AUTH: &[u8] = b"managed-miner-auth";
pub const DEPLOYER: &[u8] = b"deployer";
pub const FEE_COLLECTOR: Pubkey = pubkey!("56qSi79jWdM1zie17NKFvdsh213wPb15HHUqGUjmJ2Lr");

pub const DEPLOY_FEE: u64 = 0_000_001_000;

