import { PublicKey } from "@solana/web3.js";

// Evore program ID
export const EVORE_PROGRAM_ID = new PublicKey("8jaLKWLJAj5jVCZbxpe3zRUvLB3LD48MRtaQ2AjfCfxa");

// ORE program ID (v3)
export const ORE_PROGRAM_ID = new PublicKey("oreV3EG1i9BEgiAJ8b177Z2S2rMarzak4NMv1kULvWv");

// ORE addresses
export const ORE_TREASURY_ADDRESS = new PublicKey("45db2FSR4mcXdSVVZbKbwojU6uYDpMyhpEi7cC8nHaWG");
export const ORE_MINT_ADDRESS = new PublicKey("oreoU2P8bN6jkk3jbaiVxYnG1dCXcYxwhwyK9jSybcp");

// Fee collector
export const FEE_COLLECTOR = new PublicKey("56qSi79jWdM1zie17NKFvdsh213wPb15HHUqGUjmJ2Lr");

// Evore PDA seeds
export const MANAGED_MINER_AUTH_SEED = "managed-miner-auth";
export const DEPLOYER_SEED = "deployer";
export const AUTODEPLOY_BALANCE_SEED = "autodeploy-balance";

// ORE PDA seeds
export const ORE_MINER_SEED = "miner";
export const ORE_BOARD_SEED = "board";
export const ORE_ROUND_SEED = "round";
export const ORE_TREASURY_SEED = "treasury";

// Token program ID
export const TOKEN_PROGRAM_ID = new PublicKey("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
export const ASSOCIATED_TOKEN_PROGRAM_ID = new PublicKey("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");

// Account discriminators
export const MANAGER_DISCRIMINATOR = 100;
export const DEPLOYER_DISCRIMINATOR = 101;

// Fees
export const DEPLOY_FEE = 50_000; // 0.00005 SOL

// Minimum autodeploy balance for deployments (conservative estimate)
// Includes: AUTH_PDA_RENT + CHECKPOINT_FEE + potential MINER_RENT + AUTODEPLOY_BALANCE_RENT + buffer
// First deploy needs more (~0.007 SOL), subsequent deploys need less (~0.004 SOL)
export const MIN_AUTODEPLOY_BALANCE_FIRST = 7_000_000; // 0.007 SOL (first deploy with miner creation)
export const MIN_AUTODEPLOY_BALANCE = 4_000_000; // 0.004 SOL (subsequent deploys)

// Default deployer settings from env
export const DEFAULT_DEPLOYER_PUBKEY = process.env.NEXT_PUBLIC_DEPLOYER_PUBKEY || "";
export const DEFAULT_DEPLOYER_BPS_FEE = parseInt(process.env.NEXT_PUBLIC_DEPLOYER_BPS_FEE || "500"); // Default 5% (500 bps)
export const DEFAULT_DEPLOYER_FLAT_FEE = parseInt(process.env.NEXT_PUBLIC_DEPLOYER_FLAT_FEE || "0"); // Default 0 lamports

// Stats server URL (for when using stats-server instead of direct RPC)
export const STATS_SERVER_URL = process.env.NEXT_PUBLIC_STATS_SERVER_URL || "http://localhost:3001";
