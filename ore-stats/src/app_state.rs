use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
use ore_api::state::{AutomationStrategy, Board, Miner, Round, Treasury};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};
use steel::Pubkey;
use tokio::sync::{broadcast, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppMiner {
    /// The authority of this miner account.
    pub authority: String,

    /// The miner's prospects in the current round.
    pub deployed: [u64; 25],
    /// Total deployed (Sum of miners prospects)
    pub total_deployed: u64,

    /// The cumulative amount of SOL deployed on each square prior to this miner's move.
    pub cumulative: [u64; 25],

    /// SOL witheld in reserve to pay for checkpointing.
    pub checkpoint_fee: u64,

    /// The last round that this miner checkpointed.
    pub checkpoint_id: u64,

    /// The last time this miner claimed ORE rewards.
    pub last_claim_ore_at: i64,

    /// The last time this miner claimed SOL rewards.
    pub last_claim_sol_at: i64,

    /// The amount of SOL this miner can claim.
    pub rewards_sol: u64,

    /// The amount of ORE this miner can claim.
    pub rewards_ore: u64,

    /// The amount of ORE this miner has earned from claim fees.
    pub refined_ore: u64,

    /// The ID of the round this miner last played in.
    pub round_id: u64,

    /// The total amount of SOL this miner has mined across all blocks.
    pub lifetime_rewards_sol: u64,

    /// The total amount of ORE this miner has mined across all blocks.
    pub lifetime_rewards_ore: u64,
}

impl From<Miner> for AppMiner {
    fn from(miner: Miner) -> Self {
        let mut total = 0;
        for m in miner.deployed.iter() {
            total = total + m;
        }
        AppMiner {
            authority: miner.authority.to_string(),
            deployed: miner.deployed,
            total_deployed: total,
            cumulative: miner.cumulative,
            checkpoint_fee: miner.checkpoint_fee,
            checkpoint_id: miner.checkpoint_id,
            last_claim_ore_at: miner.last_claim_ore_at,
            last_claim_sol_at: miner.last_claim_sol_at,
            rewards_sol: miner.rewards_sol,
            rewards_ore: miner.rewards_ore,
            refined_ore: miner.refined_ore,
            round_id: miner.round_id,
            lifetime_rewards_sol: miner.lifetime_rewards_sol,
            lifetime_rewards_ore: miner.lifetime_rewards_ore,
        }
    }
}

#[derive(Debug)]
pub struct ReconstructedRound {
    pub round: AppRound,
    pub deployments: Vec<AppDeployment>,
}

#[derive(Debug, Clone)]
pub struct AppRound {
    pub round_id: i64,
    pub winning_square: i64,
    pub motherlode: i64,
    pub top_miner: String,
    pub total_deployed: i64,
    pub total_vaulted: i64,
    pub total_winnings: i64,
    pub created_at: i64,
}

impl From<Round> for AppRound {
    fn from(round: Round) -> Self {
        if let Some(r) = round.rng() {
            AppRound {
                round_id: round.id as i64,
                winning_square: round.winning_square(r) as i64,
                motherlode: round.motherlode as i64,
                top_miner: round.top_miner.to_string(),
                total_deployed: round.total_deployed as i64,
                total_vaulted: round.total_vaulted as i64,
                total_winnings: round.total_winnings as i64,
                created_at: Utc::now().timestamp(),
            }
        } else {
            AppRound {
                round_id: round.id as i64,
                winning_square: 100,
                motherlode: 0,
                top_miner: Pubkey::default().to_string(),
                total_deployed: 0,
                total_vaulted: 0,
                total_winnings: 0,
                created_at: Utc::now().timestamp(),
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AppDeployment {
    pub pubkey: String,
    pub round_id: i64,
    pub deployments: [AppDeployedSquare; 25],
    pub total_deployed: i64,
    pub total_sol_earned: i64,
    pub total_ore_earned: i64,
    pub winner: bool
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct AppDeployedSquare {
    pub amount: i64,
    pub square_id: i64,
    pub slot: i64
}

#[derive(Debug, Clone)]
pub struct ReconstructedAutomation {
    pub amount: u64,
    pub authority: Pubkey,
    pub executor: Pubkey,
    pub fee: u64,
    pub strategy: AutomationStrategy,
    pub mask: u64,
}

#[derive(Debug, Clone)]
pub struct AutomationCache {
    pub authority: Pubkey,
    pub active: bool,
    pub mask: u64,
    pub strategy: u64,
    pub amount: u64,
    pub fee: u64,
    pub executor: Pubkey,
    pub last_updated_slot: u64,
}

impl AutomationCache {
    pub fn new(authority: Pubkey) -> Self {
        Self {
            authority,
            active: false,
            mask: 0,
            strategy: 0,
            amount: 0,
            fee: 0,
            executor: Pubkey::default(),
            last_updated_slot: 0,
        }
    }
}

impl AppDeployment {
    pub fn new(pubkey: String, round_id: i64) -> Self {
        Self {
            pubkey,
            round_id,
            deployments: [AppDeployedSquare::default(); 25],
            total_deployed: 0,
            total_sol_earned: 0,
            total_ore_earned: 0,
            winner: false,
        }
    }
}


impl Default for AppDeployedSquare {
    fn default() -> Self {
        Self {
            amount: 0,
            square_id: 0,
            slot: 0,
        }
    }
}

