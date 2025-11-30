use solana_program::{
    account_info::AccountInfo, program_error::ProgramError, system_program
};
use steel::*;

use crate::state::Manager;

pub fn process_create_manager(
    accounts: &[AccountInfo],
    _instruction_data: &[u8],
) -> Result<(), ProgramError> {
    let [signer, managed_miner_account_info, system_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    if !signer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    if !managed_miner_account_info.is_writable {
        return Err(ProgramError::InvalidAccountData);
    }

    if !managed_miner_account_info.data_is_empty() {
        return Err(ProgramError::AccountAlreadyInitialized);
    }

    if *system_program.key != system_program::id() {
        return Err(ProgramError::IncorrectProgramId);
    }

    let space = 8 + std::mem::size_of::<Manager>();
    create_account(
        signer,
        managed_miner_account_info,
        system_program,
        space,
        &crate::ID,
    )?;

    // Set discriminator.
    let mut data = managed_miner_account_info.data.borrow_mut();
    data[0] = Manager::discriminator();
    drop(data);

    let manager = managed_miner_account_info.as_account_mut::<Manager>(&crate::ID)?;
    manager.authority = *signer.key;

    Ok(())
}
