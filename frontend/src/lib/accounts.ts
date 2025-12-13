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
  bpsFee: bigint;  // Percentage fee in basis points (1000 = 10%)
  flatFee: bigint; // Flat fee in lamports (added on top of bpsFee)
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
  const bpsFee = data.readBigUInt64LE(72);
  const flatFee = data.readBigUInt64LE(80);
  return { managerKey, deployAuthority, bpsFee, flatFee };
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
 * Formats deployer fees (both bps and flat are additive)
 * @param bpsFee Percentage fee in basis points
 * @param flatFee Flat fee in lamports
 */
export function formatFee(bpsFee: bigint | number, flatFee: bigint | number): string {
  const parts: string[] = [];
  
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
