const { PublicKey } = require("@solana/web3.js");

// =============================================================================
// Evore Program
// =============================================================================

/** Evore program ID */
const EVORE_PROGRAM_ID = new PublicKey("8jaLKWLJAj5jVCZbxpe3zRUvLB3LD48MRtaQ2AjfCfxa");

/** Protocol fee collector address */
const FEE_COLLECTOR = new PublicKey("56qSi79jWdM1zie17NKFvdsh213wPb15HHUqGUjmJ2Lr");

/** 
 * Base protocol deploy fee in lamports
 * This is the minimum fee charged by the Evore protocol per deployment
 */
const DEPLOY_FEE = 1000n;

// Evore PDA seeds
const MANAGED_MINER_AUTH_SEED = "managed-miner-auth";
const DEPLOYER_SEED = "deployer";

// =============================================================================
// ORE Program (v3)
// =============================================================================

/** ORE v3 program ID */
const ORE_PROGRAM_ID = new PublicKey("oreV3EG1i9BEgiAJ8b177Z2S2rMarzak4NMv1kULvWv");

/** ORE token mint address */
const ORE_MINT_ADDRESS = new PublicKey("oreoU2P8bN6jkk3jbaiVxYnG1dCXcYxwhwyK9jSybcp");

/** ORE treasury address */
const ORE_TREASURY_ADDRESS = new PublicKey("45db2FSR4mcXdSVVZbKbwojU6uYDpMyhpEi7cC8nHaWG");

/** Checkpoint fee required by ORE v3 (in lamports) */
const ORE_CHECKPOINT_FEE = 10000n;

/** Number of intermission slots between rounds */
const ORE_INTERMISSION_SLOTS = 35n;

// ORE PDA seeds
const ORE_MINER_SEED = "miner";
const ORE_BOARD_SEED = "board";
const ORE_ROUND_SEED = "round";
const ORE_CONFIG_SEED = "config";
const ORE_AUTOMATION_SEED = "automation";
const ORE_TREASURY_SEED = "treasury";

// =============================================================================
// Entropy Program
// =============================================================================

/** Entropy program ID */
const ENTROPY_PROGRAM_ID = new PublicKey("3jSkUuYBoJzQPMEzTvkDFXCZUBksPamrVhrnHR9igu2X");

/** Entropy var PDA seed */
const ENTROPY_VAR_SEED = "var";

// =============================================================================
// SPL Token Programs
// =============================================================================

/** SPL Token program ID */
const TOKEN_PROGRAM_ID = new PublicKey("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

/** Associated Token Account program ID */
const ASSOCIATED_TOKEN_PROGRAM_ID = new PublicKey("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");

/** System program ID */
const SYSTEM_PROGRAM_ID = new PublicKey("11111111111111111111111111111111");

// =============================================================================
// Account Discriminators
// =============================================================================

/** Manager account discriminator */
const MANAGER_DISCRIMINATOR = 100;

/** Deployer account discriminator */
const DEPLOYER_DISCRIMINATOR = 101;

// =============================================================================
// Instruction Discriminators
// =============================================================================

/** Evore instruction discriminators (must match program) */
const EvoreInstruction = {
  CreateManager: 0,
  MMDeploy: 1,
  MMCheckpoint: 2,
  MMClaimSOL: 3,
  MMClaimORE: 4,
  CreateDeployer: 5,
  UpdateDeployer: 6,
  MMAutodeploy: 7,
  DepositAutodeployBalance: 8,
  RecycleSol: 9,
  WithdrawAutodeployBalance: 10,
  MMAutocheckpoint: 11,
  MMFullAutodeploy: 12,
  TransferManager: 13,
  MMCreateMiner: 14,
};

// =============================================================================
// Helpful Constants for Developers
// =============================================================================

/** Minimum recommended autodeploy balance for first deploy (includes miner creation rent) */
const MIN_AUTODEPLOY_BALANCE_FIRST = 7000000n; // 0.007 SOL

/** Minimum recommended autodeploy balance for subsequent deploys */
const MIN_AUTODEPLOY_BALANCE = 4000000n; // 0.004 SOL

/** Lamports per SOL */
const LAMPORTS_PER_SOL = 1000000000n;

module.exports = {
  // Evore
  EVORE_PROGRAM_ID,
  FEE_COLLECTOR,
  DEPLOY_FEE,
  MANAGED_MINER_AUTH_SEED,
  DEPLOYER_SEED,
  
  // ORE
  ORE_PROGRAM_ID,
  ORE_MINT_ADDRESS,
  ORE_TREASURY_ADDRESS,
  ORE_CHECKPOINT_FEE,
  ORE_INTERMISSION_SLOTS,
  ORE_MINER_SEED,
  ORE_BOARD_SEED,
  ORE_ROUND_SEED,
  ORE_CONFIG_SEED,
  ORE_AUTOMATION_SEED,
  ORE_TREASURY_SEED,
  
  // Entropy
  ENTROPY_PROGRAM_ID,
  ENTROPY_VAR_SEED,
  
  // SPL
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  SYSTEM_PROGRAM_ID,
  
  // Discriminators
  MANAGER_DISCRIMINATOR,
  DEPLOYER_DISCRIMINATOR,
  EvoreInstruction,
  
  // Helpers
  MIN_AUTODEPLOY_BALANCE_FIRST,
  MIN_AUTODEPLOY_BALANCE,
  LAMPORTS_PER_SOL,
};
