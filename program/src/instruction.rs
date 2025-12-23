use spl_associated_token_account::get_associated_token_address;
use steel::*;

use crate::{consts::FEE_COLLECTOR, entropy_api, ore_api::{self, automation_pda, board_pda, config_pda, miner_pda, round_pda, treasury_pda}, state::{managed_miner_auth_pda, deployer_pda}};

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, TryFromPrimitive)]
pub enum Instructions {
    CreateManager = 0,
    MMDeploy = 1,
    MMCheckpoint = 2,
    MMClaimSOL = 3,
    MMClaimORE = 4,
    CreateDeployer = 5,
    UpdateDeployer = 6,
    MMAutodeploy = 7,
    DepositAutodeployBalance = 8,
    RecycleSol = 9,
    WithdrawAutodeployBalance = 10,
    MMAutocheckpoint = 11,
    MMFullAutodeploy = 12,
    TransferManager = 13,
    MMCreateMiner = 14,
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
        attempts: u64,  // Attempt counter - makes each tx unique for same blockhash
    },
    /// Percentage-based: deploy to own X% of each square across Y squares
    Percentage {
        bankroll: u64,
        percentage: u64,      // In basis points (1000 = 10%)
        squares_count: u64,   // Number of squares (1-25)
    },
    /// Manual: specify exact amounts for each of the 25 squares
    Manual {
        amounts: [u64; 25],   // Amount to deploy on each square (0 = skip)
    },
    /// Split: deploy total amount equally across all 25 squares in one CPI call
    Split {
        amount: u64,          // Total amount to split across 25 squares
    },
}

impl DeployStrategy {
    /// Strategy discriminant
    pub fn discriminant(&self) -> u8 {
        match self {
            DeployStrategy::EV { .. } => 0,
            DeployStrategy::Percentage { .. } => 1,
            DeployStrategy::Manual { .. } => 2,
            DeployStrategy::Split { .. } => 3,
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

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct TransferManager {}

instruction!(Instructions, TransferManager);

/// Transfer manager authority to a new pubkey.
/// Note: This transfers all associated mining accounts (deployer, miner, etc.)
pub fn transfer_manager(signer: Pubkey, manager: Pubkey, new_authority: Pubkey) -> Instruction {
    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(manager, false),
            AccountMeta::new_readonly(new_authority, false),
        ],
        data: TransferManager {}.to_bytes(),
    }
}

/// On-chain MMDeploy instruction data (Pod/Zeroable)
/// 
/// Layout (272 bytes total):
/// - auth_id: [u8; 8] - Manager auth ID
/// - bump: u8 - PDA bump
/// - allow_multi_deploy: u8 - If 0, fail if already deployed this round (applies to all strategies)
/// - _pad: [u8; 6] - Padding for alignment
/// - data: [u8; 256] - Strategy data where:
///   - data[0]: strategy discriminant (0 = EV, 1 = Percentage, 2 = Manual, 3 = Split)
///   
///   EV (strategy = 0):
///     - data[1..9]: bankroll
///     - data[9..17]: max_per_square
///     - data[17..25]: min_bet
///     - data[25..33]: ore_value
///     - data[33..41]: slots_left
///     - data[41..49]: attempts (makes each tx unique for same blockhash)
///   
///   Percentage (strategy = 1):
///     - data[1..9]: bankroll
///     - data[9..17]: percentage (basis points)
///     - data[17..25]: squares_count
///   
///   Manual (strategy = 2):
///     - data[1..201]: 25 x u64 amounts (one per square)
///   
///   Split (strategy = 3):
///     - data[1..9]: amount (total to split across 25 squares)
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MMDeploy {
    pub auth_id: [u8; 8],
    pub bump: u8,
    pub allow_multi_deploy: u8,
    pub _pad: [u8; 6],
    pub data: [u8; 256],
}

instruction!(Instructions, MMDeploy);

impl MMDeploy {
    /// Create MMDeploy instruction data from auth_id, bump, allow_multi_deploy, and strategy enum
    pub fn new(auth_id: u64, bump: u8, allow_multi_deploy: bool, strategy: DeployStrategy) -> Self {
        let mut data = [0u8; 256];
        
        match strategy {
            DeployStrategy::EV { bankroll, max_per_square, min_bet, ore_value, slots_left, attempts } => {
                data[0] = 0; // EV strategy
                data[1..9].copy_from_slice(&bankroll.to_le_bytes());
                data[9..17].copy_from_slice(&max_per_square.to_le_bytes());
                data[17..25].copy_from_slice(&min_bet.to_le_bytes());
                data[25..33].copy_from_slice(&ore_value.to_le_bytes());
                data[33..41].copy_from_slice(&slots_left.to_le_bytes());
                data[41..49].copy_from_slice(&attempts.to_le_bytes());
            },
            DeployStrategy::Percentage { bankroll, percentage, squares_count } => {
                data[0] = 1; // Percentage strategy
                data[1..9].copy_from_slice(&bankroll.to_le_bytes());
                data[9..17].copy_from_slice(&percentage.to_le_bytes());
                data[17..25].copy_from_slice(&squares_count.to_le_bytes());
            },
            DeployStrategy::Manual { amounts } => {
                data[0] = 2; // Manual strategy
                for (i, amount) in amounts.iter().enumerate() {
                    let start = 1 + i * 8;
                    let end = start + 8;
                    data[start..end].copy_from_slice(&amount.to_le_bytes());
                }
            },
            DeployStrategy::Split { amount } => {
                data[0] = 3; // Split strategy
                data[1..9].copy_from_slice(&amount.to_le_bytes());
            },
        }
        
        Self {
            auth_id: auth_id.to_le_bytes(),
            bump,
            allow_multi_deploy: if allow_multi_deploy { 1 } else { 0 },
            _pad: [0; 6],
            data,
        }
    }

    /// Parse the strategy from the instruction data
    pub fn get_strategy(&self) -> Result<DeployStrategy, ()> {
        let strategy = self.data[0];
        
        match strategy {
            0 => { // EV
                let bankroll = u64::from_le_bytes(self.data[1..9].try_into().unwrap());
                let max_per_square = u64::from_le_bytes(self.data[9..17].try_into().unwrap());
                let min_bet = u64::from_le_bytes(self.data[17..25].try_into().unwrap());
                let ore_value = u64::from_le_bytes(self.data[25..33].try_into().unwrap());
                let slots_left = u64::from_le_bytes(self.data[33..41].try_into().unwrap());
                let attempts = u64::from_le_bytes(self.data[41..49].try_into().unwrap());
                Ok(DeployStrategy::EV { bankroll, max_per_square, min_bet, ore_value, slots_left, attempts })
            },
            1 => { // Percentage
                let bankroll = u64::from_le_bytes(self.data[1..9].try_into().unwrap());
                let percentage = u64::from_le_bytes(self.data[9..17].try_into().unwrap());
                let squares_count = u64::from_le_bytes(self.data[17..25].try_into().unwrap());
                Ok(DeployStrategy::Percentage { bankroll, percentage, squares_count })
            },
            2 => { // Manual
                let mut amounts = [0u64; 25];
                for i in 0..25 {
                    let start = 1 + i * 8;
                    let end = start + 8;
                    amounts[i] = u64::from_le_bytes(self.data[start..end].try_into().unwrap());
                }
                Ok(DeployStrategy::Manual { amounts })
            },
            3 => { // Split
                let amount = u64::from_le_bytes(self.data[1..9].try_into().unwrap());
                Ok(DeployStrategy::Split { amount })
            },
            _ => Err(()),
        }
    }

    /// Check if allow_multi_deploy is enabled
    pub fn get_allow_multi_deploy(&self) -> bool {
        self.allow_multi_deploy != 0
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
    let config_address = config_pda().0;
    let round_address = round_pda(round_id).0;
    let entropy_var_address = entropy_api::var_pda(board_address, 0).0;

    let accounts = vec![
        AccountMeta::new(signer, true),
        AccountMeta::new(manager, false),
        AccountMeta::new(managed_miner_auth_address, false),
        AccountMeta::new(ore_miner_address.0, false),
        AccountMeta::new(FEE_COLLECTOR, false),
        AccountMeta::new(automation_address, false),
        AccountMeta::new(config_address, false),
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
    attempts: u64,
    allow_multi_deploy: bool,
) -> Instruction {
    let (accounts, bump) = build_deploy_accounts(signer, manager, auth_id, round_id);
    
    let strategy = DeployStrategy::EV {
        bankroll,
        max_per_square,
        min_bet,
        ore_value,
        slots_left,
        attempts,
    };

    Instruction {
        program_id: crate::id(),
        accounts,
        data: MMDeploy::new(auth_id, bump, allow_multi_deploy, strategy).to_bytes(),
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
    allow_multi_deploy: bool,
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
        data: MMDeploy::new(auth_id, bump, allow_multi_deploy, strategy).to_bytes(),
    }
}

/// Deploy using manual strategy - specify exact amounts for each square
pub fn manual_deploy(
    signer: Pubkey,
    manager: Pubkey,
    auth_id: u64,
    round_id: u64,
    amounts: [u64; 25],   // Amount to deploy on each square (0 = skip)
    allow_multi_deploy: bool,
) -> Instruction {
    let (accounts, bump) = build_deploy_accounts(signer, manager, auth_id, round_id);
    
    let strategy = DeployStrategy::Manual { amounts };

    Instruction {
        program_id: crate::id(),
        accounts,
        data: MMDeploy::new(auth_id, bump, allow_multi_deploy, strategy).to_bytes(),
    }
}

/// Deploy using split strategy - split total amount equally across all 25 squares in one CPI call
pub fn split_deploy(
    signer: Pubkey,
    manager: Pubkey,
    auth_id: u64,
    round_id: u64,
    amount: u64,          // Total amount to split across 25 squares
    allow_multi_deploy: bool,
) -> Instruction {
    let (accounts, bump) = build_deploy_accounts(signer, manager, auth_id, round_id);
    
    let strategy = DeployStrategy::Split { amount };

    Instruction {
        program_id: crate::id(),
        accounts,
        data: MMDeploy::new(auth_id, bump, allow_multi_deploy, strategy).to_bytes(),
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

// ============================================================================
// Deployer Instructions
// ============================================================================

/// CreateDeployer instruction data
/// Creates a new deployer account for a manager
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct CreateDeployer {
    /// Maximum bps fee user accepts (deployer can charge up to this)
    pub bps_fee: [u8; 8],
    /// Maximum flat fee in lamports user accepts (deployer can charge up to this)
    pub flat_fee: [u8; 8],
    /// Maximum lamports to deploy per round (0 = unlimited)
    pub max_per_round: [u8; 8],
}

instruction!(Instructions, CreateDeployer);

/// Create a deployer account for a manager
/// The manager authority signs to authorize the deployer creation
/// deploy_authority is the key that will be allowed to execute autodeploys
/// bps_fee: Max bps fee user accepts (deployer can set actual fee up to this)
/// flat_fee: Max flat fee user accepts (deployer can set actual fee up to this)
/// max_per_round: Maximum lamports to deploy per round (0 = unlimited)
pub fn create_deployer(
    signer: Pubkey,
    manager: Pubkey,
    deploy_authority: Pubkey,
    bps_fee: u64,
    flat_fee: u64,
    max_per_round: u64,
) -> Instruction {
    let (deployer_address, _bump) = deployer_pda(manager);

    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(manager, false),
            AccountMeta::new(deployer_address, false),
            AccountMeta::new_readonly(deploy_authority, false),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data: CreateDeployer {
            bps_fee: bps_fee.to_le_bytes(),
            flat_fee: flat_fee.to_le_bytes(),
            max_per_round: max_per_round.to_le_bytes(),
        }.to_bytes(),
    }
}

/// UpdateDeployer instruction data
/// Updates deployer configuration
/// - Manager authority: can update deploy_authority, expected_bps_fee, expected_flat_fee, max_per_round
/// - Deploy authority: can update deploy_authority, bps_fee, flat_fee
/// Pass current values for fields you don't want to change
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct UpdateDeployer {
    /// Actual bps fee charged (deploy_authority only, must be <= expected_bps_fee)
    pub bps_fee: [u8; 8],
    /// Actual flat fee charged (deploy_authority only, must be <= expected_flat_fee)
    pub flat_fee: [u8; 8],
    /// Max bps fee user accepts (manager only, 0 = accept any)
    pub expected_bps_fee: [u8; 8],
    /// Max flat fee user accepts (manager only, 0 = accept any)
    pub expected_flat_fee: [u8; 8],
    /// Maximum lamports to deploy per round (0 = unlimited) - manager only
    pub max_per_round: [u8; 8],
}

instruction!(Instructions, UpdateDeployer);

/// Update deployer configuration
/// - Manager authority: can update deploy_authority, expected_bps_fee, expected_flat_fee, max_per_round
/// - Deploy authority: can update deploy_authority, bps_fee, flat_fee
/// Pass current values for fields you don't want to change
pub fn update_deployer(
    signer: Pubkey,
    manager: Pubkey,
    new_deploy_authority: Pubkey,
    new_bps_fee: u64,
    new_flat_fee: u64,
    new_expected_bps_fee: u64,
    new_expected_flat_fee: u64,
    new_max_per_round: u64,
) -> Instruction {
    let (deployer_address, _bump) = deployer_pda(manager);

    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(manager, false),
            AccountMeta::new(deployer_address, false),
            AccountMeta::new_readonly(new_deploy_authority, false),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data: UpdateDeployer {
            bps_fee: new_bps_fee.to_le_bytes(),
            flat_fee: new_flat_fee.to_le_bytes(),
            expected_bps_fee: new_expected_bps_fee.to_le_bytes(),
            expected_flat_fee: new_expected_flat_fee.to_le_bytes(),
            max_per_round: new_max_per_round.to_le_bytes(),
        }.to_bytes(),
    }
}

/// MMAutodeploy instruction data
/// A simplified deploy wrapper for third-party deployers
/// Funds come from managed_miner_auth directly
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MMAutodeploy {
    /// Auth ID
    pub auth_id: [u8; 8],
    /// Amount to deploy per square
    pub amount: [u8; 8],
    /// Bitmask of squares to deploy to
    pub squares_mask: [u8; 4],
    /// Padding for alignment
    pub _pad: [u8; 4],
}

instruction!(Instructions, MMAutodeploy);

/// Build autodeploy accounts
fn build_autodeploy_accounts(
    signer: Pubkey,
    manager: Pubkey,
    auth_id: u64,
    round_id: u64,
) -> Vec<AccountMeta> {
    let (managed_miner_auth_address, _) = managed_miner_auth_pda(manager, auth_id);
    let (deployer_address, _) = deployer_pda(manager);
    let ore_miner_address = miner_pda(managed_miner_auth_address);

    let automation_address = automation_pda(managed_miner_auth_address).0;
    let board_address = board_pda().0;
    let config_address = config_pda().0;
    let round_address = round_pda(round_id).0;
    let entropy_var_address = entropy_api::var_pda(board_address, 0).0;

    vec![
        AccountMeta::new(signer, true),                           // 0: deploy_authority (signer)
        AccountMeta::new(manager, false),                         // 1: manager
        AccountMeta::new(deployer_address, false),                // 2: deployer PDA
        AccountMeta::new(managed_miner_auth_address, false),      // 3: managed_miner_auth PDA (funds source)
        AccountMeta::new(ore_miner_address.0, false),             // 4: ore_miner
        AccountMeta::new(FEE_COLLECTOR, false),                   // 5: fee_collector
        AccountMeta::new(automation_address, false),              // 6: automation
        AccountMeta::new(config_address, false),                  // 7: config
        AccountMeta::new(board_address, false),                   // 8: board
        AccountMeta::new(round_address, false),                   // 9: round
        AccountMeta::new(entropy_var_address, false),             // 10: entropy_var
        AccountMeta::new_readonly(ore_api::id(), false),          // 11: ore_program
        AccountMeta::new_readonly(entropy_api::id(), false),      // 12: entropy_program
        AccountMeta::new_readonly(system_program::id(), false),   // 13: system_program
    ]
}

/// Deploy using autodeploy (via deployer)
/// Funds are taken from managed_miner_auth directly
pub fn mm_autodeploy(
    signer: Pubkey,
    manager: Pubkey,
    auth_id: u64,
    round_id: u64,
    amount: u64,
    squares_mask: u32,
) -> Instruction {
    let accounts = build_autodeploy_accounts(signer, manager, auth_id, round_id);

    Instruction {
        program_id: crate::id(),
        accounts,
        data: MMAutodeploy {
            auth_id: auth_id.to_le_bytes(),
            amount: amount.to_le_bytes(),
            squares_mask: squares_mask.to_le_bytes(),
            _pad: [0; 4],
        }.to_bytes(),
    }
}

// ============================================================================
// Autodeploy Balance Instructions
// ============================================================================

/// DepositAutodeployBalance instruction data
/// Deposits SOL into the managed_miner_auth PDA for a specific miner
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct DepositAutodeployBalance {
    /// Auth ID of the managed miner
    pub auth_id: [u8; 8],
    /// Amount to deposit in lamports
    pub amount: [u8; 8],
}

instruction!(Instructions, DepositAutodeployBalance);

/// Deposit SOL into the managed_miner_auth PDA
/// Only the manager authority can deposit
pub fn deposit_autodeploy_balance(
    signer: Pubkey,
    manager: Pubkey,
    auth_id: u64,
    amount: u64,
) -> Instruction {
    let (managed_miner_auth_address, _) = managed_miner_auth_pda(manager, auth_id);

    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new(signer, true),                      // 0: signer (manager authority)
            AccountMeta::new(manager, false),                    // 1: manager
            AccountMeta::new(managed_miner_auth_address, false), // 2: managed_miner_auth PDA
            AccountMeta::new_readonly(system_program::id(), false), // 3: system_program
        ],
        data: DepositAutodeployBalance {
            auth_id: auth_id.to_le_bytes(),
            amount: amount.to_le_bytes(),
        }.to_bytes(),
    }
}

/// RecycleSol instruction data
/// Claims SOL from miner account (stays in managed_miner_auth)
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct RecycleSol {
    /// Auth ID of the managed miner
    pub auth_id: [u8; 8],
}

instruction!(Instructions, RecycleSol);

/// Recycle SOL from a miner account (claim SOL rewards, stays in managed_miner_auth)
/// Can be called by deploy_authority
pub fn recycle_sol(
    signer: Pubkey,
    manager: Pubkey,
    auth_id: u64,
) -> Instruction {
    let (managed_miner_auth_address, _) = managed_miner_auth_pda(manager, auth_id);
    let ore_miner_address = miner_pda(managed_miner_auth_address);
    let (deployer_address, _) = deployer_pda(manager);

    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new(signer, true),                      // 0: signer (deploy_authority)
            AccountMeta::new(manager, false),                    // 1: manager
            AccountMeta::new(deployer_address, false),           // 2: deployer PDA
            AccountMeta::new(managed_miner_auth_address, false), // 3: managed_miner_auth PDA
            AccountMeta::new(ore_miner_address.0, false),        // 4: ore_miner
            AccountMeta::new_readonly(ore_api::id(), false),     // 5: ore_program
        ],
        data: RecycleSol {
            auth_id: auth_id.to_le_bytes(),
        }.to_bytes(),
    }
}

/// WithdrawAutodeployBalance instruction data
/// Withdraws SOL from the managed_miner_auth PDA to the manager authority
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct WithdrawAutodeployBalance {
    /// Auth ID of the managed miner
    pub auth_id: [u8; 8],
    /// Amount to withdraw in lamports
    pub amount: [u8; 8],
}

instruction!(Instructions, WithdrawAutodeployBalance);

/// Withdraw SOL from the managed_miner_auth PDA
/// Only the manager authority can withdraw, and only to themselves
pub fn withdraw_autodeploy_balance(
    signer: Pubkey,
    manager: Pubkey,
    auth_id: u64,
    amount: u64,
) -> Instruction {
    let (managed_miner_auth_address, _) = managed_miner_auth_pda(manager, auth_id);

    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new(signer, true),                      // 0: signer (manager authority, also recipient)
            AccountMeta::new(manager, false),                    // 1: manager
            AccountMeta::new(managed_miner_auth_address, false), // 2: managed_miner_auth PDA
            AccountMeta::new_readonly(system_program::id(), false), // 3: system_program
        ],
        data: WithdrawAutodeployBalance {
            auth_id: auth_id.to_le_bytes(),
            amount: amount.to_le_bytes(),
        }.to_bytes(),
    }
}

// =============================================================================
// MMAutocheckpoint - Checkpoint callable by deploy_authority
// =============================================================================

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MMAutocheckpoint {
    pub auth_id: [u8; 8],
    pub bump: u8,
}

instruction!(Instructions, MMAutocheckpoint);

/// Create an MMAutocheckpoint instruction
/// 
/// Similar to MMCheckpoint but can be called by deploy_authority instead of manager authority.
/// This allows the autodeploy crank to checkpoint before deploying.
pub fn mm_autocheckpoint(
    signer: Pubkey,
    manager: Pubkey,
    round_id: u64,
    auth_id: u64,
) -> Instruction {
    let (deployer_address, _) = deployer_pda(manager);
    let (managed_miner_auth_address, bump) = managed_miner_auth_pda(manager, auth_id);
    let ore_miner_address = miner_pda(managed_miner_auth_address);
    let treasury_address = ore_api::TREASURY_ADDRESS;
    let board_address = board_pda();
    let round_address = round_pda(round_id);

    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new(signer, true),                           // 0: deploy_authority (signer)
            AccountMeta::new(manager, false),                         // 1: manager
            AccountMeta::new(deployer_address, false),                // 2: deployer PDA
            AccountMeta::new(managed_miner_auth_address, false),      // 3: managed_miner_auth PDA
            AccountMeta::new(ore_miner_address.0, false),             // 4: ore_miner
            AccountMeta::new(treasury_address, false),                // 5: treasury
            AccountMeta::new(board_address.0, false),                 // 6: board
            AccountMeta::new(round_address.0, false),                 // 7: round
            AccountMeta::new_readonly(system_program::id(), false),   // 8: system_program
            AccountMeta::new_readonly(ore_api::id(), false),          // 9: ore_program
        ],
        data: MMAutocheckpoint {
            auth_id: auth_id.to_le_bytes(),
            bump,
        }.to_bytes(),
    }
}

// ============================================================================
// DeployerData Instructions
// ============================================================================
// ============================================================================
// MMFullAutodeploy Instruction
// ============================================================================

/// MMFullAutodeploy instruction data
/// Combined checkpoint (if needed) + recycle (if needed) + deploy in one instruction.
/// Uses find_program_address for PDA validation, reads fees from Deployer account.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MMFullAutodeploy {
    /// Auth ID for the managed miner
    pub auth_id: [u8; 8],
    /// Amount to deploy per selected square
    pub amount: [u8; 8],
    /// Bitmask of squares to deploy to (each bit = one square, 25 bits used)
    pub squares_mask: [u8; 4],
    /// Padding for alignment
    pub _pad: [u8; 4],
}

instruction!(Instructions, MMFullAutodeploy);

/// Build accounts list for mm_full_autodeploy (16 accounts)
fn build_full_autodeploy_accounts(
    signer: Pubkey,
    manager: Pubkey,
    auth_id: u64,
    round_id: u64,
    checkpoint_round_id: u64,
) -> Vec<AccountMeta> {
    let (deployer_address, _) = deployer_pda(manager);
    let (managed_miner_auth_address, _) = managed_miner_auth_pda(manager, auth_id);
    let ore_miner_address = miner_pda(managed_miner_auth_address);
    let automation_address = automation_pda(managed_miner_auth_address).0;
    let board_address = board_pda().0;
    let config_address = config_pda().0;
    let round_address = round_pda(round_id).0;
    let checkpoint_round_address = round_pda(checkpoint_round_id).0;
    let treasury_address = ore_api::TREASURY_ADDRESS;
    let entropy_var_address = entropy_api::var_pda(board_address, 0).0;

    vec![
        AccountMeta::new(signer, true),                           // 0: deploy_authority (signer)
        AccountMeta::new(manager, false),                         // 1: manager
        AccountMeta::new(deployer_address, false),                // 2: deployer PDA
        AccountMeta::new(managed_miner_auth_address, false),      // 3: managed_miner_auth PDA (funds source)
        AccountMeta::new(ore_miner_address.0, false),             // 4: ore_miner
        AccountMeta::new(FEE_COLLECTOR, false),                   // 5: fee_collector
        AccountMeta::new(automation_address, false),              // 6: automation
        AccountMeta::new(config_address, false),                  // 7: config
        AccountMeta::new(board_address, false),                   // 8: board
        AccountMeta::new(round_address, false),                   // 9: round (current round for deploy)
        AccountMeta::new(checkpoint_round_address, false),        // 10: checkpoint_round (for checkpoint CPI)
        AccountMeta::new(treasury_address, false),                // 11: treasury (for checkpoint)
        AccountMeta::new(entropy_var_address, false),             // 12: entropy_var
        AccountMeta::new_readonly(ore_api::id(), false),          // 13: ore_program
        AccountMeta::new_readonly(entropy_api::id(), false),      // 14: entropy_program
        AccountMeta::new_readonly(system_program::id(), false),   // 15: system_program
    ]
}

/// Combined checkpoint + recycle + deploy in one instruction
/// 
/// This instruction:
/// 1. Checks if checkpoint is needed, does it using checkpoint_round
/// 2. Checks if recycle is needed, does it inline
/// 3. Deploys to the specified squares using current round
/// 
/// Uses find_program_address for PDA validation, reads fees directly from Deployer account.
/// 
/// Args:
/// - auth_id: Auth ID for the managed miner
/// - round_id: Current round ID for deploying
/// - checkpoint_round_id: Round ID that needs checkpointing (usually round_id - 1, or same as round_id if no checkpoint needed)
pub fn mm_full_autodeploy(
    signer: Pubkey,
    manager: Pubkey,
    auth_id: u64,
    round_id: u64,
    checkpoint_round_id: u64,
    amount: u64,
    squares_mask: u32,
) -> Instruction {
    let accounts = build_full_autodeploy_accounts(signer, manager, auth_id, round_id, checkpoint_round_id);

    Instruction {
        program_id: crate::id(),
        accounts,
        data: MMFullAutodeploy {
            auth_id: auth_id.to_le_bytes(),
            amount: amount.to_le_bytes(),
            squares_mask: squares_mask.to_le_bytes(),
            _pad: [0; 4],
        }.to_bytes(),
    }
}

// ============================================================================
// MMCreateMiner Instruction
// ============================================================================

/// MMCreateMiner instruction data
/// Creates an ORE miner account by CPIing to automate twice (open then close)
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MMCreateMiner {
    pub auth_id: [u8; 8],
    pub bump: u8,
}

instruction!(Instructions, MMCreateMiner);

/// Create an ORE miner account for a managed miner authority.
/// This CPIs to ORE's automate instruction twice:
/// 1. First with executor = signer (opens automation, creates miner)
/// 2. Second with executor = Pubkey::default() (closes automation)
/// 
/// Note: executor_2 = Pubkey::default() = system_program::id()
/// We use readonly for executor_2 to avoid privilege conflicts with system_program.
/// ORE doesn't actually check that executor is writable.
pub fn mm_create_miner(signer: Pubkey, manager: Pubkey, auth_id: u64) -> Instruction {
    let (managed_miner_auth_address, bump) = managed_miner_auth_pda(manager, auth_id);
    let automation_address = automation_pda(managed_miner_auth_address).0;
    let miner_address = miner_pda(managed_miner_auth_address).0;

    let executor_1 = signer;
    let executor_2 = Pubkey::default();

    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(manager, false),
            AccountMeta::new(managed_miner_auth_address, false),
            AccountMeta::new(automation_address, false),
            AccountMeta::new(miner_address, false),
            AccountMeta::new(executor_1, false),
            AccountMeta::new_readonly(executor_2, false), // readonly to match system_program
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(ore_api::id(), false),
        ],
        data: MMCreateMiner { auth_id: auth_id.to_le_bytes(), bump }.to_bytes(),
    }
}
