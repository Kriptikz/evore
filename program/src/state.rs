use steel::*;
use serde::{Serialize, Deserialize};

use crate::consts::{MANAGED_MINER_AUTH};

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoPrimitive, TryFromPrimitive)]
pub enum EvoreAccount {
    Manager = 100,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct Manager {
    /// The authority of this managed miner account. Which is authority of all 
    /// associated auth_id's miners
    pub authority: Pubkey,
}

account!(EvoreAccount, Manager);

pub fn managed_miner_auth_pda(manager: Pubkey, auth_id: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[MANAGED_MINER_AUTH, &manager.to_bytes(), &auth_id.to_le_bytes()], &crate::ID)
}
