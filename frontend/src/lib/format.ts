/**
 * Shared formatting utilities for consistent number display across the app.
 */

export const LAMPORTS_PER_SOL = 1_000_000_000;
export const ORE_DECIMALS = 11;

/**
 * Format lamports to SOL with consistent decimal places.
 * Always shows 4 decimal places for consistency in lists/tables.
 * Uses comma separators for thousands.
 */
export function formatSol(lamports: number, options?: { decimals?: number; showUnit?: boolean }): string {
  const decimals = options?.decimals ?? 4;
  const showUnit = options?.showUnit ?? true;
  const sol = lamports / LAMPORTS_PER_SOL;
  
  // Format with fixed decimals and add thousand separators
  const formatted = sol.toLocaleString(undefined, {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  });
  
  return showUnit ? `${formatted} SOL` : formatted;
}

/**
 * Format atomic ORE to ORE with consistent decimal places.
 * Always shows 4 decimal places for consistency in lists/tables.
 * Uses comma separators for thousands.
 */
export function formatOre(atomic: number, options?: { decimals?: number; showUnit?: boolean }): string {
  const decimals = options?.decimals ?? 4;
  const showUnit = options?.showUnit ?? true;
  const ore = atomic / Math.pow(10, ORE_DECIMALS);
  
  // Format with fixed decimals and add thousand separators
  const formatted = ore.toLocaleString(undefined, {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  });
  
  return showUnit ? `${formatted} ORE` : formatted;
}

/**
 * Format a number with consistent decimal places and thousand separators.
 */
export function formatNumber(value: number, options?: { decimals?: number }): string {
  const decimals = options?.decimals ?? 2;
  
  return value.toLocaleString(undefined, {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  });
}

/**
 * Truncate a Solana address for display.
 */
export function truncateAddress(addr: string, chars: number = 6): string {
  if (addr.length <= chars * 2 + 3) return addr;
  return `${addr.slice(0, chars)}...${addr.slice(-4)}`;
}

