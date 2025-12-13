use solana_program::{
    account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey, system_program,
};
use steel::*;

use crate::{
    consts::{AUTODEPLOY_BALANCE, DEPLOY_FEE, DEPLOYER, FEE_COLLECTOR, MANAGED_MINER_AUTH},
    entropy_api,
    error::EvoreError,
    instruction::MMAutodeploy,
    ore_api::{self, Board},
    state::{Deployer, Manager},
};

pub fn process_mm_autodeploy(
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> Result<(), ProgramError> {
    let args = MMAutodeploy::try_from_bytes(instruction_data)?;
    let auth_id = u64::from_le_bytes(args.auth_id);
    let amount = u64::from_le_bytes(args.amount);
    let squares_mask = u32::from_le_bytes(args.squares_mask);
    let expected_bps_fee = u64::from_le_bytes(args.expected_bps_fee);
    let expected_flat_fee = u64::from_le_bytes(args.expected_flat_fee);

    let [
        signer,                            // 0: deploy_authority (signer)
        manager_account_info,              // 1: manager
        deployer_account_info,             // 2: deployer PDA
        autodeploy_balance_account_info,   // 3: autodeploy_balance PDA (funds source)
        managed_miner_auth_account_info,   // 4: managed_miner_auth PDA
        ore_miner_account_info,            // 5: ore_miner
        fee_collector_account_info,        // 6: fee_collector
        automation_account_info,           // 7: automation
        config_account_info,               // 8: config
        board_account_info,                // 9: board
        round_account_info,                // 10: round
        entropy_var_account_info,          // 11: entropy_var
        ore_program,                       // 12: ore_program
        entropy_program,                   // 13: entropy_program
        system_program_info,               // 14: system_program
    ] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // Basic validations
    if !signer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    if *ore_program.key != ore_api::id() {
        return Err(ProgramError::IncorrectProgramId);
    }

    if *system_program_info.key != system_program::id() {
        return Err(ProgramError::IncorrectProgramId);
    }

    if *entropy_program.key != entropy_api::id() {
        return Err(ProgramError::IncorrectProgramId);
    }

    if *fee_collector_account_info.key != FEE_COLLECTOR {
        return Err(EvoreError::InvalidFeeCollector.into());
    }

    // Verify manager is initialized
    if manager_account_info.data_is_empty() {
        return Err(EvoreError::ManagerNotInitialized.into());
    }

    let _manager = manager_account_info.as_account::<Manager>(&crate::id())?;

    // Verify deployer is initialized and load it
    if deployer_account_info.data_is_empty() {
        return Err(EvoreError::DeployerNotInitialized.into());
    }

    // Verify deployer PDA
    let deployer_pda = Pubkey::create_program_address(
        &[
            DEPLOYER,
            manager_account_info.key.as_ref(),
            &[args.deployer_bump],
        ],
        &crate::id(),
    ).map_err(|_| EvoreError::InvalidPDA)?;

    if deployer_pda != *deployer_account_info.key {
        return Err(EvoreError::InvalidPDA.into());
    }

    // PDA derivation already verifies deployer belongs to this manager
    let deployer = deployer_account_info.as_account::<Deployer>(&crate::id())?;

    // Verify signer is the deploy_authority
    if deployer.deploy_authority != *signer.key {
        return Err(EvoreError::InvalidDeployAuthority.into());
    }

    // Verify expected fees match deployer configuration (if expected values > 0)
    // This ensures the user hasn't changed the fee settings since the crank read them
    if expected_bps_fee > 0 && deployer.bps_fee != expected_bps_fee {
        return Err(EvoreError::UnexpectedFee.into());
    }
    if expected_flat_fee > 0 && deployer.flat_fee != expected_flat_fee {
        return Err(EvoreError::UnexpectedFee.into());
    }

    // Verify autodeploy_balance PDA
    let autodeploy_balance_pda = Pubkey::create_program_address(
        &[
            AUTODEPLOY_BALANCE,
            deployer_account_info.key.as_ref(),
            &[args.autodeploy_balance_bump],
        ],
        &crate::id(),
    ).map_err(|_| EvoreError::InvalidPDA)?;

    if autodeploy_balance_pda != *autodeploy_balance_account_info.key {
        return Err(EvoreError::InvalidPDA.into());
    }

    // Verify managed_miner_auth PDA
    let managed_miner_auth_pda = Pubkey::create_program_address(
        &[
            MANAGED_MINER_AUTH,
            manager_account_info.key.as_ref(),
            &auth_id.to_le_bytes(),
            &[args.bump],
        ],
        &crate::id(),
    ).map_err(|_| EvoreError::InvalidPDA)?;

    if managed_miner_auth_pda != *managed_miner_auth_account_info.key {
        return Err(EvoreError::InvalidPDA.into());
    }

    // Verify board and check round hasn't ended
    let clock = Clock::get()?;
    let board = board_account_info.as_account::<Board>(&ore_api::id())?;

    if clock.slot >= board.end_slot {
        return Err(EvoreError::EndSlotReached.into());
    }

    // Convert squares_mask to [bool; 25]
    let mut squares = [false; 25];
    for i in 0..25 {
        if (squares_mask >> i) & 1 == 1 {
            squares[i] = true;
        }
    }

    // Count how many squares are being deployed to
    let num_squares = squares.iter().filter(|&&s| s).count() as u64;
    if num_squares == 0 {
        return Err(EvoreError::NoDeployments.into());
    }

    // Calculate total deployment amount
    let total_deployed = amount.saturating_mul(num_squares);

    if total_deployed == 0 {
        return Err(EvoreError::NoDeployments.into());
    }

    // Calculate deployer fee (both bps_fee and flat_fee are additive)
    // bps_fee: percentage of total deployed (1000 = 10%)
    // flat_fee: fixed lamports amount
    let bps_fee_amount = if deployer.bps_fee > 0 {
        total_deployed
            .saturating_mul(deployer.bps_fee)
            .saturating_div(10_000)
    } else {
        0
    };
    
    let deployer_fee = bps_fee_amount.saturating_add(deployer.flat_fee);

    // Protocol fee (same as regular deploy)
    let protocol_fee = DEPLOY_FEE;

    // Calculate funds needed for deployment to managed_miner_auth PDA
    // Minimum rent for 0-byte account (PDA has no data)
    const AUTH_PDA_RENT: u64 = 890_880;
    
    // Miner account rent: ORE creates miner account on first deploy
    let miner_rent = if ore_miner_account_info.data_is_empty() {
        let size = 8 + std::mem::size_of::<ore_api::Miner>();
        solana_program::rent::Rent::default().minimum_balance(size)
    } else {
        0
    };
    
    // Required balance for managed_miner_auth:
    // - AUTH_PDA_RENT: keep PDA rent-exempt
    // - CHECKPOINT_FEE: ORE checkpoint requires this
    // - total_deployed: funds for deployments
    // - miner_rent: if miner account needs creation
    let required_miner_balance = AUTH_PDA_RENT
        .saturating_add(ore_api::CHECKPOINT_FEE)
        .saturating_add(total_deployed)
        .saturating_add(miner_rent);
    
    let current_miner_balance = managed_miner_auth_account_info.lamports();
    let transfer_to_miner = required_miner_balance.saturating_sub(current_miner_balance);

    // Total funds needed from autodeploy_balance PDA
    let total_funds_needed = transfer_to_miner
        .saturating_add(deployer_fee)
        .saturating_add(protocol_fee);

    // Check autodeploy_balance has enough funds
    let autodeploy_balance = autodeploy_balance_account_info.lamports();
    if autodeploy_balance < total_funds_needed {
        return Err(EvoreError::InsufficientAutodeployBalance.into());
    }

    // Autodeploy balance PDA seeds for signed transfers
    let autodeploy_balance_seeds: &[&[u8]] = &[
        AUTODEPLOY_BALANCE,
        deployer_account_info.key.as_ref(),
        &[args.autodeploy_balance_bump],
    ];

    // Transfer protocol fee from autodeploy_balance to FEE_COLLECTOR
    if protocol_fee > 0 {
        solana_program::program::invoke_signed(
            &solana_program::system_instruction::transfer(
                autodeploy_balance_account_info.key,
                fee_collector_account_info.key,
                protocol_fee,
            ),
            &[
                autodeploy_balance_account_info.clone(),
                fee_collector_account_info.clone(),
                system_program_info.clone(),
            ],
            &[autodeploy_balance_seeds],
        )?;
    }

    // Transfer deployer fee from autodeploy_balance to deploy_authority (signer)
    if deployer_fee > 0 {
        solana_program::program::invoke_signed(
            &solana_program::system_instruction::transfer(
                autodeploy_balance_account_info.key,
                signer.key,
                deployer_fee,
            ),
            &[
                autodeploy_balance_account_info.clone(),
                signer.clone(),
                system_program_info.clone(),
            ],
            &[autodeploy_balance_seeds],
        )?;
    }

    // Transfer deployment funds from autodeploy_balance to managed_miner_auth PDA
    if transfer_to_miner > 0 {
        solana_program::program::invoke_signed(
            &solana_program::system_instruction::transfer(
                autodeploy_balance_account_info.key,
                managed_miner_auth_account_info.key,
                transfer_to_miner,
            ),
            &[
                autodeploy_balance_account_info.clone(),
                managed_miner_auth_account_info.clone(),
                system_program_info.clone(),
            ],
            &[autodeploy_balance_seeds],
        )?;
    }

    // Get round ID for the deploy CPI
    let round = round_account_info.as_account::<ore_api::Round>(&ore_api::id())?;

    // Build accounts for ORE deploy CPI
    let deploy_accounts = vec![
        managed_miner_auth_account_info.clone(),
        managed_miner_auth_account_info.clone(),
        automation_account_info.clone(),
        board_account_info.clone(),
        config_account_info.clone(),
        ore_miner_account_info.clone(),
        round_account_info.clone(),
        system_program_info.clone(),
        ore_program.clone(),
        entropy_var_account_info.clone(),
        entropy_program.clone(),
        ore_program.clone(),
    ];

    // Execute single ORE deploy CPI
    let managed_miner_auth_key = *managed_miner_auth_account_info.key;
    solana_program::program::invoke_signed(
        &ore_api::deploy(
            managed_miner_auth_key,
            managed_miner_auth_key,
            amount,
            round.id,
            squares,
        ),
        &deploy_accounts,
        &[&[
            MANAGED_MINER_AUTH,
            manager_account_info.key.as_ref(),
            &auth_id.to_le_bytes(),
            &[args.bump],
        ]],
    )?;

    Ok(())
}
