use solana_program::{
    account_info::AccountInfo, program_error::ProgramError, system_program,
};
use steel::*;

use crate::{
    consts::{AUTODEPLOY_BALANCE, DEPLOYER},
    error::EvoreError,
    instruction::DepositAutodeployBalance,
    state::{Deployer, Manager},
};

pub fn process_deposit_autodeploy_balance(
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> Result<(), ProgramError> {
    let args = DepositAutodeployBalance::try_from_bytes(instruction_data)?;
    let amount = u64::from_le_bytes(args.amount);

    let [
        signer,
        manager_account_info,
        deployer_account_info,
        autodeploy_balance_account_info,
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

    // If autodeploy_balance PDA doesn't exist, create it (0-byte account)
    if autodeploy_balance_account_info.data_is_empty() && autodeploy_balance_account_info.lamports() == 0 {
        // Create the PDA with just rent-exempt minimum for 0-byte account
        // Actually, for a 0-byte PDA we just need to transfer SOL to it
        // The PDA doesn't need to be "created" in the traditional sense
        // We'll just transfer the amount directly
    }

    // Transfer SOL from signer to autodeploy_balance PDA
    solana_program::program::invoke(
        &solana_program::system_instruction::transfer(
            signer.key,
            autodeploy_balance_account_info.key,
            amount,
        ),
        &[
            signer.clone(),
            autodeploy_balance_account_info.clone(),
            system_program_info.clone(),
        ],
    )?;

    Ok(())
}
