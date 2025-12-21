use solana_program::{account_info::AccountInfo, program_error::ProgramError};
use steel::*;

use crate::{error::EvoreError, state::Manager};

pub fn process_transfer_manager(
    accounts: &[AccountInfo],
    _instruction_data: &[u8],
) -> Result<(), ProgramError> {
    let [signer, manager_account_info, new_authority_info] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // 1. Verify signer
    if !signer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // 2. Verify manager is initialized
    if manager_account_info.data_is_empty() {
        return Err(EvoreError::ManagerNotInitialized.into());
    }

    // 3. Load manager mutably and verify current authority
    let manager = manager_account_info.as_account_mut::<Manager>(&crate::id())?;
    
    if manager.authority != *signer.key {
        return Err(EvoreError::NotAuthorized.into());
    }

    // 4. Update authority to new pubkey
    manager.authority = *new_authority_info.key;

    Ok(())
}
