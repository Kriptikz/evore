use solana_program::{
    account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey, system_program,
};
use steel::*;

use crate::{
    consts::{DEPLOY_FEE, DEPLOYER, FEE_COLLECTOR, MANAGED_MINER_AUTH},
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

    let [
        signer,                            // 0: deploy_authority (signer)
        manager_account_info,              // 1: manager
        deployer_account_info,             // 2: deployer PDA
        managed_miner_auth_account_info,   // 3: managed_miner_auth PDA (funds source)
        ore_miner_account_info,            // 4: ore_miner
        fee_collector_account_info,        // 5: fee_collector
        automation_account_info,           // 6: automation
        config_account_info,               // 7: config
        board_account_info,                // 8: board
        round_account_info,                // 9: round
        entropy_var_account_info,          // 10: entropy_var
        ore_program,                       // 11: ore_program
        entropy_program,                   // 12: entropy_program
        system_program_info,               // 13: system_program
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

    // Verify deployer is initialized
    if deployer_account_info.data_is_empty() {
        return Err(EvoreError::DeployerNotInitialized.into());
    }

    // Verify deployer PDA
    let (deployer_pda, _) = Pubkey::find_program_address(
        &[DEPLOYER, manager_account_info.key.as_ref()],
        &crate::id(),
    );

    if deployer_pda != *deployer_account_info.key {
        return Err(EvoreError::InvalidPDA.into());
    }

    // Load deployer data using as_account (handles discriminator + alignment)
    // NOTE: This will fail on V1 deployers - they must be migrated first via mm_full_autodeploy or migrate_deployer
    let deployer = deployer_account_info.as_account::<Deployer>(&crate::id())?;
    let deploy_authority = deployer.deploy_authority;
    let bps_fee = deployer.bps_fee;
    let flat_fee = deployer.flat_fee;
    let expected_bps_fee = deployer.expected_bps_fee;
    let expected_flat_fee = deployer.expected_flat_fee;
    let max_per_round = deployer.max_per_round;

    // Verify signer is the deploy_authority
    if deploy_authority != *signer.key {
        return Err(EvoreError::InvalidDeployAuthority.into());
    }

    // Verify actual fees don't exceed expected fees (if expected > 0)
    // This allows deployer to dynamically adjust fees while respecting user's max
    if expected_bps_fee > 0 && bps_fee > expected_bps_fee {
        return Err(EvoreError::UnexpectedFee.into());
    }
    if expected_flat_fee > 0 && flat_fee > expected_flat_fee {
        return Err(EvoreError::UnexpectedFee.into());
    }

    // Verify managed_miner_auth PDA
    let (managed_miner_auth_pda, managed_miner_auth_bump) = Pubkey::find_program_address(
        &[MANAGED_MINER_AUTH, manager_account_info.key.as_ref(), &auth_id.to_le_bytes()],
        &crate::id(),
    );

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

    // Calculate total amount to deploy in this transaction
    let total_to_deploy = amount.saturating_mul(num_squares);

    if total_to_deploy == 0 {
        return Err(EvoreError::NoDeployments.into());
    }

    // Check max_per_round limit (includes already deployed amount for this round)
    if max_per_round > 0 {
        // Get already deployed amount for this round (if miner exists and is in current round)
        let already_deployed = if !ore_miner_account_info.data_is_empty() {
            let miner = ore_miner_account_info.as_account::<ore_api::Miner>(&ore_api::id())?;
            if miner.round_id == board.round_id {
                // Sum all deployed amounts for current round
                miner.deployed.iter().sum::<u64>()
            } else {
                0
            }
        } else {
            0
        };

        let total_for_round = already_deployed.saturating_add(total_to_deploy);
        if total_for_round > max_per_round {
            return Err(EvoreError::ExceedsMaxPerRound.into());
        }
    }

    // Calculate deployer fee
    let bps_fee_amount = if bps_fee > 0 {
        total_to_deploy.saturating_mul(bps_fee).saturating_div(10_000)
    } else {
        0
    };
    
    let deployer_fee = bps_fee_amount.saturating_add(flat_fee);
    let protocol_fee = DEPLOY_FEE;

    // Calculate funds needed
    const AUTH_PDA_RENT: u64 = 890_880;
    let miner_rent = if ore_miner_account_info.data_is_empty() {
        let size = 8 + std::mem::size_of::<ore_api::Miner>();
        solana_program::rent::Rent::default().minimum_balance(size)
    } else {
        0
    };
    
    let required_balance = AUTH_PDA_RENT
        .saturating_add(ore_api::CHECKPOINT_FEE)
        .saturating_add(total_to_deploy)
        .saturating_add(miner_rent)
        .saturating_add(deployer_fee)
        .saturating_add(protocol_fee);

    // Check managed_miner_auth has enough funds
    let current_balance = managed_miner_auth_account_info.lamports();
    if current_balance < required_balance {
        return Err(EvoreError::InsufficientAutodeployBalance.into());
    }

    // Managed miner auth PDA seeds for signed transfers
    let managed_miner_auth_seeds: &[&[u8]] = &[
        MANAGED_MINER_AUTH,
        manager_account_info.key.as_ref(),
        &auth_id.to_le_bytes(),
        &[managed_miner_auth_bump],
    ];

    // Check if already deployed this round (only if miner exists)
    let is_already_deployed = if !ore_miner_account_info.data_is_empty() {
        let miner = ore_miner_account_info.as_account::<ore_api::Miner>(&ore_api::id())?;
        miner.round_id == board.round_id
    } else {
        false // First ever deploy, miner doesn't exist yet
    };

    // Transfer protocol fee from managed_miner_auth to FEE_COLLECTOR (only on first deploy of round)
    if protocol_fee > 0 && !is_already_deployed {
        solana_program::program::invoke_signed(
            &solana_program::system_instruction::transfer(
                managed_miner_auth_account_info.key,
                fee_collector_account_info.key,
                protocol_fee,
            ),
            &[
                managed_miner_auth_account_info.clone(),
                fee_collector_account_info.clone(),
                system_program_info.clone(),
            ],
            &[managed_miner_auth_seeds],
        )?;
    }

    // Transfer deployer fee from managed_miner_auth to deploy_authority (signer)
    if deployer_fee > 0  && !is_already_deployed {
        solana_program::program::invoke_signed(
            &solana_program::system_instruction::transfer(
                managed_miner_auth_account_info.key,
                signer.key,
                deployer_fee,
            ),
            &[
                managed_miner_auth_account_info.clone(),
                signer.clone(),
                system_program_info.clone(),
            ],
            &[managed_miner_auth_seeds],
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
    solana_program::program::invoke_signed(
        &ore_api::deploy(
            *managed_miner_auth_account_info.key,
            *managed_miner_auth_account_info.key,
            amount,
            round.id,
            squares,
        ),
        &deploy_accounts,
        &[managed_miner_auth_seeds],
    )?;

    Ok(())
}
