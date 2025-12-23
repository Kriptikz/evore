use steel::*;
use serde::{Serialize, Deserialize};

use crate::consts::{MANAGED_MINER_AUTH, DEPLOYER};

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoPrimitive, TryFromPrimitive)]
pub enum EvoreAccount {
    Manager = 100,
    Deployer = 101,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct Manager {
    /// The authority of this managed miner account. Which is authority of all 
    /// associated auth_id's miners
    pub authority: Pubkey,
}

account!(EvoreAccount, Manager);

/// Deployer account - allows a deploy_authority to execute deploys on behalf of a manager
/// PDA seeds: ["deployer", manager_key]
/// Stores manager_key for easy lookup when scanning by deploy_authority
/// The deployer charges fees on each deployment (both fees are applied if > 0)
/// 
/// expected_bps_fee and expected_flat_fee provide deploy_authority protection.
/// If expected fee > 0, the actual fee must match for the deploy to succeed.
/// Size: 32 + 32 + 8 + 8 + 8 + 8 + 8 = 104 bytes (+ 8 discriminator = 112)
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct Deployer {
    /// The manager account this deployer is for (needed for PDA derivation lookups)
    pub manager_key: Pubkey,
    /// The authority that can execute deploys via this deployer
    pub deploy_authority: Pubkey,
    /// Actual percentage fee in basis points (1000 = 10%, etc.) - set by deploy_authority
    /// Must be <= expected_bps_fee for autodeploys to succeed
    pub bps_fee: u64,
    /// Actual flat fee in lamports - set by deploy_authority
    /// Must be <= expected_flat_fee for autodeploys to succeed
    pub flat_fee: u64,
    /// Maximum bps_fee the manager accepts (set by manager) - deployer can charge up to this
    pub expected_bps_fee: u64,
    /// Maximum flat_fee in lamports the manager accepts (set by manager) - deployer can charge up to this
    pub expected_flat_fee: u64,
    /// Maximum lamports to deploy per round (0 = unlimited) - set by manager
    pub max_per_round: u64,
}

account!(EvoreAccount, Deployer);

pub fn managed_miner_auth_pda(manager: Pubkey, auth_id: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[MANAGED_MINER_AUTH, &manager.to_bytes(), &auth_id.to_le_bytes()], &crate::ID)
}

/// Derives the deployer PDA for a given manager key
/// Seeds: ["deployer", manager_key]
pub fn deployer_pda(manager_key: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[DEPLOYER, &manager_key.to_bytes()], &crate::ID)
}
