use solana_program::{
    account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey, system_program
};
use steel::*;

use crate::{
    error::EvoreError, instruction::{EvDeploy, MMClaimORE, MMClaimSOL}, ore_api::{self, Board, Round}, state::Manager
};

pub fn process_claim_ore(
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> Result<(), ProgramError> {
    let args = MMClaimORE::try_from_bytes(instruction_data)?;
    let auth_id = u64::from_le_bytes(args.auth_id);

    let [
            signer,
            manager_account_info,
            managed_miner_auth_account_info,
            ore_miner_account_info,
            mint_account_info,
            recipient_account_info,
            signer_recipient_account_info,
            treasury_account_info,
            treasury_tokens_account_info,
            system_program,
            spl_program,
            spl_ata_program,
            ore_program,
    ] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    if !signer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    if manager_account_info.data_is_empty() {
        return Err(ProgramError::InvalidAccountData);
    }

    if *ore_program.key != ore_api::id() {
        return Err(ProgramError::IncorrectProgramId);
    }

    if *system_program.key != system_program::id() {
        return Err(ProgramError::IncorrectProgramId);
    }

    let manager = manager_account_info
        .as_account::<Manager>(&crate::id())?;

    if manager.authority != *signer.key {
        return Err(EvoreError::NotAuthorized.into());
    }

    let managed_miner_auth_pda = Pubkey::find_program_address(
        &[
            crate::consts::MANAGED_MINER_AUTH,
            manager_account_info.key.as_ref(),
            &auth_id.to_le_bytes(),

        ],
        &crate::id(),
    );

    let claim_ore_accounts = 
        vec![
            managed_miner_auth_account_info.clone(),
            ore_miner_account_info.clone(),
            mint_account_info.clone(),
            recipient_account_info.clone(),
            treasury_account_info.clone(),
            treasury_tokens_account_info.clone(),
            system_program.clone(),
            spl_program.clone(),
            spl_ata_program.clone(),
            ore_program.clone(),
        ];
    let managed_miner_auth_key = claim_ore_accounts[0].key.clone();

    if recipient_account_info.data_is_empty() {
        create_associated_token_account(
            signer,
            managed_miner_auth_account_info,
            recipient_account_info,
            mint_account_info,
            system_program,
            spl_program,
            spl_ata_program,
        )?;
    } else {
        recipient_account_info.as_associated_token_account(managed_miner_auth_account_info.key, mint_account_info.key)?;
    }

    solana_program::program::invoke_signed(
        &ore_api::claim_ore(
            managed_miner_auth_key,
        ),
        &claim_ore_accounts,
        &[&[
            crate::consts::MANAGED_MINER_AUTH,
            manager_account_info.key.as_ref(),
            &auth_id.to_le_bytes(),
            &[managed_miner_auth_pda.1],
        ]],
    )?;


    if signer_recipient_account_info.data_is_empty() {
        create_associated_token_account(
            signer,
            signer,
            signer_recipient_account_info,
            mint_account_info,
            system_program,
            spl_program,
            spl_ata_program,
        )?;
    } else {
        signer_recipient_account_info.as_associated_token_account(signer.key, mint_account_info.key)?;
    }

    let managed_auth_tokens = recipient_account_info.as_associated_token_account(&managed_miner_auth_key, &mint_account_info.key)?;

    transfer_signed_with_bump(
        managed_miner_auth_account_info,
        recipient_account_info,
        signer_recipient_account_info,
        spl_program,
        managed_auth_tokens.amount(),
        &[
            crate::consts::MANAGED_MINER_AUTH,
            manager_account_info.key.as_ref(),
            &auth_id.to_le_bytes(),
        ],
        managed_miner_auth_pda.1
    )?;

    Ok(())
}

