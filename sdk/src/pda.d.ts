import { PublicKey } from "@solana/web3.js";

// Helper
export declare function bigintToLeBytes(value: bigint): Buffer;

// Evore PDAs
export declare function getManagedMinerAuthPda(manager: PublicKey, authId: bigint): [PublicKey, number];
export declare function getDeployerPda(manager: PublicKey): [PublicKey, number];

// ORE PDAs
export declare function getOreMinerPda(authority: PublicKey): [PublicKey, number];
export declare function getOreBoardPda(): [PublicKey, number];
export declare function getOreRoundPda(roundId: bigint): [PublicKey, number];
export declare function getOreConfigPda(): [PublicKey, number];
export declare function getOreAutomationPda(authority: PublicKey): [PublicKey, number];
export declare function getOreTreasuryPda(): [PublicKey, number];

// Entropy PDAs
export declare function getEntropyVarPda(authority: PublicKey, id: bigint): [PublicKey, number];

// Token PDAs
export declare function getAssociatedTokenAddress(wallet: PublicKey, mint: PublicKey): PublicKey;
export declare function getOreTokenAddress(wallet: PublicKey): PublicKey;
