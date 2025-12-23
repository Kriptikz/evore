use solana_program::{
    account_info::AccountInfo, program_error::ProgramError, system_program,
};
use steel::*;

use crate::{
    consts::DEPLOYER,
    error::EvoreError,
    instruction::CreateDeployer,
    state::{Deployer, EvoreAccount, Manager},
};

pub fn process_create_deployer(
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> Result<(), ProgramError> {
    let args = CreateDeployer::try_from_bytes(instruction_data)?;
    // Manager sets expected fees (max they're willing to pay)
    let expected_bps_fee = u64::from_le_bytes(args.bps_fee);
    let expected_flat_fee = u64::from_le_bytes(args.flat_fee);
    let max_per_round = u64::from_le_bytes(args.max_per_round);

    let [
        signer,
        manager_account_info,
        deployer_account_info,
        deploy_authority_info,
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

    // Verify deployer account is not already initialized
    if !deployer_account_info.data_is_empty() {
        return Err(EvoreError::DeployerAlreadyInitialized.into());
    }

    // Derive and verify deployer PDA
    let (deployer_pda, deployer_bump) = Pubkey::find_program_address(
        &[DEPLOYER, manager_account_info.key.as_ref()],
        &crate::id(),
    );

    if deployer_pda != *deployer_account_info.key {
        return Err(EvoreError::InvalidPDA.into());
    }

    // Calculate space for Deployer account
    // 8 bytes discriminator + Deployer struct size
    let deployer_size = 8 + std::mem::size_of::<Deployer>();
    let rent = solana_program::rent::Rent::get()?;
    let lamports = rent.minimum_balance(deployer_size);

    // Create the deployer account
    solana_program::program::invoke_signed(
        &solana_program::system_instruction::create_account(
            signer.key,
            deployer_account_info.key,
            lamports,
            deployer_size as u64,
            &crate::id(),
        ),
        &[signer.clone(), deployer_account_info.clone(), system_program_info.clone()],
        &[&[DEPLOYER, manager_account_info.key.as_ref(), &[deployer_bump]]],
    )?;

    // Initialize the deployer data
    // Manager sets expected fees (max they'll accept), actual fees start at expected (deploy_authority can lower)
    let deployer = Deployer {
        manager_key: *manager_account_info.key,
        deploy_authority: *deploy_authority_info.key,
        bps_fee: expected_bps_fee,        // Actual fee starts at expected max
        flat_fee: expected_flat_fee,       // Actual fee starts at expected max
        expected_bps_fee,                  // Max bps fee manager accepts
        expected_flat_fee,                 // Max flat fee manager accepts
        max_per_round,
    };

    // Write discriminator and data
    let mut data = deployer_account_info.try_borrow_mut_data()?;
    let discr = (EvoreAccount::Deployer as u64).to_le_bytes();
    data[..8].copy_from_slice(&discr);
    data[8..8 + std::mem::size_of::<Deployer>()].copy_from_slice(deployer.to_bytes());

    Ok(())
}
