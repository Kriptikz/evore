// Re-export all constants from the SDK
export {
  EVORE_PROGRAM_ID,
  ORE_PROGRAM_ID,
  ORE_MINT_ADDRESS,
  ORE_TREASURY_ADDRESS,
  FEE_COLLECTOR,
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  MANAGED_MINER_AUTH_SEED,
  DEPLOYER_SEED,
  ORE_MINER_SEED,
  ORE_BOARD_SEED,
  ORE_ROUND_SEED,
  ORE_TREASURY_SEED,
  MANAGER_DISCRIMINATOR,
  DEPLOYER_DISCRIMINATOR,
  DEPLOY_FEE,
  MIN_AUTODEPLOY_BALANCE_FIRST,
  MIN_AUTODEPLOY_BALANCE,
  LAMPORTS_PER_SOL,
} from "evore-sdk";

// Frontend-specific constants from environment
export const DEFAULT_DEPLOYER_PUBKEY = process.env.NEXT_PUBLIC_DEPLOYER_PUBKEY || "";
export const DEFAULT_DEPLOYER_BPS_FEE = parseInt(process.env.NEXT_PUBLIC_DEPLOYER_BPS_FEE || "500"); // Default 5% (500 bps)
export const DEFAULT_DEPLOYER_FLAT_FEE = parseInt(process.env.NEXT_PUBLIC_DEPLOYER_FLAT_FEE || "0"); // Default 0 lamports

// Stats server URL (for when using stats-server instead of direct RPC)
export const STATS_SERVER_URL = process.env.NEXT_PUBLIC_STATS_SERVER_URL || "http://localhost:3001";
