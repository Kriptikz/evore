use solana_sdk::pubkey;
use steel::*;

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoPrimitive, TryFromPrimitive)]
pub enum EntropyAccount {
    Var = 0,
}


#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct Var {
    /// The creator of the variable.
    pub authority: Pubkey,

    /// The id of the variable.
    pub id: u64,

    /// The provider of the entropy data.
    pub provider: Pubkey,

    /// The commit provided by Entropy provider.
    pub commit: [u8; 32],

    /// The revealed seed.
    pub seed: [u8; 32],

    /// The slot hash
    pub slot_hash: [u8; 32],

    /// The current value of the variable.
    pub value: [u8; 32],

    /// The number of random variables remaining to be sampled.
    pub samples: u64,

    /// Whether or not the Entropy provider should automatically sample the slot hash.
    pub is_auto: u64,

    /// The slot at which the variable was opened.
    pub start_at: u64,

    /// The slot at which the variable should sample the slothash.
    pub end_at: u64,
}

pub const ORE_VAR_ADDRESS: Pubkey = pubkey!("BWCaDY96Xe4WkFq1M7UiCCRcChsJ3p51L5KrGzhxgm2E");


steel::account!(EntropyAccount, Var);
