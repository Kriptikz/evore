use spl_associated_token_account::get_associated_token_address;
use steel::*;

use crate::{consts::FEE_COLLECTOR, entropy_api, ore_api::{self, automation_pda, board_pda, miner_pda, round_pda, treasury_pda}, state::managed_miner_auth_pda};

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, TryFromPrimitive)]
pub enum Instructions {
    CreateManager = 0,
    EvDeploy = 1,
    MMCheckpoint = 2,
    MMClaimSOL = 3,
    MMClaimORE = 4,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct CreateManager {}

instruction!(Instructions, CreateManager);

pub fn create_manager(signer: Pubkey, manager: Pubkey) -> Instruction {
    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(manager, true),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data: CreateManager {}.to_bytes(),
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct EvDeploy {
    pub auth_id: [u8; 8],
    pub bankroll: [u8; 8],
    pub max_per_square: [u8; 8],
    pub min_bet: [u8; 8],
    pub ore_value: [u8; 8],
    pub slots_left: [u8; 8],
}

instruction!(Instructions, EvDeploy);

pub fn ev_deploy(
    signer: Pubkey,
    manager: Pubkey,
    auth_id: u64,
    round_id: u64,
    bankroll: u64,
    max_per_square: u64,
    min_bet: u64,
    ore_value: u64,
    slots_left: u64,

) -> Instruction {
    let managed_miner_auth_address = managed_miner_auth_pda(manager, auth_id);
    let ore_miner_address = miner_pda(managed_miner_auth_address.0);

    let authority = managed_miner_auth_address.0;
    let automation_address = automation_pda(authority).0;
    let board_address = board_pda().0;
    let round_address = round_pda(round_id).0;
    let entropy_var_address = entropy_api::var_pda(board_address, 0).0;

    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(manager, false),
            AccountMeta::new(managed_miner_auth_address.0, false),
            AccountMeta::new(ore_miner_address.0, false),
            AccountMeta::new(FEE_COLLECTOR, false),
            AccountMeta::new(automation_address, false),
            AccountMeta::new(board_address, false),
            AccountMeta::new(round_address, false),
            AccountMeta::new(entropy_var_address, false),
            AccountMeta::new_readonly(ore_api::id(), false),
            AccountMeta::new_readonly(entropy_api::id(), false),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data: EvDeploy {
            auth_id: auth_id.to_le_bytes(),
            bankroll: bankroll.to_le_bytes(),
            max_per_square: max_per_square.to_le_bytes(),
            min_bet: min_bet.to_le_bytes(),
            ore_value: ore_value.to_le_bytes(),
            slots_left: slots_left.to_le_bytes(),
        }.to_bytes(),
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MMCheckpoint {
    pub auth_id: [u8; 8],
}

instruction!(Instructions, MMCheckpoint);

pub fn mm_checkpoint(signer: Pubkey, manager: Pubkey, round_id: u64, auth_id: u64) -> Instruction {
    let managed_miner_auth_address = managed_miner_auth_pda(manager, auth_id);
    let ore_miner_address = miner_pda(managed_miner_auth_address.0);
    let treasury_address = ore_api::TREASURY_ADDRESS;

    let board_address = board_pda();
    let round_address = round_pda(round_id);

    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(manager, false),
            AccountMeta::new(managed_miner_auth_address.0, false),
            AccountMeta::new(ore_miner_address.0, false),
            AccountMeta::new(treasury_address, false),
            AccountMeta::new(board_address.0, false),
            AccountMeta::new(round_address.0, false),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(ore_api::id(), false),
        ],
        data: MMCheckpoint {
            auth_id: auth_id.to_le_bytes(),
        }.to_bytes(),
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MMClaimSOL {
    pub auth_id: [u8; 8],
}

instruction!(Instructions, MMClaimSOL);

pub fn mm_claim_sol(signer: Pubkey, manager: Pubkey, auth_id: u64) -> Instruction {
    let managed_miner_auth_address = managed_miner_auth_pda(manager, auth_id);
    let ore_miner_address = miner_pda(managed_miner_auth_address.0);

    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(manager, false),
            AccountMeta::new(managed_miner_auth_address.0, false),
            AccountMeta::new(ore_miner_address.0, false),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(ore_api::id(), false),
        ],
        data: MMClaimSOL {
            auth_id: auth_id.to_le_bytes(),
        }.to_bytes(),
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MMClaimORE {
    pub auth_id: [u8; 8],
}
instruction!(Instructions, MMClaimORE);

pub fn mm_claim_ore(signer: Pubkey, manager: Pubkey, auth_id: u64) -> Instruction {
    let managed_miner_auth_address = managed_miner_auth_pda(manager, auth_id);
    let ore_miner_address = miner_pda(managed_miner_auth_address.0);
    let treasury_address = treasury_pda().0;
    let treasury_tokens_address = get_associated_token_address(&treasury_address, &ore_api::MINT_ADDRESS);
    let recipient_address = get_associated_token_address(&managed_miner_auth_address.0, &ore_api::MINT_ADDRESS);
    let signer_recipient_address = get_associated_token_address(&signer, &ore_api::MINT_ADDRESS);

    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(manager, false),
            AccountMeta::new(managed_miner_auth_address.0, false),
            AccountMeta::new(ore_miner_address.0, false),
            AccountMeta::new(ore_api::MINT_ADDRESS, false),
            AccountMeta::new(recipient_address, false),
            AccountMeta::new(signer_recipient_address, false),
            AccountMeta::new(treasury_address, false),
            AccountMeta::new(treasury_tokens_address, false),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(spl_associated_token_account::id(), false),
            AccountMeta::new_readonly(ore_api::id(), false),
        ],
        data: MMClaimORE {
            auth_id: auth_id.to_le_bytes(),
        }.to_bytes(),
    }
}
