use solana_program::{
    account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey,
};
use steel::*;

use crate::{
    consts::{DEPLOYER, MANAGED_MINER_AUTH},
    error::EvoreError,
    instruction::RecycleSol,
    ore_api::{self, Miner},
    state::{Deployer, Manager},
};

/// Process RecycleSol instruction
/// Claims SOL from miner account via ORE claim_sol CPI
/// SOL stays in managed_miner_auth for future deploys
pub fn process_recycle_sol(
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> Result<(), ProgramError> {
    let args = RecycleSol::try_from_bytes(instruction_data)?;
    let auth_id = u64::from_le_bytes(args.auth_id);

    let [
        signer,                            // 0: deploy_authority (signer)
        manager_account_info,              // 1: manager
        deployer_account_info,             // 2: deployer PDA
        managed_miner_auth_account_info,   // 3: managed_miner_auth PDA
        ore_miner_account_info,            // 4: ore_miner
        ore_program,                       // 5: ore_program
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
    let data = deployer_account_info.try_borrow_data()?;
    let deployer = Deployer::try_from_bytes(&data[8..])?;
    let deploy_authority = deployer.deploy_authority;
    drop(data);

    if deploy_authority != *signer.key {
        return Err(EvoreError::InvalidDeployAuthority.into());
    }

    // Verify managed_miner_auth PDA
    let (managed_miner_auth_pda, managed_miner_auth_bump) = Pubkey::find_program_address(
        &[MANAGED_MINER_AUTH, manager_account_info.key.as_ref(), &auth_id.to_le_bytes()],
        &crate::id(),
    );

    if managed_miner_auth_pda != *managed_miner_auth_account_info.key {
        return Err(EvoreError::InvalidPDA.into());
    }

    // Verify ore miner belongs to this managed_miner_auth
    let expected_ore_miner = ore_api::miner_pda(*managed_miner_auth_account_info.key).0;
    if expected_ore_miner != *ore_miner_account_info.key {
        return Err(EvoreError::InvalidPDA.into());
    }

    // Check if miner exists and has claimable SOL - return Ok if nothing to recycle
    if ore_miner_account_info.data_is_empty() {
        return Ok(());
    }

    let miner = ore_miner_account_info.as_account::<Miner>(&ore_api::id())?;
    let claimable_sol = miner.rewards_sol;

    if claimable_sol == 0 {
        return Ok(());
    }

    // Call ORE claim_sol CPI - SOL stays in managed_miner_auth
    let claim_accounts = vec![
        managed_miner_auth_account_info.clone(),
        ore_miner_account_info.clone(),
        ore_program.clone(),
    ];

    solana_program::program::invoke_signed(
        &ore_api::claim_sol(*managed_miner_auth_account_info.key),
        &claim_accounts,
        &[&[
            MANAGED_MINER_AUTH,
            manager_account_info.key.as_ref(),
            &auth_id.to_le_bytes(),
            &[managed_miner_auth_bump],
        ]],
    )?;

    Ok(())
}
