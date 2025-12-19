const { PublicKey } = require("@solana/web3.js");
const { LAMPORTS_PER_SOL } = require("./constants");

// =============================================================================
// Account Decoders
// =============================================================================

/**
 * Decodes a Manager account from raw account data
 * @param {Buffer|Uint8Array} data - Raw account data from getAccountInfo
 * @returns {{ authority: PublicKey }} - Decoded manager data
 */
function decodeManager(data) {
  // Skip 8-byte discriminator
  const authorityBytes = data.slice(8, 40);
  const authority = new PublicKey(authorityBytes);
  return { authority };
}

/**
 * Decodes a Deployer account from raw account data
 * Size: 112 bytes (8 discriminator + 32 manager_key + 32 deploy_authority + 8 bps_fee + 8 flat_fee + 8 expected_bps_fee + 8 expected_flat_fee + 8 max_per_round)
 * @param {Buffer|Uint8Array} data - Raw account data from getAccountInfo
 * @returns {{ managerKey: PublicKey, deployAuthority: PublicKey, bpsFee: bigint, flatFee: bigint, expectedBpsFee: bigint, expectedFlatFee: bigint, maxPerRound: bigint }}
 */
function decodeDeployer(data) {
  const buffer = Buffer.from(data);
  
  // Skip 8-byte discriminator
  const managerKey = new PublicKey(buffer.slice(8, 40));
  const deployAuthority = new PublicKey(buffer.slice(40, 72));
  const bpsFee = buffer.readBigUInt64LE(72);
  const flatFee = buffer.readBigUInt64LE(80);
  const expectedBpsFee = buffer.readBigUInt64LE(88);
  const expectedFlatFee = buffer.readBigUInt64LE(96);
  const maxPerRound = buffer.readBigUInt64LE(104);
  
  return { managerKey, deployAuthority, bpsFee, flatFee, expectedBpsFee, expectedFlatFee, maxPerRound };
}

/**
 * Decodes an ORE Board account from raw account data
 * @param {Buffer|Uint8Array} data - Raw account data from getAccountInfo
 * @returns {{ roundId: bigint, startSlot: bigint, endSlot: bigint, epochId: bigint }}
 */
function decodeOreBoard(data) {
  const buffer = Buffer.from(data);
  
  // Skip 8-byte discriminator
  const roundId = buffer.readBigUInt64LE(8);
  const startSlot = buffer.readBigUInt64LE(16);
  const endSlot = buffer.readBigUInt64LE(24);
  const epochId = buffer.readBigUInt64LE(32);
  
  return { roundId, startSlot, endSlot, epochId };
}

/**
 * Decodes an ORE Round account from raw account data
 * @param {Buffer|Uint8Array} data - Raw account data from getAccountInfo
 * @returns {Object} - Decoded round data with deployed amounts, counts, etc.
 */
function decodeOreRound(data) {
  const buffer = Buffer.from(data);
  
  // Skip 8-byte discriminator
  let offset = 8;
  
  const id = buffer.readBigUInt64LE(offset);
  offset += 8;
  
  // deployed: [u64; 25]
  const deployed = [];
  for (let i = 0; i < 25; i++) {
    deployed.push(buffer.readBigUInt64LE(offset));
    offset += 8;
  }
  
  // slot_hash: [u8; 32]
  const slotHash = buffer.slice(offset, offset + 32);
  offset += 32;
  
  // count: [u64; 25]
  const count = [];
  for (let i = 0; i < 25; i++) {
    count.push(buffer.readBigUInt64LE(offset));
    offset += 8;
  }
  
  const expiresAt = buffer.readBigUInt64LE(offset);
  offset += 8;
  
  const motherlode = buffer.readBigUInt64LE(offset);
  offset += 8;
  
  const rentPayer = new PublicKey(buffer.slice(offset, offset + 32));
  offset += 32;
  
  const topMiner = new PublicKey(buffer.slice(offset, offset + 32));
  offset += 32;
  
  const topMinerReward = buffer.readBigUInt64LE(offset);
  offset += 8;
  
  const totalDeployed = buffer.readBigUInt64LE(offset);
  offset += 8;
  
  const totalMiners = buffer.readBigUInt64LE(offset);
  offset += 8;
  
  const totalVaulted = buffer.readBigUInt64LE(offset);
  offset += 8;
  
  const totalWinnings = buffer.readBigUInt64LE(offset);
  
  return {
    id,
    deployed,
    slotHash,
    count,
    expiresAt,
    motherlode,
    rentPayer,
    topMiner,
    topMinerReward,
    totalDeployed,
    totalMiners,
    totalVaulted,
    totalWinnings,
  };
}

/**
 * Decodes an ORE Miner account from raw account data
 * @param {Buffer|Uint8Array} data - Raw account data from getAccountInfo
 * @returns {Object} - Decoded miner data
 */
function decodeOreMiner(data) {
  const buffer = Buffer.from(data);
  
  // Skip 8-byte discriminator
  let offset = 8;
  
  const authority = new PublicKey(buffer.slice(offset, offset + 32));
  offset += 32;
  
  // deployed: [u64; 25]
  const deployed = [];
  for (let i = 0; i < 25; i++) {
    deployed.push(buffer.readBigUInt64LE(offset));
    offset += 8;
  }
  
  // cumulative: [u64; 25]
  const cumulative = [];
  for (let i = 0; i < 25; i++) {
    cumulative.push(buffer.readBigUInt64LE(offset));
    offset += 8;
  }
  
  const checkpointFee = buffer.readBigUInt64LE(offset);
  offset += 8;
  
  const checkpointId = buffer.readBigUInt64LE(offset);
  offset += 8;
  
  const lastClaimOreAt = buffer.readBigInt64LE(offset);
  offset += 8;
  
  const lastClaimSolAt = buffer.readBigInt64LE(offset);
  offset += 8;
  
  // rewards_factor: Numeric (16 bytes)
  const rewardsFactor = buffer.slice(offset, offset + 16);
  offset += 16;
  
  const rewardsSol = buffer.readBigUInt64LE(offset);
  offset += 8;
  
  const rewardsOre = buffer.readBigUInt64LE(offset);
  offset += 8;
  
  const refinedOre = buffer.readBigUInt64LE(offset);
  offset += 8;
  
  const roundId = buffer.readBigUInt64LE(offset);
  offset += 8;
  
  const lifetimeRewardsSol = buffer.readBigUInt64LE(offset);
  offset += 8;
  
  const lifetimeRewardsOre = buffer.readBigUInt64LE(offset);
  offset += 8;
  
  const lifetimeDeployed = buffer.readBigUInt64LE(offset);
  
  return {
    authority,
    deployed,
    cumulative,
    checkpointFee,
    checkpointId,
    lastClaimOreAt,
    lastClaimSolAt,
    rewardsFactor,
    rewardsSol,
    rewardsOre,
    refinedOre,
    roundId,
    lifetimeRewardsSol,
    lifetimeRewardsOre,
    lifetimeDeployed,
  };
}

// =============================================================================
// Formatting Utilities
// =============================================================================

/**
 * Formats lamports as SOL with specified decimal places
 * @param {bigint|number} lamports - Amount in lamports
 * @param {number} decimals - Number of decimal places (default: 4)
 * @returns {string} - Formatted SOL string
 */
function formatSol(lamports, decimals = 4) {
  const sol = Number(lamports) / Number(LAMPORTS_PER_SOL);
  return sol.toFixed(decimals);
}

/**
 * Formats ORE token amount (11 decimals) with specified decimal places
 * @param {bigint|number} amount - Amount in smallest ORE units
 * @param {number} decimals - Number of decimal places (default: 4)
 * @returns {string} - Formatted ORE string
 */
function formatOre(amount, decimals = 4) {
  const ore = Number(amount) / 100_000_000_000; // 11 decimals
  return ore.toFixed(decimals);
}

/**
 * Formats basis points as percentage
 * @param {bigint|number} bps - Basis points (1000 = 10%)
 * @returns {string} - Formatted percentage string
 */
function formatBps(bps) {
  return `${Number(bps) / 100}%`;
}

/**
 * Formats deployer fees (both bps and flat are additive)
 * @param {bigint|number} bpsFee - Percentage fee in basis points
 * @param {bigint|number} flatFee - Flat fee in lamports
 * @returns {string} - Human-readable fee description
 */
function formatFee(bpsFee, flatFee) {
  const parts = [];
  
  if (Number(bpsFee) > 0) {
    parts.push(`${Number(bpsFee) / 100}%`);
  }
  
  if (Number(flatFee) > 0) {
    parts.push(`${flatFee} lamports`);
  }
  
  if (parts.length === 0) {
    return "No fee";
  }
  
  return parts.join(" + ");
}

// =============================================================================
// Parsing Utilities
// =============================================================================

/**
 * Parses SOL string to lamports
 * @param {string} sol - SOL amount as string (e.g., "1.5")
 * @returns {bigint} - Amount in lamports
 */
function parseSolToLamports(sol) {
  const parsed = parseFloat(sol);
  if (isNaN(parsed)) return 0n;
  return BigInt(Math.floor(parsed * Number(LAMPORTS_PER_SOL)));
}

/**
 * Parses percentage string to basis points
 * @param {string} percent - Percentage as string (e.g., "5.5" for 5.5%)
 * @returns {bigint} - Basis points
 */
function parsePercentToBps(percent) {
  const parsed = parseFloat(percent);
  if (isNaN(parsed)) return 0n;
  return BigInt(Math.floor(parsed * 100));
}

/**
 * Shortens a PublicKey for display
 * @param {PublicKey|string} pubkey - PublicKey to shorten
 * @param {number} chars - Number of characters to show on each end (default: 4)
 * @returns {string} - Shortened pubkey string
 */
function shortenPubkey(pubkey, chars = 4) {
  const str = pubkey.toString();
  return `${str.slice(0, chars)}...${str.slice(-chars)}`;
}

/**
 * Calculates the deployer fee for a given deployment
 * @param {bigint} totalDeployed - Total lamports being deployed
 * @param {bigint} bpsFee - Basis points fee
 * @param {bigint} flatFee - Flat fee in lamports
 * @returns {bigint} - Total fee in lamports
 */
function calculateDeployerFee(totalDeployed, bpsFee, flatFee) {
  const bpsFeeAmount = (totalDeployed * bpsFee) / 10000n;
  return bpsFeeAmount + flatFee;
}

module.exports = {
  // Decoders
  decodeManager,
  decodeDeployer,
  decodeOreBoard,
  decodeOreRound,
  decodeOreMiner,
  
  // Formatting
  formatSol,
  formatOre,
  formatBps,
  formatFee,
  
  // Parsing
  parseSolToLamports,
  parsePercentToBps,
  shortenPubkey,
  calculateDeployerFee,
};
