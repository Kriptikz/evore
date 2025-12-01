use spl_associated_token_account::get_associated_token_address;
use steel::*;

use crate::{consts::FEE_COLLECTOR, entropy_api, ore_api::{self, automation_pda, board_pda, miner_pda, round_pda, treasury_pda}, state::managed_miner_auth_pda};

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, TryFromPrimitive)]
pub enum Instructions {
    CreateManager = 0,
    MMDeploy = 1,
    MMCheckpoint = 2,
    MMClaimSOL = 3,
    MMClaimORE = 4,
}

/// Deployment strategy enum with associated data
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeployStrategy {
    /// EV-based waterfill algorithm - calculates optimal +EV deployments
    EV {
        bankroll: u64,
        max_per_square: u64,
        min_bet: u64,
        ore_value: u64,
        slots_left: u64,
    },
    /// Percentage-based: deploy to own X% of each square across Y squares
    Percentage {
        bankroll: u64,
        percentage: u64,      // In basis points (1000 = 10%)
        squares_count: u64,   // Number of squares (1-25)
    },
    // Manual { ... },  // Future
}

impl DeployStrategy {
    /// Strategy discriminant
    pub fn discriminant(&self) -> u8 {
        match self {
            DeployStrategy::EV { .. } => 0,
            DeployStrategy::Percentage { .. } => 1,
        }
    }
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

/// On-chain MMDeploy instruction data (Pod/Zeroable)
/// 
/// Layout (64 bytes total for Pod alignment):
/// - auth_id: [u8; 8] - Manager auth ID
/// - bump: u8 - PDA bump
/// - _pad: [u8; 7] - Padding for alignment
/// - data: [u8; 48] - Strategy data where:
///   - data[0]: strategy discriminant (0 = EV, 1 = Percentage)
///   - data[1..9]: bankroll
///   - data[9..17]: max_per_square (EV) or percentage (Percentage)
///   - data[17..25]: min_bet (EV) or squares_count (Percentage)
///   - data[25..33]: ore_value (EV only)
///   - data[33..41]: slots_left (EV only)
///   - data[41..48]: unused padding
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MMDeploy {
    pub auth_id: [u8; 8],
    pub bump: u8,
    pub _pad: [u8; 7],
    pub data: [u8; 48],  // [strategy(1), bankroll(8), param1(8), param2(8), param3(8), param4(8), padding(7)]
}

instruction!(Instructions, MMDeploy);

impl MMDeploy {
    /// Create MMDeploy instruction data from auth_id, bump, and strategy enum
    pub fn new(auth_id: u64, bump: u8, strategy: DeployStrategy) -> Self {
        let mut data = [0u8; 48];
        
        match strategy {
            DeployStrategy::EV { bankroll, max_per_square, min_bet, ore_value, slots_left } => {
                data[0] = 0; // EV strategy
                data[1..9].copy_from_slice(&bankroll.to_le_bytes());
                data[9..17].copy_from_slice(&max_per_square.to_le_bytes());
                data[17..25].copy_from_slice(&min_bet.to_le_bytes());
                data[25..33].copy_from_slice(&ore_value.to_le_bytes());
                data[33..41].copy_from_slice(&slots_left.to_le_bytes());
            },
            DeployStrategy::Percentage { bankroll, percentage, squares_count } => {
                data[0] = 1; // Percentage strategy
                data[1..9].copy_from_slice(&bankroll.to_le_bytes());
                data[9..17].copy_from_slice(&percentage.to_le_bytes());
                data[17..25].copy_from_slice(&squares_count.to_le_bytes());
                // data[25..] remains zero (unused)
            },
        }
        
        Self {
            auth_id: auth_id.to_le_bytes(),
            bump,
            _pad: [0; 7],
            data,
        }
    }

    /// Parse the strategy from the instruction data
    pub fn get_strategy(&self) -> Result<DeployStrategy, ()> {
        let strategy = self.data[0];
        let bankroll = u64::from_le_bytes(self.data[1..9].try_into().unwrap());
        
        match strategy {
            0 => {
                let max_per_square = u64::from_le_bytes(self.data[9..17].try_into().unwrap());
                let min_bet = u64::from_le_bytes(self.data[17..25].try_into().unwrap());
                let ore_value = u64::from_le_bytes(self.data[25..33].try_into().unwrap());
                let slots_left = u64::from_le_bytes(self.data[33..41].try_into().unwrap());
                Ok(DeployStrategy::EV { bankroll, max_per_square, min_bet, ore_value, slots_left })
            },
            1 => {
                let percentage = u64::from_le_bytes(self.data[9..17].try_into().unwrap());
                let squares_count = u64::from_le_bytes(self.data[17..25].try_into().unwrap());
                Ok(DeployStrategy::Percentage { bankroll, percentage, squares_count })
            },
            _ => Err(()),
        }
    }
}

/// Build deploy accounts (shared by all strategies)
fn build_deploy_accounts(
    signer: Pubkey,
    manager: Pubkey,
    auth_id: u64,
    round_id: u64,
) -> (Vec<AccountMeta>, u8) {
    let (managed_miner_auth_address, bump) = managed_miner_auth_pda(manager, auth_id);
    let ore_miner_address = miner_pda(managed_miner_auth_address);

    let authority = managed_miner_auth_address;
    let automation_address = automation_pda(authority).0;
    let board_address = board_pda().0;
    let round_address = round_pda(round_id).0;
    let entropy_var_address = entropy_api::var_pda(board_address, 0).0;

    let accounts = vec![
        AccountMeta::new(signer, true),
        AccountMeta::new(manager, false),
        AccountMeta::new(managed_miner_auth_address, false),
        AccountMeta::new(ore_miner_address.0, false),
        AccountMeta::new(FEE_COLLECTOR, false),
        AccountMeta::new(automation_address, false),
        AccountMeta::new(board_address, false),
        AccountMeta::new(round_address, false),
        AccountMeta::new(entropy_var_address, false),
        AccountMeta::new_readonly(ore_api::id(), false),
        AccountMeta::new_readonly(entropy_api::id(), false),
        AccountMeta::new_readonly(system_program::id(), false),
    ];

    (accounts, bump)
}

/// Deploy using EV strategy
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
    let (accounts, bump) = build_deploy_accounts(signer, manager, auth_id, round_id);
    
    let strategy = DeployStrategy::EV {
        bankroll,
        max_per_square,
        min_bet,
        ore_value,
        slots_left,
    };

    Instruction {
        program_id: crate::id(),
        accounts,
        data: MMDeploy::new(auth_id, bump, strategy).to_bytes(),
    }
}

/// Deploy using percentage strategy - own X% of each square across Y squares
pub fn percentage_deploy(
    signer: Pubkey,
    manager: Pubkey,
    auth_id: u64,
    round_id: u64,
    bankroll: u64,
    percentage: u64,      // In basis points (1000 = 10%)
    squares_count: u64,   // Number of squares (1-25)
) -> Instruction {
    let (accounts, bump) = build_deploy_accounts(signer, manager, auth_id, round_id);
    
    let strategy = DeployStrategy::Percentage {
        bankroll,
        percentage,
        squares_count,
    };

    Instruction {
        program_id: crate::id(),
        accounts,
        data: MMDeploy::new(auth_id, bump, strategy).to_bytes(),
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MMCheckpoint {
    pub auth_id: [u8; 8],
    pub bump: u8,
}

instruction!(Instructions, MMCheckpoint);

pub fn mm_checkpoint(signer: Pubkey, manager: Pubkey, round_id: u64, auth_id: u64) -> Instruction {
    let (managed_miner_auth_address, bump) = managed_miner_auth_pda(manager, auth_id);
    let ore_miner_address = miner_pda(managed_miner_auth_address);
    let treasury_address = ore_api::TREASURY_ADDRESS;

    let board_address = board_pda();
    let round_address = round_pda(round_id);

    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(manager, false),
            AccountMeta::new(managed_miner_auth_address, false),
            AccountMeta::new(ore_miner_address.0, false),
            AccountMeta::new(treasury_address, false),
            AccountMeta::new(board_address.0, false),
            AccountMeta::new(round_address.0, false),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(ore_api::id(), false),
        ],
        data: MMCheckpoint {
            auth_id: auth_id.to_le_bytes(),
            bump,
        }.to_bytes(),
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MMClaimSOL {
    pub auth_id: [u8; 8],
    pub bump: u8,
}

instruction!(Instructions, MMClaimSOL);

pub fn mm_claim_sol(signer: Pubkey, manager: Pubkey, auth_id: u64) -> Instruction {
    let (managed_miner_auth_address, bump) = managed_miner_auth_pda(manager, auth_id);
    let ore_miner_address = miner_pda(managed_miner_auth_address);

    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(manager, false),
            AccountMeta::new(managed_miner_auth_address, false),
            AccountMeta::new(ore_miner_address.0, false),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(ore_api::id(), false),
        ],
        data: MMClaimSOL {
            auth_id: auth_id.to_le_bytes(),
            bump,
        }.to_bytes(),
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MMClaimORE {
    pub auth_id: [u8; 8],
    pub bump: u8,
}

instruction!(Instructions, MMClaimORE);

pub fn mm_claim_ore(signer: Pubkey, manager: Pubkey, auth_id: u64) -> Instruction {
    let (managed_miner_auth_address, bump) = managed_miner_auth_pda(manager, auth_id);
    let ore_miner_address = miner_pda(managed_miner_auth_address);
    let treasury_address = treasury_pda().0;
    let treasury_tokens_address = get_associated_token_address(&treasury_address, &ore_api::MINT_ADDRESS);
    let recipient_address = get_associated_token_address(&managed_miner_auth_address, &ore_api::MINT_ADDRESS);
    let signer_recipient_address = get_associated_token_address(&signer, &ore_api::MINT_ADDRESS);

    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(manager, false),
            AccountMeta::new(managed_miner_auth_address, false),
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
            bump,
        }.to_bytes(),
    }
}
