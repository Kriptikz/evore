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

declare_id!("8jaLKWLJAj5jVCZbxpe3zRUvLB3LD48MRtaQ2AjfCfxa");

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
        Instructions::CreateDeployer => {
            process_create_deployer::process_create_deployer(accounts, data)?;
        }
        Instructions::UpdateDeployer => {
            process_update_deployer::process_update_deployer(accounts, data)?;
        }
        Instructions::MMAutodeploy => {
            process_mm_autodeploy::process_mm_autodeploy(accounts, data)?;
        }
        Instructions::DepositAutodeployBalance => {
            process_deposit_autodeploy_balance::process_deposit_autodeploy_balance(accounts, data)?;
        }
        Instructions::RecycleSol => {
            process_recycle_sol::process_recycle_sol(accounts, data)?;
        }
        Instructions::WithdrawAutodeployBalance => {
            process_withdraw_autodeploy_balance::process_withdraw_autodeploy_balance(accounts, data)?;
        }
        Instructions::MMAutocheckpoint => {
            process_mm_autocheckpoint::process_mm_autocheckpoint(accounts, data)?;
        }
        Instructions::MMFullAutodeploy => {
            process_mm_full_autodeploy::process_mm_full_autodeploy(accounts, data)?;
        }
        Instructions::TransferManager => {
            process_transfer_manager::process_transfer_manager(accounts, data)?;
        }
        Instructions::MMCreateMiner => {
            process_mm_create_miner::process_mm_create_miner(accounts, data)?;
        }
    }

    Ok(())
}
