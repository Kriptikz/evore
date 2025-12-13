import { PublicKey } from "@solana/web3.js";

/**
 * Decoded Manager account data
 */
export interface Manager {
  authority: PublicKey;
}

/**
 * Decoded Deployer account data
 */
export interface Deployer {
  managerKey: PublicKey;
  deployAuthority: PublicKey;
  feeBps: bigint;
}

/**
 * Decodes a Manager account from raw data
 */
export function decodeManager(data: Buffer): Manager {
  // Skip 8-byte discriminator
  const authority = new PublicKey(data.subarray(8, 40));
  return { authority };
}

/**
 * Decodes a Deployer account from raw data
 */
export function decodeDeployer(data: Buffer): Deployer {
  // Skip 8-byte discriminator
  const managerKey = new PublicKey(data.subarray(8, 40));
  const deployAuthority = new PublicKey(data.subarray(40, 72));
  const feeBps = data.readBigUInt64LE(72);
  return { managerKey, deployAuthority, feeBps };
}

/**
 * Formats lamports as SOL with specified decimal places
 */
export function formatSol(lamports: bigint | number, decimals: number = 4): string {
  const sol = Number(lamports) / 1_000_000_000;
  return sol.toFixed(decimals);
}

/**
 * Formats ORE token amount (11 decimals) with specified decimal places
 */
export function formatOre(amount: bigint | number, decimals: number = 4): string {
  const ore = Number(amount) / 100_000_000_000; // 11 decimals
  return ore.toFixed(decimals);
}

/**
 * Parses SOL string to lamports
 */
export function parseSolToLamports(sol: string): bigint {
  const parsed = parseFloat(sol);
  if (isNaN(parsed)) return BigInt(0);
  return BigInt(Math.floor(parsed * 1_000_000_000));
}

/**
 * Formats basis points as percentage
 */
export function formatBps(bps: bigint | number): string {
  return `${Number(bps) / 100}%`;
}

/**
 * Parses percentage string to basis points
 */
export function parsePercentToBps(percent: string): bigint {
  const parsed = parseFloat(percent);
  if (isNaN(parsed)) return BigInt(0);
  return BigInt(Math.floor(parsed * 100));
}

/**
 * Shortens a public key for display
 */
export function shortenPubkey(pubkey: PublicKey | string, chars: number = 4): string {
  const str = typeof pubkey === 'string' ? pubkey : pubkey.toBase58();
  return `${str.slice(0, chars)}...${str.slice(-chars)}`;
}
