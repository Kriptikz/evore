use std::{collections::HashMap, sync::Arc};

use ore_api::state::{Board, Miner, Round, Treasury};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};
use tokio::sync::{broadcast, RwLock};

use crate::GetDeploymentSquished;

#[derive(Clone, Debug, Serialize)]
pub struct AppLiveDeployment {
    pub round: u64,
    pub slot: u64,
    pub authority: String,
    pub deployments: [u64; 25],
    pub total_deployed: u64,
}

#[derive(Clone, Debug, Serialize)]
pub enum LiveBroadcastData {
    Round(AppRound),
    Deployment(AppLiveDeployment),
    WinningSquare(AppWinningSquare),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppWinningSquare {
    pub round_id: u64,
    pub winning_square: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentsCache {
    pub item: HashMap<u64, (Vec<GetDeploymentSquished>, u64)>,
}

#[derive(Clone)]
pub struct AppState {
    pub treasury: Arc<RwLock<AppTreasury>>,
    pub board: Arc<RwLock<AppBoard>>,
    pub staring_round: u64,
    pub rounds: Arc<RwLock<Vec<AppRound>>>,
    pub miners: Arc<RwLock<Vec<AppMiner>>>,
    pub live_data_broadcaster: broadcast::Sender<LiveBroadcastData>,
    pub live_round: Arc<RwLock<AppRound>>,
    pub live_deployments: Arc<RwLock<Vec<AppLiveDeployment>>>,
    pub db_pool: Pool<Sqlite>,
    pub deployments_cache: Arc<RwLock<DeploymentsCache>>
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppTreasury {
    pub balance: u64,
    pub motherlode: u64,
    pub total_staked: u64,
    pub total_unclaimed: u64,
    pub total_refined: u64,
}

impl From<Treasury> for AppTreasury {
    fn from(t: Treasury) -> Self {
        AppTreasury {
            balance: t.balance,
            motherlode: t.motherlode,
            total_staked: t.total_staked,
            total_unclaimed: t.total_unclaimed,
            total_refined: t.total_refined,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppRound {
    pub id: u64,
    pub deployed: [u64; 25],
    pub count: [u64; 25],
    pub slot_hash: [u8; 32],
    pub expires_at: u64,
    pub motherlode: u64,
    pub rent_payer: String,
    pub top_miner: String,
    pub top_miner_reward: u64,
    pub total_deployed: u64,
    pub total_vaulted: u64,
    pub total_winnings: u64,
}

impl From<Round> for AppRound {
    fn from(r: Round) -> Self {
        AppRound {
            id: r.id,
            deployed: r.deployed,
            count: r.count,
            slot_hash: r.slot_hash,
            expires_at: r.expires_at,
            motherlode: r.motherlode,
            rent_payer: r.rent_payer.to_string(),
            top_miner: r.top_miner.to_string(),
            top_miner_reward: r.top_miner_reward,
            total_deployed: r.total_deployed,
            total_vaulted: r.total_vaulted,
            total_winnings: r.total_winnings,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppBoard {
    pub round_id: u64,
    pub start_slot: u64,
    pub end_slot: u64,
}

impl From<Board> for AppBoard {
    fn from(b: Board) -> Self {
        AppBoard {
            round_id: b.round_id,
            start_slot: b.start_slot,
            end_slot: b.end_slot,
        }
    }
}

