use instruction::Instructions;
use solana_program::{
    account_info::AccountInfo, declare_id, entrypoint::ProgramResult, program_error::ProgramError,
    pubkey::Pubkey,
};

use processor::*;

pub mod processor;
pub mod error;
pub mod instruction;
pub mod state;
pub mod consts;
pub mod ore_api;
pub mod entropy_api;

declare_id!("6kJMMw6psY1MjH3T3yK351uw1FL1aE7rF3xKFz4prHb");

#[cfg(not(feature = "no-entrypoint"))]
solana_program::entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    if program_id.ne(&crate::id()) {
        return Err(ProgramError::IncorrectProgramId);
    }

    let (instruction, data) = instruction_data
        .split_first()
        .ok_or(ProgramError::InvalidInstructionData)?;

    let instruction =
        Instructions::try_from(*instruction).or(Err(ProgramError::InvalidInstructionData))?;

    match instruction {
        Instructions::CreateManager => {
            process_create_manager::process_create_manager(accounts, data)?;
        }
        Instructions::MMDeploy => {
            process_mm_deploy::process_mm_deploy(accounts, data)?;
        }
        Instructions::MMCheckpoint => {
            process_checkpoint::process_checkpoint(accounts, data)?;
        }
        Instructions::MMClaimSOL => {
            process_claim_sol::process_claim_sol(accounts, data)?;
        }
        Instructions::MMClaimORE => {
            process_claim_ore::process_claim_ore(accounts, data)?;
        }
    }

    Ok(())
}
