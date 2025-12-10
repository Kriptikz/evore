use solana_program::pubkey;
use serde::{Serialize, Deserialize};
use spl_associated_token_account::get_associated_token_address;
use steel::*;

use crate::entropy_api;

pub const PROGRAM_ID: Pubkey = pubkey!("oreV3EG1i9BEgiAJ8b177Z2S2rMarzak4NMv1kULvWv");

/// The seed of the board account PDA.
pub const BOARD: &[u8] = b"board";

/// The seed of the miner account PDA.
pub const MINER: &[u8] = b"miner";

/// The seed of the round account PDA.
pub const ROUND: &[u8] = b"round";

/// The seed of the config account PDA.
pub const CONFIG: &[u8] = b"config";

/// The seed of the automation account PDA.
pub const AUTOMATION: &[u8] = b"automation";

/// The seed of the treasury account PDA.
pub const TREASURY: &[u8] = b"treasury";

/// The address of the treasury account.
pub const TREASURY_ADDRESS: Pubkey = pubkey!("45db2FSR4mcXdSVVZbKbwojU6uYDpMyhpEi7cC8nHaWG");

pub const INTERMISSION_SLOTS: u64 = 35;

/// The checkpoint fee that miners must hold (in lamports)
/// This is required by ORE v3 for the checkpoint operation
pub const CHECKPOINT_FEE: u64 = 10_000; // 0.00001 SOL

/// The address of the mint account.
pub const MINT_ADDRESS: Pubkey = pubkey!("oreoU2P8bN6jkk3jbaiVxYnG1dCXcYxwhwyK9jSybcp");

pub fn id() -> Pubkey {
    PROGRAM_ID
}

pub fn board_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[BOARD], &PROGRAM_ID)
}

pub fn config_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[CONFIG], &PROGRAM_ID)
}

pub fn miner_pda(authority: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[MINER, &authority.to_bytes()], &PROGRAM_ID)
}

pub fn round_pda(id: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[ROUND, &id.to_le_bytes()], &PROGRAM_ID)
}

pub fn automation_pda(authority: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[AUTOMATION, &authority.to_bytes()], &PROGRAM_ID)
}

pub fn treasury_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[TREASURY], &PROGRAM_ID)
}

pub fn treasury_tokens_address() -> Pubkey {
    spl_associated_token_account::get_associated_token_address(&TREASURY_ADDRESS, &MINT_ADDRESS)
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoPrimitive, TryFromPrimitive)]
pub enum OreAccount {
    Automation = 100,
    Config = 101,
    Miner = 103,
    Treasury = 104,
    Board = 105,
    Stake = 108,
    Round = 109,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct Board {
    /// The current round number.
    pub round_id: u64,

    /// The slot at which the current round starts mining.
    pub start_slot: u64,

    /// The slot at which the current round ends mining.
    pub end_slot: u64,

    /// The current epoch id.
    pub epoch_id: u64,
}

impl Board {
    pub fn pda(&self) -> (Pubkey, u8) {
        board_pda()
    }
}

account!(OreAccount, Board);

/// Treasury is a singleton account which is the mint authority for the ORE token and the authority of
/// Treasury is a singleton account which is the mint authority for the ORE token and the authority of
/// the program's global token account.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct Treasury {
    // The amount of SOL collected for buy-bury operations.
    pub balance: u64,

    /// Buffer a (placeholder)
    pub buffer_a: u64,

    /// The amount of ORE in the motherlode rewards pool.
    pub motherlode: u64,

    /// The cumulative ORE distributed to miners, divided by the total unclaimed ORE at the time of distribution.
    pub miner_rewards_factor: Numeric,

    /// The cumulative ORE distributed to stakers, divided by the total stake at the time of distribution.
    pub stake_rewards_factor: Numeric,

    /// Buffer b (placeholder)
    pub buffer_b: u64,

    /// The current total amount of refined ORE mining rewards.
    pub total_refined: u64,

    /// The current total amount of ORE staking deposits.
    pub total_staked: u64,

    /// The current total amount of unclaimed ORE mining rewards.
    pub total_unclaimed: u64,
}

account!(OreAccount, Treasury);


#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct Round {
    /// The round number.
    pub id: u64,

    /// The amount of SOL deployed in each square.
    pub deployed: [u64; 25],

    /// The hash of the end slot, provided by solana, used for random number generation.
    pub slot_hash: [u8; 32],

    /// The count of miners on each square.
    pub count: [u64; 25],

    /// The slot at which claims for this round account end.
    pub expires_at: u64,

    /// The amount of ORE in the motherlode.
    pub motherlode: u64,

    /// The account to which rent should be returned when this account is closed.
    pub rent_payer: Pubkey,

    /// The top miner of the round.
    pub top_miner: Pubkey,

    /// The amount of ORE to distribute to the top miner.
    pub top_miner_reward: u64,

    /// The total amount of SOL deployed in the round.
    pub total_deployed: u64,

    /// The total number of unique miners that played in the round.
    pub total_miners: u64,

    /// The total amount of SOL put in the ORE vault.
    pub total_vaulted: u64,

    /// The total amount of SOL won by miners for the round.
    pub total_winnings: u64,
}

account!(OreAccount, Round);

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct Miner {
    /// The authority of this miner account.
    pub authority: Pubkey,

    /// The miner's prospects in the current round.
    pub deployed: [u64; 25],

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

    /// The rewards factor last time rewards were updated on this miner account.
    pub rewards_factor: Numeric,

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

    /// The total amount of ORE this miner has deployed across all rounds.
    pub lifetime_deployed: u64,
}

account!(OreAccount, Miner);

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct Config {
    /// The address that can update the config.
    pub admin: Pubkey,

    /// The adress with authority to call wrap and bury.
    pub bury_authority: Pubkey,

    /// The address that receives admin fees.
    pub fee_collector: Pubkey,

    /// The program to be used for protocol swaps.
    pub swap_program: Pubkey,

    /// The address of the entropy var account.
    pub var_address: Pubkey,

    /// Amount to pay to fee collector (bps)
    pub admin_fee: u64,
}

impl Config {
    pub fn pda() -> (Pubkey, u8) {
        config_pda()
    }
}

account!(OreAccount, Config);

pub fn deploy(
    signer: Pubkey,
    authority: Pubkey,
    amount: u64,
    round_id: u64,
    squares: [bool; 25],
) -> Instruction {
    let automation_address = automation_pda(authority).0;
    let board_address = board_pda().0;
    let miner_address = miner_pda(authority).0;
    let round_address = round_pda(round_id).0;
    let config_address = config_pda().0;
    let entropy_var_address = entropy_api::var_pda(board_address, 0).0;

    // Convert array of 25 booleans into a 32-bit mask where each bit represents whether
    // that square index is selected (1) or not (0)
    let mut mask: u32 = 0;
    for (i, &square) in squares.iter().enumerate() {
        if square {
            mask |= 1 << i;
        }
    }

    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(authority, false),
            AccountMeta::new(automation_address, false),
            AccountMeta::new(board_address, false),
            AccountMeta::new(config_address, false),
            AccountMeta::new(miner_address, false),
            AccountMeta::new(round_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(PROGRAM_ID, false),
            // Entropy accounts.
            AccountMeta::new(entropy_var_address, false),
            AccountMeta::new_readonly(entropy_api::id(), false),
        ],
        data: Deploy {
            amount: amount.to_le_bytes(),
            squares: mask.to_le_bytes(),
        }
        .to_bytes(),
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, TryFromPrimitive)]
pub enum OreInstruction {
    // Miner
    Automate = 0,
    Checkpoint = 2,
    ClaimSOL = 3,
    ClaimORE = 4,
    Close = 5,
    Deploy = 6,
    Log = 8,
    Reset = 9,
    ReloadSOL = 21,

    // Staker
    Deposit = 10,
    Withdraw = 11,
    ClaimYield = 12,

    // Admin
    Bury = 13,
    Wrap = 14,
    SetAdmin = 15,
    SetFeeCollector = 16,
    SetSwapProgram = 17,
    SetVarAddress = 18,
    NewVar = 19,
    SetAdminFee = 20,
    MigrateAutomation = 22,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Deploy {
    pub amount: [u8; 8],
    pub squares: [u8; 4],
}

instruction!(OreInstruction, Deploy);

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Checkpoint {}
instruction!(OreInstruction, Checkpoint);

pub fn checkpoint(signer: Pubkey, authority: Pubkey, round_id: u64) -> Instruction {
    let miner_address = miner_pda(authority).0;
    let board_address = board_pda().0;
    let round_address = round_pda(round_id).0;
    let treasury_address = TREASURY_ADDRESS;
    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(board_address, false),
            AccountMeta::new(miner_address, false),
            AccountMeta::new(round_address, false),
            AccountMeta::new(treasury_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: Checkpoint {}.to_bytes(),
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ClaimSOL {}
instruction!(OreInstruction, ClaimSOL);

pub fn claim_sol(signer: Pubkey) -> Instruction {
    let miner_address = miner_pda(signer).0;
    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(miner_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: ClaimSOL {}.to_bytes(),
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ClaimORE {}
instruction!(OreInstruction, ClaimORE);
pub fn claim_ore(signer: Pubkey) -> Instruction {
    let miner_address = miner_pda(signer).0;
    let treasury_address = treasury_pda().0;
    let treasury_tokens_address = get_associated_token_address(&treasury_address, &MINT_ADDRESS);
    let recipient_address = get_associated_token_address(&signer, &MINT_ADDRESS);

    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(miner_address, false),
            AccountMeta::new(MINT_ADDRESS, false),
            AccountMeta::new(recipient_address, false),
            AccountMeta::new(treasury_address, false),
            AccountMeta::new(treasury_tokens_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(spl_token::ID, false),
            AccountMeta::new_readonly(spl_associated_token_account::ID, false),
        ],
        data: ClaimORE {}.to_bytes(),
    }
}






#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Reset {}
instruction!(OreInstruction, Reset);

pub fn reset(
    signer: Pubkey,
    fee_collector: Pubkey,
    round_id: u64,
    top_miner: Pubkey,
) -> Instruction {
    let board_address = board_pda().0;
    let config_address = config_pda().0;
    let mint_address = MINT_ADDRESS;
    let round_address = round_pda(round_id).0;
    let round_next_address = round_pda(round_id + 1).0;
    let top_miner_address = miner_pda(top_miner).0;
    let treasury_address = TREASURY_ADDRESS;
    let treasury_tokens_address = treasury_tokens_address();
    let entropy_var_address = entropy_api::var_pda(board_address, 0).0;
    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(board_address, false),
            AccountMeta::new(config_address, false),
            AccountMeta::new(fee_collector, false),
            AccountMeta::new(mint_address, false),
            AccountMeta::new(round_address, false),
            AccountMeta::new(round_next_address, false),
            AccountMeta::new(top_miner_address, false),
            AccountMeta::new(treasury_address, false),
            AccountMeta::new(treasury_tokens_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(spl_token::ID, false),
            AccountMeta::new_readonly(PROGRAM_ID, false),
            AccountMeta::new_readonly(sysvar::slot_hashes::ID, false),
            // Entropy accounts.
            AccountMeta::new(entropy_var_address, false),
            AccountMeta::new_readonly(entropy_api::id(), false),
        ],
        data: Reset {}.to_bytes(),
    }
}

