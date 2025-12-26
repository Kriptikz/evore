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
export const DEFAULT_DEPLOYER_BPS_FEE = parseInt(process.env.NEXT_PUBLIC_DEPLOYER_BPS_FEE || "0"); // Default 5% (500 bps)
export const DEFAULT_DEPLOYER_FLAT_FEE = parseInt(process.env.NEXT_PUBLIC_DEPLOYER_FLAT_FEE || "715"); // Default 715 lamports

// ============================================================================
// API Configuration
// ============================================================================

/**
 * ORE Stats API URL - primary data source for ALL reads:
 * - ORE accounts (Board, Round, Treasury, Miners)
 * - EVORE accounts (Managers, Deployers, Auth balances) [Phase 1b]
 * - SOL balances
 * - ORE token balances
 * - Admin operations
 * 
 * The frontend should NEVER make direct RPC calls for reading data.
 * All reads go through ore-stats for caching, rate limiting, and efficiency.
 */
export const API_URL = process.env.NEXT_PUBLIC_API_URL || "http://localhost:3000";

/**
 * Rate limiting - respect ore-stats server limits.
 * The API client enforces these limits to avoid 429 errors.
 */
export const API_RATE_LIMIT = {
  requestsPerSecond: 2,
  minDelayMs: 500,
};

/**
 * NOTE: RPC_URL (in WalletProvider) is ONLY for transaction operations:
 * - getLatestBlockhash
 * - sendTransaction  
 * - confirmTransaction
 * 
 * It's required by wallet-adapter but should be a minimal/free endpoint.
 * Use Solana's public RPC or a basic plan - no heavy reads go through it.
 */
