use solana_program::{
    account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey, system_program,
};
use steel::*;

use crate::{
    consts::{AUTODEPLOY_BALANCE, DEPLOYER},
    error::EvoreError,
    instruction::WithdrawAutodeployBalance,
    state::{Deployer, Manager},
};

/// Process WithdrawAutodeployBalance instruction
/// Withdraws SOL from autodeploy_balance PDA to manager authority
pub fn process_withdraw_autodeploy_balance(
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> Result<(), ProgramError> {
    let args = WithdrawAutodeployBalance::try_from_bytes(instruction_data)?;
    let amount = u64::from_le_bytes(args.amount);

    let [
        signer,                            // 0: signer (manager authority, also recipient)
        manager_account_info,              // 1: manager
        deployer_account_info,             // 2: deployer PDA
        autodeploy_balance_account_info,   // 3: autodeploy_balance PDA
        system_program_info,               // 4: system_program
    ] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // Basic validations
    if !signer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    if *system_program_info.key != system_program::id() {
        return Err(ProgramError::IncorrectProgramId);
    }

    // Verify manager is initialized and signer is the authority
    if manager_account_info.data_is_empty() {
        return Err(EvoreError::ManagerNotInitialized.into());
    }

    let manager = manager_account_info.as_account::<Manager>(&crate::id())?;

    if manager.authority != *signer.key {
        return Err(EvoreError::NotAuthorized.into());
    }

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

    // Load deployer (PDA derivation verifies it belongs to this manager)
    let _deployer = deployer_account_info.as_account::<Deployer>(&crate::id())?;

    // Verify autodeploy_balance PDA
    let (autodeploy_balance_pda, autodeploy_balance_bump) = Pubkey::find_program_address(
        &[AUTODEPLOY_BALANCE, deployer_account_info.key.as_ref()],
        &crate::id(),
    );

    if autodeploy_balance_pda != *autodeploy_balance_account_info.key {
        return Err(EvoreError::InvalidPDA.into());
    }

    // Check sufficient balance
    let current_balance = autodeploy_balance_account_info.lamports();
    if current_balance < amount {
        return Err(EvoreError::InsufficientAutodeployBalance.into());
    }

    // Transfer SOL from autodeploy_balance to manager authority (signer)
    solana_program::program::invoke_signed(
        &solana_program::system_instruction::transfer(
            autodeploy_balance_account_info.key,
            signer.key,
            amount,
        ),
        &[
            autodeploy_balance_account_info.clone(),
            signer.clone(),
            system_program_info.clone(),
        ],
        &[&[
            AUTODEPLOY_BALANCE,
            deployer_account_info.key.as_ref(),
            &[autodeploy_balance_bump],
        ]],
    )?;

    Ok(())
}
