use steel::*;
use serde::{Serialize, Deserialize};

use crate::consts::{MANAGED_MINER_AUTH, DEPLOYER, AUTODEPLOY_BALANCE};

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
/// The deployer charges a fee (in basis points) on each deployment
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct Deployer {
    /// The manager account this deployer is for (needed for PDA derivation lookups)
    pub manager_key: Pubkey,
    /// The authority that can execute deploys via this deployer
    pub deploy_authority: Pubkey,
    /// Fee in basis points (1000 = 10%, 500 = 5%, etc.)
    pub fee_bps: u64,
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

/// Derives the autodeploy balance PDA for a given deployer
/// This is a 0-byte PDA that holds SOL for autodeploys
/// Seeds: ["autodeploy-balance", deployer_key]
pub fn autodeploy_balance_pda(deployer_key: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[AUTODEPLOY_BALANCE, &deployer_key.to_bytes()], &crate::ID)
}
