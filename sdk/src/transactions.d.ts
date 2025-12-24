import { PublicKey, TransactionInstruction } from "@solana/web3.js";

// Setup
// bpsFee and flatFee are the max fees user accepts (deployer can charge up to these)
export declare function buildCreateAutoMinerInstructions(
  signer: PublicKey,
  managerAccount: PublicKey,
  deployAuthority: PublicKey,
  /** Max bps fee user accepts */
  bpsFee: bigint,
  /** Max flat fee user accepts */
  flatFee?: bigint
): TransactionInstruction[];

export declare function buildSetupAutoMinerInstructions(
  signer: PublicKey,
  managerAccount: PublicKey,
  deployAuthority: PublicKey,
  /** Max bps fee user accepts */
  bpsFee: bigint,
  /** Max flat fee user accepts */
  flatFee: bigint,
  depositAmount: bigint
): TransactionInstruction[];

// Deposit/Withdraw
export declare function buildDepositInstructions(
  signer: PublicKey,
  manager: PublicKey,
  amount: bigint
): TransactionInstruction[];

export declare function buildWithdrawInstructions(
  signer: PublicKey,
  manager: PublicKey,
  amount: bigint
): TransactionInstruction[];

// Claims
export declare function buildCheckpointAndClaimSolInstructions(
  signer: PublicKey,
  manager: PublicKey,
  roundId: bigint,
  authId?: bigint
): TransactionInstruction[];

export declare function buildClaimSolInstructions(
  signer: PublicKey,
  manager: PublicKey,
  authId?: bigint
): TransactionInstruction[];

export declare function buildClaimOreInstructions(
  signer: PublicKey,
  manager: PublicKey,
  authId?: bigint
): TransactionInstruction[];

export declare function buildClaimAllInstructions(
  signer: PublicKey,
  manager: PublicKey,
  authId?: bigint
): TransactionInstruction[];

// Executor/Crank
export declare function buildAutodeployInstructions(
  executor: PublicKey,
  manager: PublicKey,
  authId: bigint,
  roundId: bigint,
  amount: bigint,
  squaresMask: number,
  expectedBpsFee?: bigint,
  expectedFlatFee?: bigint
): TransactionInstruction[];

export declare function buildCheckpointAndAutodeployInstructions(
  executor: PublicKey,
  manager: PublicKey,
  authId: bigint,
  checkpointRoundId: bigint,
  deployRoundId: bigint,
  amount: bigint,
  squaresMask: number,
  expectedBpsFee?: bigint,
  expectedFlatFee?: bigint
): TransactionInstruction[];

export declare function buildRecycleSolInstructions(
  executor: PublicKey,
  manager: PublicKey,
  authId: bigint
): TransactionInstruction[];

interface DeployConfig {
  manager: PublicKey;
  authId: bigint;
  amount: bigint;
  squaresMask: number;
  expectedBpsFee?: bigint;
  expectedFlatFee?: bigint;
}

export declare function buildBatchedAutodeployInstructions(
  executor: PublicKey,
  deploys: DeployConfig[],
  roundId: bigint
): TransactionInstruction[];
