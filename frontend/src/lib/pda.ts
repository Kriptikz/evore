import { PublicKey } from "@solana/web3.js";
import { 
  EVORE_PROGRAM_ID, 
  ORE_PROGRAM_ID, 
  ORE_MINT_ADDRESS,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  MANAGED_MINER_AUTH_SEED, 
  DEPLOYER_SEED, 
  AUTODEPLOY_BALANCE_SEED, 
  ORE_MINER_SEED,
  ORE_BOARD_SEED,
  ORE_ROUND_SEED,
  ORE_TREASURY_SEED,
} from "./constants";

/**
 * Derives the managed miner auth PDA for a manager and auth_id
 */
export function getManagedMinerAuthPda(manager: PublicKey, authId: bigint): [PublicKey, number] {
  const authIdBuffer = Buffer.alloc(8);
  authIdBuffer.writeBigUInt64LE(authId);
  
  return PublicKey.findProgramAddressSync(
    [Buffer.from(MANAGED_MINER_AUTH_SEED), manager.toBuffer(), authIdBuffer],
    EVORE_PROGRAM_ID
  );
}

/**
 * Derives the ORE miner PDA for a managed miner auth
 */
export function getOreMinerPda(authority: PublicKey): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from(ORE_MINER_SEED), authority.toBuffer()],
    ORE_PROGRAM_ID
  );
}

/**
 * Derives the deployer PDA for a manager
 */
export function getDeployerPda(manager: PublicKey): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from(DEPLOYER_SEED), manager.toBuffer()],
    EVORE_PROGRAM_ID
  );
}

/**
 * Derives the autodeploy balance PDA for a deployer
 */
export function getAutodeployBalancePda(deployer: PublicKey): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from(AUTODEPLOY_BALANCE_SEED), deployer.toBuffer()],
    EVORE_PROGRAM_ID
  );
}

/**
 * Derives the ORE board PDA
 */
export function getOreBoardPda(): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from(ORE_BOARD_SEED)],
    ORE_PROGRAM_ID
  );
}

/**
 * Derives the ORE round PDA for a round ID
 */
export function getOreRoundPda(roundId: bigint): [PublicKey, number] {
  const roundIdBuffer = Buffer.alloc(8);
  roundIdBuffer.writeBigUInt64LE(roundId);
  
  return PublicKey.findProgramAddressSync(
    [Buffer.from(ORE_ROUND_SEED), roundIdBuffer],
    ORE_PROGRAM_ID
  );
}

/**
 * Derives the ORE treasury PDA
 */
export function getOreTreasuryPda(): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from(ORE_TREASURY_SEED)],
    ORE_PROGRAM_ID
  );
}

/**
 * Derives the associated token address for a wallet and mint
 */
export function getAssociatedTokenAddress(wallet: PublicKey, mint: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [wallet.toBuffer(), TOKEN_PROGRAM_ID.toBuffer(), mint.toBuffer()],
    ASSOCIATED_TOKEN_PROGRAM_ID
  )[0];
}

/**
 * Derives the ORE token address for a wallet
 */
export function getOreTokenAddress(wallet: PublicKey): PublicKey {
  return getAssociatedTokenAddress(wallet, ORE_MINT_ADDRESS);
}
