/**
 * Transaction builders for Evore SDK
 * 
 * These functions return arrays of TransactionInstructions that can be added
 * to a Transaction object:
 * 
 * @example
 * import { Transaction } from "@solana/web3.js";
 * 
 * const instructions = buildCreateAutoMinerInstructions(...);
 * const tx = new Transaction().add(...instructions);
 */

const {
  createManagerInstruction,
  createDeployerInstruction,
  depositAutodeployBalanceInstruction,
  withdrawAutodeployBalanceInstruction,
  mmCheckpointInstruction,
  mmClaimSolInstruction,
  mmClaimOreInstruction,
  mmAutodeployInstruction,
  mmAutocheckpointInstruction,
  recycleSolInstruction,
} = require("./instructions");

// =============================================================================
// AutoMiner Setup
// =============================================================================

/**
 * Builds instructions to create a new AutoMiner (Manager + Deployer)
 * 
 * This is the primary setup for users joining an automining platform.
 * After signing, the user will have:
 * - A Manager account (their miner container)
 * - A Deployer account linked to the platform's executor
 * 
 * @param {import("@solana/web3.js").PublicKey} signer - User's wallet (pays for account creation)
 * @param {import("@solana/web3.js").PublicKey} managerAccount - New keypair for manager (must also sign!)
 * @param {import("@solana/web3.js").PublicKey} deployAuthority - Platform's executor pubkey
 * @param {bigint} bpsFee - Max bps fee user accepts (deployer can charge up to this)
 * @param {bigint} flatFee - Max flat fee user accepts (deployer can charge up to this)
 * @returns {import("@solana/web3.js").TransactionInstruction[]}
 */
function buildCreateAutoMinerInstructions(
  signer,
  managerAccount,
  deployAuthority,
  bpsFee,
  flatFee = 0n
) {
  const createManager = createManagerInstruction(signer, managerAccount);
  const createDeployer = createDeployerInstruction(signer, managerAccount, deployAuthority, bpsFee, flatFee);
  
  return [createManager, createDeployer];
}

/**
 * Builds instructions to create AutoMiner + initial deposit
 * 
 * Combines setup with initial funding.
 * 
 * @param {import("@solana/web3.js").PublicKey} signer - User's wallet
 * @param {import("@solana/web3.js").PublicKey} managerAccount - New keypair for manager (must also sign!)
 * @param {import("@solana/web3.js").PublicKey} deployAuthority - Platform's executor pubkey
 * @param {bigint} bpsFee - Max bps fee user accepts (deployer can charge up to this)
 * @param {bigint} flatFee - Max flat fee user accepts (deployer can charge up to this)
 * @param {bigint} depositAmount - Initial SOL deposit in lamports
 * @returns {import("@solana/web3.js").TransactionInstruction[]}
 */
function buildSetupAutoMinerInstructions(
  signer,
  managerAccount,
  deployAuthority,
  bpsFee,
  flatFee = 0n,
  depositAmount
) {
  const createManager = createManagerInstruction(signer, managerAccount);
  const createDeployer = createDeployerInstruction(signer, managerAccount, deployAuthority, bpsFee, flatFee);
  const deposit = depositAutodeployBalanceInstruction(signer, managerAccount, depositAmount);
  
  return [createManager, createDeployer, deposit];
}

// =============================================================================
// Deposit/Withdraw
// =============================================================================

/**
 * Builds a deposit instruction
 * 
 * Deposits SOL from user's wallet to their autodeploy balance.
 * 
 * @param {import("@solana/web3.js").PublicKey} signer - User's wallet (manager authority)
 * @param {import("@solana/web3.js").PublicKey} manager - Manager account
 * @param {bigint} amount - Amount to deposit in lamports
 * @returns {import("@solana/web3.js").TransactionInstruction[]}
 */
function buildDepositInstructions(signer, manager, amount) {
  const deposit = depositAutodeployBalanceInstruction(signer, manager, amount);
  return [deposit];
}

/**
 * Builds a withdraw instruction
 * 
 * Withdraws SOL from autodeploy balance back to user's wallet.
 * 
 * @param {import("@solana/web3.js").PublicKey} signer - User's wallet (manager authority)
 * @param {import("@solana/web3.js").PublicKey} manager - Manager account
 * @param {bigint} amount - Amount to withdraw in lamports
 * @returns {import("@solana/web3.js").TransactionInstruction[]}
 */
function buildWithdrawInstructions(signer, manager, amount) {
  const withdraw = withdrawAutodeployBalanceInstruction(signer, manager, amount);
  return [withdraw];
}

// =============================================================================
// Claim Instructions
// =============================================================================

/**
 * Builds checkpoint + claim SOL instructions
 * 
 * Checkpoints a round (claims winnings to miner) then claims SOL to user.
 * 
 * @param {import("@solana/web3.js").PublicKey} signer - User's wallet (manager authority)
 * @param {import("@solana/web3.js").PublicKey} manager - Manager account
 * @param {bigint} roundId - Round to checkpoint
 * @param {bigint} authId - Auth ID (default: 0)
 * @returns {import("@solana/web3.js").TransactionInstruction[]}
 */
function buildCheckpointAndClaimSolInstructions(signer, manager, roundId, authId = 0n) {
  const checkpoint = mmCheckpointInstruction(signer, manager, roundId, authId);
  const claim = mmClaimSolInstruction(signer, manager, authId);
  return [checkpoint, claim];
}

/**
 * Builds a claim SOL instruction
 * 
 * @param {import("@solana/web3.js").PublicKey} signer - User's wallet (manager authority)
 * @param {import("@solana/web3.js").PublicKey} manager - Manager account
 * @param {bigint} authId - Auth ID (default: 0)
 * @returns {import("@solana/web3.js").TransactionInstruction[]}
 */
function buildClaimSolInstructions(signer, manager, authId = 0n) {
  const claim = mmClaimSolInstruction(signer, manager, authId);
  return [claim];
}

/**
 * Builds a claim ORE instruction
 * 
 * @param {import("@solana/web3.js").PublicKey} signer - User's wallet (manager authority)
 * @param {import("@solana/web3.js").PublicKey} manager - Manager account
 * @param {bigint} authId - Auth ID (default: 0)
 * @returns {import("@solana/web3.js").TransactionInstruction[]}
 */
function buildClaimOreInstructions(signer, manager, authId = 0n) {
  const claim = mmClaimOreInstruction(signer, manager, authId);
  return [claim];
}

/**
 * Builds claim all rewards instructions (SOL + ORE)
 * 
 * @param {import("@solana/web3.js").PublicKey} signer - User's wallet (manager authority)
 * @param {import("@solana/web3.js").PublicKey} manager - Manager account
 * @param {bigint} authId - Auth ID (default: 0)
 * @returns {import("@solana/web3.js").TransactionInstruction[]}
 */
function buildClaimAllInstructions(signer, manager, authId = 0n) {
  const claimSol = mmClaimSolInstruction(signer, manager, authId);
  const claimOre = mmClaimOreInstruction(signer, manager, authId);
  return [claimSol, claimOre];
}

// =============================================================================
// Executor/Crank Instructions
// =============================================================================

/**
 * Builds an autodeploy instruction (for executors)
 * 
 * This is used by the executor's crank to deploy on behalf of users.
 * The executor (signer) must be the deploy_authority on the user's deployer.
 * 
 * @param {import("@solana/web3.js").PublicKey} executor - Executor's wallet (deploy_authority)
 * @param {import("@solana/web3.js").PublicKey} manager - User's manager account
 * @param {bigint} authId - Auth ID for the managed miner
 * @param {bigint} roundId - Current round ID
 * @param {bigint} amount - Amount to deploy per selected square
 * @param {number} squaresMask - Bitmask of squares (bits 0-24)
 * @param {bigint} expectedBpsFee - Expected bps_fee (for validation)
 * @param {bigint} expectedFlatFee - Expected flat_fee (for validation)
 * @returns {import("@solana/web3.js").TransactionInstruction[]}
 */
function buildAutodeployInstructions(
  executor,
  manager,
  authId,
  roundId,
  amount,
  squaresMask,
  expectedBpsFee = 0n,
  expectedFlatFee = 0n
) {
  const autodeploy = mmAutodeployInstruction(
    executor,
    manager,
    authId,
    roundId,
    amount,
    squaresMask,
    expectedBpsFee,
    expectedFlatFee
  );
  return [autodeploy];
}

/**
 * Builds autocheckpoint + autodeploy instructions (for executors)
 * 
 * Checkpoints the previous round then deploys for the current round.
 * Common pattern for executor cranks.
 * 
 * @param {import("@solana/web3.js").PublicKey} executor - Executor's wallet (deploy_authority)
 * @param {import("@solana/web3.js").PublicKey} manager - User's manager account
 * @param {bigint} authId - Auth ID for the managed miner
 * @param {bigint} checkpointRoundId - Round to checkpoint
 * @param {bigint} deployRoundId - Current round ID to deploy to
 * @param {bigint} amount - Amount to deploy per selected square
 * @param {number} squaresMask - Bitmask of squares (bits 0-24)
 * @param {bigint} expectedBpsFee - Expected bps_fee (for validation)
 * @param {bigint} expectedFlatFee - Expected flat_fee (for validation)
 * @returns {import("@solana/web3.js").TransactionInstruction[]}
 */
function buildCheckpointAndAutodeployInstructions(
  executor,
  manager,
  authId,
  checkpointRoundId,
  deployRoundId,
  amount,
  squaresMask,
  expectedBpsFee = 0n,
  expectedFlatFee = 0n
) {
  const checkpoint = mmAutocheckpointInstruction(executor, manager, checkpointRoundId, authId);
  const autodeploy = mmAutodeployInstruction(
    executor,
    manager,
    authId,
    deployRoundId,
    amount,
    squaresMask,
    expectedBpsFee,
    expectedFlatFee
  );
  return [checkpoint, autodeploy];
}

/**
 * Builds a recycle SOL instruction (for executors)
 * 
 * Moves SOL winnings from the miner account back to autodeploy balance.
 * Useful for automatic compounding.
 * 
 * @param {import("@solana/web3.js").PublicKey} executor - Executor's wallet (deploy_authority)
 * @param {import("@solana/web3.js").PublicKey} manager - User's manager account
 * @param {bigint} authId - Auth ID for the managed miner
 * @returns {import("@solana/web3.js").TransactionInstruction[]}
 */
function buildRecycleSolInstructions(executor, manager, authId) {
  const recycle = recycleSolInstruction(executor, manager, authId);
  return [recycle];
}

/**
 * Builds batched autodeploy instructions for multiple users (for executors)
 * 
 * Batch up to 7 autodeploys for efficiency (with Address Lookup Table).
 * Each deploy can have its own amount and squares mask.
 * 
 * @param {import("@solana/web3.js").PublicKey} executor - Executor's wallet (deploy_authority)
 * @param {Array<{manager: import("@solana/web3.js").PublicKey, authId: bigint, amount: bigint, squaresMask: number, expectedBpsFee?: bigint, expectedFlatFee?: bigint}>} deploys - Array of deploy configs
 * @param {bigint} roundId - Current round ID
 * @returns {import("@solana/web3.js").TransactionInstruction[]}
 */
function buildBatchedAutodeployInstructions(executor, deploys, roundId) {
  if (deploys.length > 7) {
    throw new Error("Maximum 7 deploys per batch");
  }
  
  const instructions = [];
  
  for (const deploy of deploys) {
    const ix = mmAutodeployInstruction(
      executor,
      deploy.manager,
      deploy.authId,
      roundId,
      deploy.amount,
      deploy.squaresMask,
      deploy.expectedBpsFee || 0n,
      deploy.expectedFlatFee || 0n
    );
    instructions.push(ix);
  }
  
  return instructions;
}

module.exports = {
  // Setup
  buildCreateAutoMinerInstructions,
  buildSetupAutoMinerInstructions,
  
  // Deposit/Withdraw
  buildDepositInstructions,
  buildWithdrawInstructions,
  
  // Claims
  buildCheckpointAndClaimSolInstructions,
  buildClaimSolInstructions,
  buildClaimOreInstructions,
  buildClaimAllInstructions,
  
  // Executor/Crank
  buildAutodeployInstructions,
  buildCheckpointAndAutodeployInstructions,
  buildRecycleSolInstructions,
  buildBatchedAutodeployInstructions,
};
