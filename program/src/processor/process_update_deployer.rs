use solana_program::{
    account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey,
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
    let new_bps_fee = u64::from_le_bytes(args.bps_fee);
    let new_flat_fee = u64::from_le_bytes(args.flat_fee);
    let new_expected_bps_fee = u64::from_le_bytes(args.expected_bps_fee);
    let new_expected_flat_fee = u64::from_le_bytes(args.expected_flat_fee);
    let new_max_per_round = u64::from_le_bytes(args.max_per_round);

    let [
        signer,
        manager_account_info,
        deployer_account_info,
        new_deploy_authority_info,
        _system_program_info,
    ] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // Verify signer
    if !signer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Verify manager is initialized
    if manager_account_info.data_is_empty() {
        return Err(EvoreError::ManagerNotInitialized.into());
    }

    let manager = manager_account_info.as_account::<Manager>(&crate::id())?;

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

    // Load existing deployer data
    let deployer = deployer_account_info.as_account::<Deployer>(&crate::id())?;
    let current_deploy_authority = deployer.deploy_authority;

    // Determine who is signing and what they can update
    let is_manager_authority = manager.authority == *signer.key;
    let is_deploy_authority = current_deploy_authority == *signer.key;

    if !is_manager_authority && !is_deploy_authority {
        return Err(EvoreError::NotAuthorized.into());
    }

    // Update deployer data
    let mut data = deployer_account_info.try_borrow_mut_data()?;
    
    if is_manager_authority {
        // Manager can update: deploy_authority, expected_bps_fee, expected_flat_fee, max_per_round
        // These are the maximum fees the manager is willing to accept
        // deploy_authority at offset 40
        data[40..72].copy_from_slice(new_deploy_authority_info.key.as_ref());
        // expected_bps_fee at offset 88
        data[88..96].copy_from_slice(&new_expected_bps_fee.to_le_bytes());
        // expected_flat_fee at offset 96
        data[96..104].copy_from_slice(&new_expected_flat_fee.to_le_bytes());
        // max_per_round at offset 104
        data[104..112].copy_from_slice(&new_max_per_round.to_le_bytes());
    }
    
    if is_deploy_authority {
        // Deploy authority can update: deploy_authority, bps_fee, flat_fee
        // These are the actual fees charged (must be <= expected fees for autodeploys)
        // deploy_authority at offset 40
        data[40..72].copy_from_slice(new_deploy_authority_info.key.as_ref());
        // bps_fee at offset 72
        data[72..80].copy_from_slice(&new_bps_fee.to_le_bytes());
        // flat_fee at offset 80
        data[80..88].copy_from_slice(&new_flat_fee.to_le_bytes());
    }

    Ok(())
}
