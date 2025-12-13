use solana_program::{
    account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey, system_program,
};
use steel::*;

use crate::{
    consts::{AUTODEPLOY_BALANCE, DEPLOYER, MANAGED_MINER_AUTH},
    error::EvoreError,
    instruction::RecycleSol,
    ore_api::{self, Miner},
    state::{Deployer, Manager},
};

/// Process RecycleSol instruction
/// Claims SOL from miner account via ORE claim_sol CPI and transfers to autodeploy_balance PDA
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
        autodeploy_balance_account_info,   // 3: autodeploy_balance PDA
        managed_miner_auth_account_info,   // 4: managed_miner_auth PDA
        ore_miner_account_info,            // 5: ore_miner
        ore_program,                       // 6: ore_program
        system_program_info,               // 7: system_program
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

    // Verify autodeploy_balance PDA
    let (autodeploy_balance_pda, _autodeploy_balance_bump) = Pubkey::find_program_address(
        &[AUTODEPLOY_BALANCE, deployer_account_info.key.as_ref()],
        &crate::id(),
    );

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

    // Get balance before claim
    let balance_before = managed_miner_auth_account_info.lamports();

    // Call ORE claim_sol CPI
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
            &[args.bump],
        ]],
    )?;

    // Calculate how much SOL was claimed
    let balance_after = managed_miner_auth_account_info.lamports();
    let claimed_amount = balance_after.saturating_sub(balance_before);

    // Transfer claimed SOL from managed_miner_auth to autodeploy_balance
    if claimed_amount > 0 {
        // Keep minimum rent for managed_miner_auth PDA
        const AUTH_PDA_RENT: u64 = 890_880;
        let transferable = balance_after.saturating_sub(AUTH_PDA_RENT);
        
        if transferable > 0 {
            solana_program::program::invoke_signed(
                &solana_program::system_instruction::transfer(
                    managed_miner_auth_account_info.key,
                    autodeploy_balance_account_info.key,
                    transferable,
                ),
                &[
                    managed_miner_auth_account_info.clone(),
                    autodeploy_balance_account_info.clone(),
                    system_program_info.clone(),
                ],
                &[&[
                    MANAGED_MINER_AUTH,
                    manager_account_info.key.as_ref(),
                    &auth_id.to_le_bytes(),
                    &[args.bump],
                ]],
            )?;
        }
    }

    Ok(())
}
