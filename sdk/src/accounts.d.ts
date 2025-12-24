import { PublicKey } from "@solana/web3.js";

// Account Types
export interface Manager {
  authority: PublicKey;
}

export interface Deployer {
  managerKey: PublicKey;
  deployAuthority: PublicKey;
  /** Actual bps fee charged (set by deploy authority, must be <= expectedBpsFee) */
  bpsFee: bigint;
  /** Actual flat fee charged (set by deploy authority, must be <= expectedFlatFee) */
  flatFee: bigint;
  /** Max bps fee user accepts (set by manager) */
  expectedBpsFee: bigint;
  /** Max flat fee user accepts (set by manager) */
  expectedFlatFee: bigint;
  maxPerRound: bigint;
}

// Account Decoders
export declare function decodeManager(data: Buffer | Uint8Array): Manager;

export declare function decodeDeployer(data: Buffer | Uint8Array): Deployer;

export declare function decodeOreBoard(data: Buffer | Uint8Array): {
  roundId: bigint;
  startSlot: bigint;
  endSlot: bigint;
  epochId: bigint;
};

export declare function decodeOreRound(data: Buffer | Uint8Array): {
  id: bigint;
  deployed: bigint[];
  slotHash: Buffer;
  count: bigint[];
  expiresAt: bigint;
  motherlode: bigint;
  rentPayer: PublicKey;
  topMiner: PublicKey;
  topMinerReward: bigint;
  totalDeployed: bigint;
  totalMiners: bigint;
  totalVaulted: bigint;
  totalWinnings: bigint;
};

export declare function decodeOreMiner(data: Buffer | Uint8Array): {
  authority: PublicKey;
  deployed: bigint[];
  cumulative: bigint[];
  checkpointFee: bigint;
  checkpointId: bigint;
  lastClaimOreAt: bigint;
  lastClaimSolAt: bigint;
  rewardsFactor: Buffer;
  rewardsSol: bigint;
  rewardsOre: bigint;
  refinedOre: bigint;
  roundId: bigint;
  lifetimeRewardsSol: bigint;
  lifetimeRewardsOre: bigint;
  lifetimeDeployed: bigint;
};

// Formatting
export declare function formatSol(lamports: bigint | number, decimals?: number): string;
export declare function formatOre(amount: bigint | number, decimals?: number): string;
export declare function formatBps(bps: bigint | number): string;
export declare function formatFee(bpsFee: bigint | number, flatFee: bigint | number): string;

// Parsing
export declare function parseSolToLamports(sol: string): bigint;
export declare function parsePercentToBps(percent: string): bigint;
export declare function shortenPubkey(pubkey: PublicKey | string, chars?: number): string;
export declare function calculateDeployerFee(totalDeployed: bigint, bpsFee: bigint, flatFee: bigint): bigint;
