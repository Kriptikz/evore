use solana_program::{
    account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey, system_program
};
use steel::*;

use crate::{
    error::EvoreError, instruction::MMCreateMiner, ore_api, state::Manager
};

pub fn process_mm_create_miner(
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> Result<(), ProgramError> {
    let args = MMCreateMiner::try_from_bytes(instruction_data)?;
    let auth_id = u64::from_le_bytes(args.auth_id);

    let [
        signer,
        manager_account_info,
        managed_miner_auth_account_info,
        automation_account_info,
        miner_account_info,
        executor_1_account_info, // executor for first CPI (signer)
        executor_2_account_info, // executor for second CPI (Pubkey::default())
        system_program_info,
        ore_program,
    ] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // Validate signer
    if !signer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    if !signer.is_writable {
        return Err(ProgramError::InvalidAccountData);
    }

    // Validate programs
    if *ore_program.key != ore_api::id() {
        return Err(ProgramError::IncorrectProgramId);
    }

    if *system_program_info.key != system_program::id() {
        return Err(ProgramError::IncorrectProgramId);
    }

    // Validate executor accounts
    // executor_1 should be the signer (for opening automation)
    if *executor_1_account_info.key != *signer.key {
        return Err(ProgramError::InvalidAccountData);
    }

    // executor_2 should be Pubkey::default() (for closing automation)
    if *executor_2_account_info.key != Pubkey::default() {
        return Err(ProgramError::InvalidAccountData);
    }

    // Validate manager
    if manager_account_info.data_is_empty() {
        return Err(EvoreError::ManagerNotInitialized.into());
    }

    let manager = manager_account_info.as_account::<Manager>(&crate::id())?;

    if manager.authority != *signer.key {
        return Err(EvoreError::NotAuthorized.into());
    }

    // Verify managed_miner_auth PDA using bump from instruction
    let managed_miner_auth_pda = Pubkey::create_program_address(
        &[
            crate::consts::MANAGED_MINER_AUTH,
            manager_account_info.key.as_ref(),
            &auth_id.to_le_bytes(),
            &[args.bump],
        ],
        &crate::id(),
    ).map_err(|_| EvoreError::InvalidPDA)?;

    if managed_miner_auth_pda != *managed_miner_auth_account_info.key {
        return Err(EvoreError::InvalidPDA.into());
    }

    // Calculate rent needed for miner and automation account creation
    // During the first automate call, ORE creates both automation and miner accounts
    // The automation account will be closed in the second call, returning its rent
    // But we need enough for both during the first call, plus the checkpoint fee
    let miner_size = 8 + std::mem::size_of::<ore_api::Miner>();
    let miner_rent = solana_program::rent::Rent::default().minimum_balance(miner_size);
    
    // Automation account size (estimate based on ORE program)
    // It stores: executor (32) + authority (32) + various u64 fields
    let automation_size = 8 + 32 + 32 + 8 * 6; // discriminator + pubkeys + u64 fields
    let automation_rent = solana_program::rent::Rent::default().minimum_balance(automation_size);
    
    // ORE requires checkpoint_fee to be held in the miner account
    let total_required = miner_rent + automation_rent + ore_api::CHECKPOINT_FEE ;
    
    // Transfer SOL from signer to managed_miner_auth for account creation
    solana_program::program::invoke(
        &solana_program::system_instruction::transfer(
            signer.key,
            managed_miner_auth_account_info.key,
            total_required,
        ),
        &[
            signer.clone(),
            managed_miner_auth_account_info.clone(),
            system_program_info.clone(),
        ],
    )?;

    // Seeds for signing CPIs
    let managed_miner_auth_seeds: &[&[u8]] = &[
        crate::consts::MANAGED_MINER_AUTH,
        manager_account_info.key.as_ref(),
        &auth_id.to_le_bytes(),
        &[args.bump],
    ];

    // Build accounts for first automate CPI (open automation)
    // executor_1 = signer (opens automation and creates miner)
    let automate_accounts_open = vec![
        managed_miner_auth_account_info.clone(),
        automation_account_info.clone(),
        executor_1_account_info.clone(), // executor = signer
        miner_account_info.clone(),
        system_program_info.clone(),
    ];

    // First CPI: Open automation (creates miner account)
    solana_program::program::invoke_signed(
        &ore_api::automate(
            *managed_miner_auth_account_info.key,
           0,
            0,
            *executor_1_account_info.key, // executor = signer
            0,
            0,
            0,
            false,
        ),
        &automate_accounts_open,
        &[managed_miner_auth_seeds],
    )?;

    // Build accounts for second automate CPI (close automation)
    let automate_accounts_close = vec![
        managed_miner_auth_account_info.clone(),
        automation_account_info.clone(),
        executor_2_account_info.clone(), // executor = Pubkey::default()
        miner_account_info.clone(),
        system_program_info.clone(),
    ];

    // Build close instruction manually with executor as readonly
    // ORE doesn't actually check executor is writable, so using readonly
    // avoids privilege conflicts with system_program (same pubkey)
    use solana_program::instruction::{AccountMeta, Instruction};
    let close_ix = Instruction {
        program_id: ore_api::id(),
        accounts: vec![
            AccountMeta::new(*managed_miner_auth_account_info.key, true),
            AccountMeta::new(*automation_account_info.key, false),
            AccountMeta::new_readonly(Pubkey::default(), false), // executor readonly!
            AccountMeta::new(*miner_account_info.key, false),
            AccountMeta::new_readonly(*system_program_info.key, false),
        ],
        data: ore_api::Automate {
            amount: 0u64.to_le_bytes(),
            deposit: 0u64.to_le_bytes(),
            fee: 0u64.to_le_bytes(),
            mask: 0u64.to_le_bytes(),
            strategy: 0,
            reload: 0u64.to_le_bytes(),
        }
        .to_bytes(),
    };

    // Second CPI: Close automation
    solana_program::program::invoke_signed(
        &close_ix,
        &automate_accounts_close,
        &[managed_miner_auth_seeds],
    )?;

    // Transfer remaining balance from auth_pda back to signer
    // The automation closure returned lamports to auth_pda
    let auth_balance = managed_miner_auth_account_info.lamports();
    if auth_balance > 0 {
        solana_program::program::invoke_signed(
            &solana_program::system_instruction::transfer(
                managed_miner_auth_account_info.key,
                signer.key,
                auth_balance,
            ),
            &[
                managed_miner_auth_account_info.clone(),
                signer.clone(),
                system_program_info.clone(),
            ],
            &[managed_miner_auth_seeds],
        )?;
    }

    Ok(())
}
