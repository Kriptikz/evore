use solana_program::{
    account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey, system_program,
};
use steel::*;

use crate::{
    consts::MANAGED_MINER_AUTH,
    error::EvoreError,
    instruction::WithdrawAutodeployBalance,
    state::Manager,
};

/// Process WithdrawAutodeployBalance instruction
/// Withdraws SOL from managed_miner_auth PDA to manager authority
pub fn process_withdraw_autodeploy_balance(
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> Result<(), ProgramError> {
    let args = WithdrawAutodeployBalance::try_from_bytes(instruction_data)?;
    let auth_id = u64::from_le_bytes(args.auth_id);
    let amount = u64::from_le_bytes(args.amount);

    let [
        signer,                            // 0: signer (manager authority, also recipient)
        manager_account_info,              // 1: manager
        managed_miner_auth_account_info,   // 2: managed_miner_auth PDA
        system_program_info,               // 3: system_program
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

    // Verify managed_miner_auth PDA
    let (managed_miner_auth_pda, managed_miner_auth_bump) = Pubkey::find_program_address(
        &[MANAGED_MINER_AUTH, manager_account_info.key.as_ref(), &auth_id.to_le_bytes()],
        &crate::id(),
    );

    if managed_miner_auth_pda != *managed_miner_auth_account_info.key {
        return Err(EvoreError::InvalidPDA.into());
    }

    // Check sufficient balance (keep rent-exempt minimum)
    const AUTH_PDA_RENT: u64 = 890_880;
    let current_balance = managed_miner_auth_account_info.lamports();
    let available = current_balance.saturating_sub(AUTH_PDA_RENT);
    
    if available < amount {
        return Err(EvoreError::InsufficientAutodeployBalance.into());
    }

    // Transfer SOL from managed_miner_auth to manager authority (signer)
    solana_program::program::invoke_signed(
        &solana_program::system_instruction::transfer(
            managed_miner_auth_account_info.key,
            signer.key,
            amount,
        ),
        &[
            managed_miner_auth_account_info.clone(),
            signer.clone(),
            system_program_info.clone(),
        ],
        &[&[
            MANAGED_MINER_AUTH,
            manager_account_info.key.as_ref(),
            &auth_id.to_le_bytes(),
            &[managed_miner_auth_bump],
        ]],
    )?;

    Ok(())
}
