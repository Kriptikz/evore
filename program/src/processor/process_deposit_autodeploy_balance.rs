use solana_program::{
    account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey, system_program,
};
use steel::*;

use crate::{
    consts::MANAGED_MINER_AUTH,
    error::EvoreError,
    instruction::DepositAutodeployBalance,
    state::Manager,
};

pub fn process_deposit_autodeploy_balance(
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> Result<(), ProgramError> {
    let args = DepositAutodeployBalance::try_from_bytes(instruction_data)?;
    let auth_id = u64::from_le_bytes(args.auth_id);
    let amount = u64::from_le_bytes(args.amount);

    let [
        signer,
        manager_account_info,
        managed_miner_auth_account_info,
        system_program_info,
    ] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // Verify signer
    if !signer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Verify system program
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
    let (managed_miner_auth_pda, _) = Pubkey::find_program_address(
        &[MANAGED_MINER_AUTH, manager_account_info.key.as_ref(), &auth_id.to_le_bytes()],
        &crate::id(),
    );

    if managed_miner_auth_pda != *managed_miner_auth_account_info.key {
        return Err(EvoreError::InvalidPDA.into());
    }

    // Transfer SOL from signer to managed_miner_auth PDA
    solana_program::program::invoke(
        &solana_program::system_instruction::transfer(
            signer.key,
            managed_miner_auth_account_info.key,
            amount,
        ),
        &[
            signer.clone(),
            managed_miner_auth_account_info.clone(),
            system_program_info.clone(),
        ],
    )?;

    Ok(())
}
