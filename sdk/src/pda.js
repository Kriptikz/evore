const { PublicKey } = require("@solana/web3.js");
const {
  EVORE_PROGRAM_ID,
  ORE_PROGRAM_ID,
  ENTROPY_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  ORE_MINT_ADDRESS,
  MANAGED_MINER_AUTH_SEED,
  DEPLOYER_SEED,
  ORE_MINER_SEED,
  ORE_BOARD_SEED,
  ORE_ROUND_SEED,
  ORE_CONFIG_SEED,
  ORE_AUTOMATION_SEED,
  ORE_TREASURY_SEED,
  ENTROPY_VAR_SEED,
} = require("./constants");

/**
 * Helper to convert a bigint to 8-byte little-endian Buffer
 * @param {bigint} value
 * @returns {Buffer}
 */
function bigintToLeBytes(value) {
  const buffer = Buffer.alloc(8);
  buffer.writeBigUInt64LE(value);
  return buffer;
}

// =============================================================================
// Evore PDAs
// =============================================================================

/**
 * Derives the managed miner auth PDA for a manager and auth_id
 * @param {PublicKey} manager - The manager account address
 * @param {bigint} authId - The auth ID (allows multiple miners per manager)
 * @returns {[PublicKey, number]} - [PDA address, bump seed]
 */
function getManagedMinerAuthPda(manager, authId) {
  return PublicKey.findProgramAddressSync(
    [
      Buffer.from(MANAGED_MINER_AUTH_SEED),
      manager.toBuffer(),
      bigintToLeBytes(authId),
    ],
    EVORE_PROGRAM_ID
  );
}

/**
 * Derives the deployer PDA for a manager
 * @param {PublicKey} manager - The manager account address
 * @returns {[PublicKey, number]} - [PDA address, bump seed]
 */
function getDeployerPda(manager) {
  return PublicKey.findProgramAddressSync(
    [
      Buffer.from(DEPLOYER_SEED),
      manager.toBuffer(),
    ],
    EVORE_PROGRAM_ID
  );
}

// =============================================================================
// ORE PDAs
// =============================================================================

/**
 * Derives the ORE miner PDA for an authority
 * @param {PublicKey} authority - The miner authority (managed_miner_auth for Evore miners)
 * @returns {[PublicKey, number]} - [PDA address, bump seed]
 */
function getOreMinerPda(authority) {
  return PublicKey.findProgramAddressSync(
    [
      Buffer.from(ORE_MINER_SEED),
      authority.toBuffer(),
    ],
    ORE_PROGRAM_ID
  );
}

/**
 * Derives the ORE board PDA (singleton)
 * @returns {[PublicKey, number]} - [PDA address, bump seed]
 */
function getOreBoardPda() {
  return PublicKey.findProgramAddressSync(
    [Buffer.from(ORE_BOARD_SEED)],
    ORE_PROGRAM_ID
  );
}

/**
 * Derives the ORE round PDA for a round ID
 * @param {bigint} roundId - The round ID
 * @returns {[PublicKey, number]} - [PDA address, bump seed]
 */
function getOreRoundPda(roundId) {
  return PublicKey.findProgramAddressSync(
    [
      Buffer.from(ORE_ROUND_SEED),
      bigintToLeBytes(roundId),
    ],
    ORE_PROGRAM_ID
  );
}

/**
 * Derives the ORE config PDA (singleton)
 * @returns {[PublicKey, number]} - [PDA address, bump seed]
 */
function getOreConfigPda() {
  return PublicKey.findProgramAddressSync(
    [Buffer.from(ORE_CONFIG_SEED)],
    ORE_PROGRAM_ID
  );
}

/**
 * Derives the ORE automation PDA for an authority
 * @param {PublicKey} authority - The automation authority
 * @returns {[PublicKey, number]} - [PDA address, bump seed]
 */
function getOreAutomationPda(authority) {
  return PublicKey.findProgramAddressSync(
    [
      Buffer.from(ORE_AUTOMATION_SEED),
      authority.toBuffer(),
    ],
    ORE_PROGRAM_ID
  );
}

/**
 * Derives the ORE treasury PDA (singleton)
 * @returns {[PublicKey, number]} - [PDA address, bump seed]
 */
function getOreTreasuryPda() {
  return PublicKey.findProgramAddressSync(
    [Buffer.from(ORE_TREASURY_SEED)],
    ORE_PROGRAM_ID
  );
}

// =============================================================================
// Entropy PDAs
// =============================================================================

/**
 * Derives the Entropy var PDA
 * @param {PublicKey} authority - The authority (board address for ORE)
 * @param {bigint} id - The var ID (usually 0)
 * @returns {[PublicKey, number]} - [PDA address, bump seed]
 */
function getEntropyVarPda(authority, id) {
  return PublicKey.findProgramAddressSync(
    [
      Buffer.from(ENTROPY_VAR_SEED),
      authority.toBuffer(),
      bigintToLeBytes(id),
    ],
    ENTROPY_PROGRAM_ID
  );
}

// =============================================================================
// Token PDAs
// =============================================================================

/**
 * Derives the associated token address for a wallet and mint
 * @param {PublicKey} wallet - The wallet address
 * @param {PublicKey} mint - The token mint address
 * @returns {PublicKey} - The associated token address
 */
function getAssociatedTokenAddress(wallet, mint) {
  const [address] = PublicKey.findProgramAddressSync(
    [
      wallet.toBuffer(),
      TOKEN_PROGRAM_ID.toBuffer(),
      mint.toBuffer(),
    ],
    ASSOCIATED_TOKEN_PROGRAM_ID
  );
  return address;
}

/**
 * Derives the ORE token address for a wallet
 * @param {PublicKey} wallet - The wallet address
 * @returns {PublicKey} - The ORE token address
 */
function getOreTokenAddress(wallet) {
  return getAssociatedTokenAddress(wallet, ORE_MINT_ADDRESS);
}

module.exports = {
  // Evore PDAs
  getManagedMinerAuthPda,
  getDeployerPda,
  
  // ORE PDAs
  getOreMinerPda,
  getOreBoardPda,
  getOreRoundPda,
  getOreConfigPda,
  getOreAutomationPda,
  getOreTreasuryPda,
  
  // Entropy PDAs
  getEntropyVarPda,
  
  // Token PDAs
  getAssociatedTokenAddress,
  getOreTokenAddress,
  
  // Helpers (exported for use in instructions)
  bigintToLeBytes,
};
