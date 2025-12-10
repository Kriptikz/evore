use solana_program::{
    account_info::AccountInfo, program_error::ProgramError,
};
use steel::*;

use crate::{
    consts::DEPLOYER,
    error::EvoreError,
    instruction::UpdateDeployer,
    state::{Deployer, Manager},
};

pub fn process_update_deployer(
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> Result<(), ProgramError> {
    let args = UpdateDeployer::try_from_bytes(instruction_data)?;
    let new_fee_bps = u64::from_le_bytes(args.fee_bps);

    let [
        signer,
        manager_account_info,
        deployer_account_info,
        new_deploy_authority_info,
    ] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // Verify signer
    if !signer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
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

    // Load deployer (PDA derivation already verifies it belongs to this manager)
    let _deployer = deployer_account_info.as_account::<Deployer>(&crate::id())?;

    // Update the deployer data
    let mut data = deployer_account_info.try_borrow_mut_data()?;
    
    // Update deploy_authority field (offset: 8 discriminator)
    let authority_offset = 8;
    data[authority_offset..authority_offset + 32].copy_from_slice(new_deploy_authority_info.key.as_ref());
    
    // Update fee_bps field (offset: 8 discriminator + 32 deploy_authority = 40)
    let fee_offset = 8 + 32;
    data[fee_offset..fee_offset + 8].copy_from_slice(&new_fee_bps.to_le_bytes());

    Ok(())
}
