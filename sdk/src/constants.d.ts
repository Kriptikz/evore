import { PublicKey } from "@solana/web3.js";

// Evore Program
export declare const EVORE_PROGRAM_ID: PublicKey;
export declare const FEE_COLLECTOR: PublicKey;
export declare const DEPLOY_FEE: bigint;
export declare const MANAGED_MINER_AUTH_SEED: string;
export declare const DEPLOYER_SEED: string;

// ORE Program
export declare const ORE_PROGRAM_ID: PublicKey;
export declare const ORE_MINT_ADDRESS: PublicKey;
export declare const ORE_TREASURY_ADDRESS: PublicKey;
export declare const ORE_CHECKPOINT_FEE: bigint;
export declare const ORE_INTERMISSION_SLOTS: bigint;
export declare const ORE_MINER_SEED: string;
export declare const ORE_BOARD_SEED: string;
export declare const ORE_ROUND_SEED: string;
export declare const ORE_CONFIG_SEED: string;
export declare const ORE_AUTOMATION_SEED: string;
export declare const ORE_TREASURY_SEED: string;

// Entropy Program
export declare const ENTROPY_PROGRAM_ID: PublicKey;
export declare const ENTROPY_VAR_SEED: string;

// SPL Token Programs
export declare const TOKEN_PROGRAM_ID: PublicKey;
export declare const ASSOCIATED_TOKEN_PROGRAM_ID: PublicKey;
export declare const SYSTEM_PROGRAM_ID: PublicKey;

// Account Discriminators
export declare const MANAGER_DISCRIMINATOR: number;
export declare const DEPLOYER_DISCRIMINATOR: number;

// Instruction Discriminators
export declare const EvoreInstruction: {
  CreateManager: number;
  MMDeploy: number;
  MMCheckpoint: number;
  MMClaimSOL: number;
  MMClaimORE: number;
  CreateDeployer: number;
  UpdateDeployer: number;
  MMAutodeploy: number;
  DepositAutodeployBalance: number;
  RecycleSol: number;
  WithdrawAutodeployBalance: number;
  MMAutocheckpoint: number;
  MMFullAutodeploy: number;
  TransferManager: number;
};

// Helpers
export declare const MIN_AUTODEPLOY_BALANCE_FIRST: bigint;
export declare const MIN_AUTODEPLOY_BALANCE: bigint;
export declare const LAMPORTS_PER_SOL: bigint;
