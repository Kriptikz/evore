use solana_program::{
    account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey, system_program
};
use steel::*;

use crate::{
    consts::{DEPLOYER, MANAGED_MINER_AUTH},
    error::EvoreError,
    instruction::MMAutocheckpoint,
    ore_api::{self, Miner, Round},
    state::{Deployer, Manager},
};

/// Process MMAutocheckpoint instruction
/// 
/// Similar to MMCheckpoint but can be called by deploy_authority instead of manager authority.
/// This allows the autodeploy crank to checkpoint before deploying.
pub fn process_mm_autocheckpoint(
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> Result<(), ProgramError> {
    let args = MMAutocheckpoint::try_from_bytes(instruction_data)?;
    let auth_id = u64::from_le_bytes(args.auth_id);

    let [
        signer,                            // 0: deploy_authority (signer)
        manager_account_info,              // 1: manager
        deployer_account_info,             // 2: deployer PDA
        managed_miner_auth_account_info,   // 3: managed_miner_auth PDA
        ore_miner_account_info,            // 4: ore_miner
        treasury_account_info,             // 5: treasury
        board_account_info,                // 6: board
        round_account_info,                // 7: round
        system_program_info,               // 8: system_program
        ore_program,                       // 9: ore_program
    ] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // Basic validations
    if !signer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    if !managed_miner_auth_account_info.is_writable {
        return Err(ProgramError::InvalidAccountData);
    }

    if *ore_program.key != ore_api::id() {
        return Err(ProgramError::IncorrectProgramId);
    }

    if *system_program_info.key != system_program::id() {
        return Err(ProgramError::IncorrectProgramId);
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
    let (deployer_pda, _deployer_bump) = Pubkey::find_program_address(
        &[DEPLOYER, manager_account_info.key.as_ref()],
        &crate::id(),
    );

    if deployer_pda != *deployer_account_info.key {
        return Err(EvoreError::InvalidPDA.into());
    }

    // Load deployer and verify signer is deploy_authority
    let deployer = deployer_account_info.as_account::<Deployer>(&crate::id())?;

    if deployer.deploy_authority != *signer.key {
        return Err(EvoreError::InvalidDeployAuthority.into());
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

    // Build checkpoint CPI accounts
    let checkpoint_accounts = vec![
        managed_miner_auth_account_info.clone(),
        board_account_info.clone(),
        ore_miner_account_info.clone(),
        round_account_info.clone(),
        treasury_account_info.clone(),
        system_program_info.clone(),
        ore_program.clone(),
    ];

    let managed_miner_auth_key = *managed_miner_auth_account_info.key;

    let checkpoint_round_id = if ore_miner_account_info.data_is_empty() {
      return Err(ProgramError::InvalidAccountData)
    } else {
      let ore_miner = ore_miner_account_info.as_account::<Miner>(&ore_api::id())?;
      ore_miner.round_id
    };

    // Call ORE checkpoint CPI
    solana_program::program::invoke_signed(
        &ore_api::checkpoint(
            managed_miner_auth_key,
            managed_miner_auth_key,
            checkpoint_round_id,
        ),
        &checkpoint_accounts,
        &[&[
            MANAGED_MINER_AUTH,
            manager_account_info.key.as_ref(),
            &auth_id.to_le_bytes(),
            &[args.bump],
        ]],
    )?;

    Ok(())
}
