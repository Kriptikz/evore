import {
  PublicKey,
  TransactionInstruction,
  SystemProgram,
} from "@solana/web3.js";
import { 
  EVORE_PROGRAM_ID, 
  ORE_PROGRAM_ID,
  ORE_TREASURY_ADDRESS,
  ORE_MINT_ADDRESS,
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
} from "./constants";
import { 
  getDeployerPda, 
  getAutodeployBalancePda,
  getManagedMinerAuthPda,
  getOreMinerPda,
  getOreBoardPda,
  getOreRoundPda,
  getOreTreasuryPda,
  getOreTokenAddress,
} from "./pda";

// Instruction discriminators (must match program/src/instruction.rs)
enum EvoreInstruction {
  CreateManager = 0,
  MMDeploy = 1,
  MMCheckpoint = 2,
  MMClaimSOL = 3,
  MMClaimORE = 4,
  CreateDeployer = 5,
  UpdateDeployer = 6,
  MMAutodeploy = 7,
  DepositAutodeployBalance = 8,
  RecycleSol = 9,
  WithdrawAutodeployBalance = 10,
  MMAutocheckpoint = 11,
}

/**
 * Creates a CreateManager instruction
 * Note: managerAccount must also sign the transaction (it's a new keypair)
 */
export function createManagerInstruction(
  signer: PublicKey,
  managerAccount: PublicKey
): TransactionInstruction {
  // Just the discriminator byte, no additional data
  const data = Buffer.from([EvoreInstruction.CreateManager]);

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys: [
      { pubkey: signer, isSigner: true, isWritable: true },
      { pubkey: managerAccount, isSigner: true, isWritable: true }, // manager must sign!
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data,
  });
}

/**
 * Creates a CreateDeployer instruction
 * @param bpsFee Percentage fee in basis points (1000 = 10%)
 * @param flatFee Flat fee in lamports (added on top of bpsFee)
 */
export function createDeployerInstruction(
  signer: PublicKey,
  managerAccount: PublicKey,
  deployAuthority: PublicKey,
  bpsFee: bigint,
  flatFee: bigint = BigInt(0)
): TransactionInstruction {
  const [deployerPda] = getDeployerPda(managerAccount);
  
  // Build instruction data: 1 byte discriminator + 8 bytes bps_fee + 8 bytes flat_fee
  const data = Buffer.alloc(17);
  data.writeUInt8(5, 0);  // CreateDeployer = 5
  data.writeBigUInt64LE(bpsFee, 1);
  data.writeBigUInt64LE(flatFee, 9);

  // Debug: log the instruction data
  console.log("CreateDeployer instruction data:", {
    discriminator: data[0],
    bpsFee: bpsFee.toString(),
    flatFee: flatFee.toString(),
    rawBytes: Array.from(data),
    managerAccount: managerAccount.toBase58(),
    deployerPda: deployerPda.toBase58(),
    deployAuthority: deployAuthority.toBase58(),
  });

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys: [
      { pubkey: signer, isSigner: true, isWritable: true },
      { pubkey: managerAccount, isSigner: false, isWritable: true },
      { pubkey: deployerPda, isSigner: false, isWritable: true },
      { pubkey: deployAuthority, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data,
  });
}

/**
 * Creates an UpdateDeployer instruction
 * @param newBpsFee Percentage fee in basis points (1000 = 10%)
 * @param newFlatFee Flat fee in lamports (added on top of bpsFee)
 */
export function updateDeployerInstruction(
  signer: PublicKey,
  managerAccount: PublicKey,
  newDeployAuthority: PublicKey,
  newBpsFee: bigint,
  newFlatFee: bigint = BigInt(0)
): TransactionInstruction {
  const [deployerPda] = getDeployerPda(managerAccount);
  
  // 1 byte discriminator + 8 bytes bps_fee + 8 bytes flat_fee
  const data = Buffer.alloc(1 + 8 + 8);
  data[0] = EvoreInstruction.UpdateDeployer;
  data.writeBigUInt64LE(newBpsFee, 1);
  data.writeBigUInt64LE(newFlatFee, 9);

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys: [
      { pubkey: signer, isSigner: true, isWritable: true },
      { pubkey: managerAccount, isSigner: false, isWritable: true },
      { pubkey: deployerPda, isSigner: false, isWritable: true },
      { pubkey: newDeployAuthority, isSigner: false, isWritable: false },
    ],
    data,
  });
}

/**
 * Creates a DepositAutodeployBalance instruction
 */
export function depositAutodeployBalanceInstruction(
  signer: PublicKey,
  managerAccount: PublicKey,
  amount: bigint
): TransactionInstruction {
  const [deployerPda] = getDeployerPda(managerAccount);
  const [autodeployBalancePda] = getAutodeployBalancePda(deployerPda);
  
  // 1 byte discriminator + 8 bytes amount
  const data = Buffer.alloc(1 + 8);
  data[0] = EvoreInstruction.DepositAutodeployBalance;
  data.writeBigUInt64LE(amount, 1);

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys: [
      { pubkey: signer, isSigner: true, isWritable: true },
      { pubkey: managerAccount, isSigner: false, isWritable: true },
      { pubkey: deployerPda, isSigner: false, isWritable: true },
      { pubkey: autodeployBalancePda, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data,
  });
}

/**
 * Creates a WithdrawAutodeployBalance instruction
 */
export function withdrawAutodeployBalanceInstruction(
  signer: PublicKey,
  managerAccount: PublicKey,
  amount: bigint
): TransactionInstruction {
  const [deployerPda] = getDeployerPda(managerAccount);
  const [autodeployBalancePda] = getAutodeployBalancePda(deployerPda);
  
  // 1 byte discriminator + 8 bytes amount
  const data = Buffer.alloc(1 + 8);
  data[0] = EvoreInstruction.WithdrawAutodeployBalance;
  data.writeBigUInt64LE(amount, 1);

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys: [
      { pubkey: signer, isSigner: true, isWritable: true },
      { pubkey: managerAccount, isSigner: false, isWritable: true },
      { pubkey: deployerPda, isSigner: false, isWritable: true },
      { pubkey: autodeployBalancePda, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data,
  });
}

/**
 * Creates an MMCheckpoint instruction
 * Checkpoints the miner to claim winnings from a round
 */
export function mmCheckpointInstruction(
  signer: PublicKey,
  managerAccount: PublicKey,
  roundId: bigint,
  authId: bigint = BigInt(0)
): TransactionInstruction {
  const [managedMinerAuth, bump] = getManagedMinerAuthPda(managerAccount, authId);
  const [oreMiner] = getOreMinerPda(managedMinerAuth);
  const [oreBoard] = getOreBoardPda();
  const [oreRound] = getOreRoundPda(roundId);
  
  // 1 byte discriminator + 8 bytes auth_id + 1 byte bump
  const data = Buffer.alloc(1 + 8 + 1);
  data[0] = EvoreInstruction.MMCheckpoint;
  data.writeBigUInt64LE(authId, 1);
  data[9] = bump;

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys: [
      { pubkey: signer, isSigner: true, isWritable: true },
      { pubkey: managerAccount, isSigner: false, isWritable: true },
      { pubkey: managedMinerAuth, isSigner: false, isWritable: true },
      { pubkey: oreMiner, isSigner: false, isWritable: true },
      { pubkey: ORE_TREASURY_ADDRESS, isSigner: false, isWritable: true },
      { pubkey: oreBoard, isSigner: false, isWritable: true },
      { pubkey: oreRound, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: ORE_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data,
  });
}

/**
 * Creates an MMClaimSOL instruction
 * Claims SOL rewards from the miner to the manager authority
 */
export function mmClaimSolInstruction(
  signer: PublicKey,
  managerAccount: PublicKey,
  authId: bigint = BigInt(0)
): TransactionInstruction {
  const [managedMinerAuth, bump] = getManagedMinerAuthPda(managerAccount, authId);
  const [oreMiner] = getOreMinerPda(managedMinerAuth);
  
  // 1 byte discriminator + 8 bytes auth_id + 1 byte bump
  const data = Buffer.alloc(1 + 8 + 1);
  data[0] = EvoreInstruction.MMClaimSOL;
  data.writeBigUInt64LE(authId, 1);
  data[9] = bump;

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys: [
      { pubkey: signer, isSigner: true, isWritable: true },
      { pubkey: managerAccount, isSigner: false, isWritable: true },
      { pubkey: managedMinerAuth, isSigner: false, isWritable: true },
      { pubkey: oreMiner, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: ORE_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data,
  });
}

/**
 * Creates an MMClaimORE instruction
 * Claims ORE token rewards from the miner to the signer
 */
export function mmClaimOreInstruction(
  signer: PublicKey,
  managerAccount: PublicKey,
  authId: bigint = BigInt(0)
): TransactionInstruction {
  const [managedMinerAuth, bump] = getManagedMinerAuthPda(managerAccount, authId);
  const [oreMiner] = getOreMinerPda(managedMinerAuth);
  const [treasury] = getOreTreasuryPda();
  const treasuryTokens = getOreTokenAddress(treasury);
  const recipientTokens = getOreTokenAddress(managedMinerAuth);
  const signerTokens = getOreTokenAddress(signer);
  
  // 1 byte discriminator + 8 bytes auth_id + 1 byte bump
  const data = Buffer.alloc(1 + 8 + 1);
  data[0] = EvoreInstruction.MMClaimORE;
  data.writeBigUInt64LE(authId, 1);
  data[9] = bump;

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys: [
      { pubkey: signer, isSigner: true, isWritable: true },
      { pubkey: managerAccount, isSigner: false, isWritable: true },
      { pubkey: managedMinerAuth, isSigner: false, isWritable: true },
      { pubkey: oreMiner, isSigner: false, isWritable: true },
      { pubkey: ORE_MINT_ADDRESS, isSigner: false, isWritable: true },
      { pubkey: recipientTokens, isSigner: false, isWritable: true },
      { pubkey: signerTokens, isSigner: false, isWritable: true },
      { pubkey: treasury, isSigner: false, isWritable: true },
      { pubkey: treasuryTokens, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: ASSOCIATED_TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: ORE_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data,
  });
}
