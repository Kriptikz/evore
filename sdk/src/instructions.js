const { PublicKey, TransactionInstruction, SystemProgram } = require("@solana/web3.js");
const {
  EVORE_PROGRAM_ID,
  ORE_PROGRAM_ID,
  ENTROPY_PROGRAM_ID,
  FEE_COLLECTOR,
  ORE_TREASURY_ADDRESS,
  ORE_MINT_ADDRESS,
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  SYSTEM_PROGRAM_ID,
  EvoreInstruction,
} = require("./constants");
const {
  getManagedMinerAuthPda,
  getDeployerPda,
  getOreMinerPda,
  getOreBoardPda,
  getOreRoundPda,
  getOreConfigPda,
  getOreAutomationPda,
  getOreTreasuryPda,
  getEntropyVarPda,
  getOreTokenAddress,
  bigintToLeBytes,
} = require("./pda");

// =============================================================================
// Manager Instructions
// =============================================================================

/**
 * Creates a CreateManager instruction
 * Note: managerAccount must also sign the transaction (it's a new keypair)
 * @param {PublicKey} signer - The user's wallet (pays for account creation)
 * @param {PublicKey} managerAccount - New keypair for the manager account (must sign)
 * @returns {TransactionInstruction}
 */
function createManagerInstruction(signer, managerAccount) {
  const data = Buffer.from([EvoreInstruction.CreateManager]);

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys: [
      { pubkey: signer, isSigner: true, isWritable: true },
      { pubkey: managerAccount, isSigner: true, isWritable: true },
      { pubkey: SYSTEM_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data,
  });
}

/**
 * Creates a TransferManager instruction
 * Transfers manager authority to a new public key.
 * Note: This transfers all associated mining accounts (deployer, miner, automation, etc.)
 * @param {PublicKey} signer - Current manager authority
 * @param {PublicKey} manager - The manager account to transfer
 * @param {PublicKey} newAuthority - The new authority public key
 * @returns {TransactionInstruction}
 */
function transferManagerInstruction(signer, manager, newAuthority) {
  const data = Buffer.from([EvoreInstruction.TransferManager]);

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys: [
      { pubkey: signer, isSigner: true, isWritable: true },
      { pubkey: manager, isSigner: false, isWritable: true },
      { pubkey: newAuthority, isSigner: false, isWritable: false },
    ],
    data,
  });
}

// =============================================================================
// Deploy Instructions (Manager Authority Required)
// =============================================================================

/**
 * Creates an EV Deploy instruction
 * Uses expected value calculations to determine optimal deployments
 * @param {PublicKey} signer - Manager authority
 * @param {PublicKey} manager - Manager account
 * @param {bigint} authId - Auth ID for the managed miner
 * @param {bigint} roundId - Current round ID
 * @param {bigint} bankroll - Available bankroll in lamports
 * @param {bigint} maxPerSquare - Maximum to deploy per square
 * @param {bigint} minBet - Minimum bet threshold
 * @param {bigint} oreValue - Current ORE value in lamports
 * @param {bigint} slotsLeft - Slots remaining in round
 * @param {bigint} attempts - Attempt counter (makes tx unique)
 * @param {boolean} allowMultiDeploy - Allow multiple deploys per round
 * @returns {TransactionInstruction}
 */
function evDeployInstruction(
  signer,
  manager,
  authId,
  roundId,
  bankroll,
  maxPerSquare,
  minBet,
  oreValue,
  slotsLeft,
  attempts,
  allowMultiDeploy = false
) {
  const { keys, bump } = buildDeployAccounts(signer, manager, authId, roundId);
  
  // Build instruction data
  const data = Buffer.alloc(1 + 272); // discriminator + MMDeploy size
  
  data[0] = EvoreInstruction.MMDeploy;
  data.writeBigUInt64LE(authId, 1);
  data[9] = bump;
  data[10] = allowMultiDeploy ? 1 : 0;
  
  // Strategy data starts at offset 17
  const strategyOffset = 17;
  data[strategyOffset] = 0; // EV strategy
  data.writeBigUInt64LE(bankroll, strategyOffset + 1);
  data.writeBigUInt64LE(maxPerSquare, strategyOffset + 9);
  data.writeBigUInt64LE(minBet, strategyOffset + 17);
  data.writeBigUInt64LE(oreValue, strategyOffset + 25);
  data.writeBigUInt64LE(slotsLeft, strategyOffset + 33);
  data.writeBigUInt64LE(attempts, strategyOffset + 41);

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys,
    data,
  });
}

/**
 * Creates a Percentage Deploy instruction
 * Deploys to own X% of each selected square
 * @param {PublicKey} signer - Manager authority
 * @param {PublicKey} manager - Manager account
 * @param {bigint} authId - Auth ID for the managed miner
 * @param {bigint} roundId - Current round ID
 * @param {bigint} bankroll - Available bankroll in lamports
 * @param {bigint} percentage - Target percentage in basis points (1000 = 10%)
 * @param {bigint} squaresCount - Number of squares to deploy to (1-25)
 * @param {boolean} allowMultiDeploy - Allow multiple deploys per round
 * @returns {TransactionInstruction}
 */
function percentageDeployInstruction(
  signer,
  manager,
  authId,
  roundId,
  bankroll,
  percentage,
  squaresCount,
  allowMultiDeploy = false
) {
  const { keys, bump } = buildDeployAccounts(signer, manager, authId, roundId);
  
  const data = Buffer.alloc(1 + 272);
  
  data[0] = EvoreInstruction.MMDeploy;
  data.writeBigUInt64LE(authId, 1);
  data[9] = bump;
  data[10] = allowMultiDeploy ? 1 : 0;
  
  const strategyOffset = 17;
  data[strategyOffset] = 1; // Percentage strategy
  data.writeBigUInt64LE(bankroll, strategyOffset + 1);
  data.writeBigUInt64LE(percentage, strategyOffset + 9);
  data.writeBigUInt64LE(squaresCount, strategyOffset + 17);

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys,
    data,
  });
}

/**
 * Creates a Manual Deploy instruction
 * Specify exact amounts for each of the 25 squares
 * @param {PublicKey} signer - Manager authority
 * @param {PublicKey} manager - Manager account
 * @param {bigint} authId - Auth ID for the managed miner
 * @param {bigint} roundId - Current round ID
 * @param {bigint[]} amounts - Array of 25 amounts (lamports per square, 0 to skip)
 * @param {boolean} allowMultiDeploy - Allow multiple deploys per round
 * @returns {TransactionInstruction}
 */
function manualDeployInstruction(
  signer,
  manager,
  authId,
  roundId,
  amounts,
  allowMultiDeploy = false
) {
  if (amounts.length !== 25) {
    throw new Error("amounts array must have exactly 25 elements");
  }

  const { keys, bump } = buildDeployAccounts(signer, manager, authId, roundId);
  
  const data = Buffer.alloc(1 + 272);
  
  data[0] = EvoreInstruction.MMDeploy;
  data.writeBigUInt64LE(authId, 1);
  data[9] = bump;
  data[10] = allowMultiDeploy ? 1 : 0;
  
  const strategyOffset = 17;
  data[strategyOffset] = 2; // Manual strategy
  for (let i = 0; i < 25; i++) {
    data.writeBigUInt64LE(amounts[i], strategyOffset + 1 + i * 8);
  }

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys,
    data,
  });
}

/**
 * Creates a Split Deploy instruction
 * Splits total amount equally across all 25 squares
 * @param {PublicKey} signer - Manager authority
 * @param {PublicKey} manager - Manager account
 * @param {bigint} authId - Auth ID for the managed miner
 * @param {bigint} roundId - Current round ID
 * @param {bigint} amount - Total amount to split across all squares
 * @param {boolean} allowMultiDeploy - Allow multiple deploys per round
 * @returns {TransactionInstruction}
 */
function splitDeployInstruction(
  signer,
  manager,
  authId,
  roundId,
  amount,
  allowMultiDeploy = false
) {
  const { keys, bump } = buildDeployAccounts(signer, manager, authId, roundId);
  
  const data = Buffer.alloc(1 + 272);
  
  data[0] = EvoreInstruction.MMDeploy;
  data.writeBigUInt64LE(authId, 1);
  data[9] = bump;
  data[10] = allowMultiDeploy ? 1 : 0;
  
  const strategyOffset = 17;
  data[strategyOffset] = 3; // Split strategy
  data.writeBigUInt64LE(amount, strategyOffset + 1);

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys,
    data,
  });
}

// =============================================================================
// Checkpoint & Claim Instructions (Manager Authority Required)
// =============================================================================

/**
 * Creates an MMCheckpoint instruction
 * Checkpoints the miner to claim winnings from a round
 * @param {PublicKey} signer - Manager authority
 * @param {PublicKey} manager - Manager account
 * @param {bigint} roundId - Round to checkpoint
 * @param {bigint} authId - Auth ID for the managed miner (default: 0)
 * @returns {TransactionInstruction}
 */
function mmCheckpointInstruction(signer, manager, roundId, authId = 0n) {
  const [managedMinerAuth, bump] = getManagedMinerAuthPda(manager, authId);
  const [oreMiner] = getOreMinerPda(managedMinerAuth);
  const [oreBoard] = getOreBoardPda();
  const [oreRound] = getOreRoundPda(roundId);
  
  const data = Buffer.alloc(10);
  data[0] = EvoreInstruction.MMCheckpoint;
  data.writeBigUInt64LE(authId, 1);
  data[9] = bump;

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys: [
      { pubkey: signer, isSigner: true, isWritable: true },
      { pubkey: manager, isSigner: false, isWritable: true },
      { pubkey: managedMinerAuth, isSigner: false, isWritable: true },
      { pubkey: oreMiner, isSigner: false, isWritable: true },
      { pubkey: ORE_TREASURY_ADDRESS, isSigner: false, isWritable: true },
      { pubkey: oreBoard, isSigner: false, isWritable: true },
      { pubkey: oreRound, isSigner: false, isWritable: true },
      { pubkey: SYSTEM_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: ORE_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data,
  });
}

/**
 * Creates an MMClaimSOL instruction
 * Claims SOL rewards from the miner to the manager authority
 * @param {PublicKey} signer - Manager authority
 * @param {PublicKey} manager - Manager account
 * @param {bigint} authId - Auth ID for the managed miner (default: 0)
 * @returns {TransactionInstruction}
 */
function mmClaimSolInstruction(signer, manager, authId = 0n) {
  const [managedMinerAuth, bump] = getManagedMinerAuthPda(manager, authId);
  const [oreMiner] = getOreMinerPda(managedMinerAuth);
  
  const data = Buffer.alloc(10);
  data[0] = EvoreInstruction.MMClaimSOL;
  data.writeBigUInt64LE(authId, 1);
  data[9] = bump;

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys: [
      { pubkey: signer, isSigner: true, isWritable: true },
      { pubkey: manager, isSigner: false, isWritable: true },
      { pubkey: managedMinerAuth, isSigner: false, isWritable: true },
      { pubkey: oreMiner, isSigner: false, isWritable: true },
      { pubkey: SYSTEM_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: ORE_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data,
  });
}

/**
 * Creates an MMClaimORE instruction
 * Claims ORE token rewards from the miner to the signer
 * @param {PublicKey} signer - Manager authority
 * @param {PublicKey} manager - Manager account
 * @param {bigint} authId - Auth ID for the managed miner (default: 0)
 * @returns {TransactionInstruction}
 */
function mmClaimOreInstruction(signer, manager, authId = 0n) {
  const [managedMinerAuth, bump] = getManagedMinerAuthPda(manager, authId);
  const [oreMiner] = getOreMinerPda(managedMinerAuth);
  const [treasury] = getOreTreasuryPda();
  const treasuryTokens = getOreTokenAddress(treasury);
  const recipientTokens = getOreTokenAddress(managedMinerAuth);
  const signerTokens = getOreTokenAddress(signer);
  
  const data = Buffer.alloc(10);
  data[0] = EvoreInstruction.MMClaimORE;
  data.writeBigUInt64LE(authId, 1);
  data[9] = bump;

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys: [
      { pubkey: signer, isSigner: true, isWritable: true },
      { pubkey: manager, isSigner: false, isWritable: true },
      { pubkey: managedMinerAuth, isSigner: false, isWritable: true },
      { pubkey: oreMiner, isSigner: false, isWritable: true },
      { pubkey: ORE_MINT_ADDRESS, isSigner: false, isWritable: true },
      { pubkey: recipientTokens, isSigner: false, isWritable: true },
      { pubkey: signerTokens, isSigner: false, isWritable: true },
      { pubkey: treasury, isSigner: false, isWritable: true },
      { pubkey: treasuryTokens, isSigner: false, isWritable: true },
      { pubkey: SYSTEM_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: ASSOCIATED_TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: ORE_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data,
  });
}

// =============================================================================
// Deployer Instructions (Manager Authority Required)
// =============================================================================

/**
 * Creates a CreateDeployer instruction
 * Creates a new deployer account linked to the manager
 * @param {PublicKey} signer - Manager authority
 * @param {PublicKey} manager - Manager account
 * @param {PublicKey} deployAuthority - The authority that will execute autodeploys
 * @param {bigint} bpsFee - Max bps fee the user accepts (deployer can charge up to this)
 * @param {bigint} flatFee - Max flat fee in lamports the user accepts (deployer can charge up to this)
 * @param {bigint} maxPerRound - Maximum lamports to deploy per round (0 = unlimited)
 * @returns {TransactionInstruction}
 */
function createDeployerInstruction(signer, manager, deployAuthority, bpsFee, flatFee = 0n, maxPerRound = 1_000_000_000n) {
  const [deployerPda] = getDeployerPda(manager);
  
  const data = Buffer.alloc(25);
  data[0] = EvoreInstruction.CreateDeployer;
  data.writeBigUInt64LE(bpsFee, 1);
  data.writeBigUInt64LE(flatFee, 9);
  data.writeBigUInt64LE(maxPerRound, 17);

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys: [
      { pubkey: signer, isSigner: true, isWritable: true },
      { pubkey: manager, isSigner: false, isWritable: true },
      { pubkey: deployerPda, isSigner: false, isWritable: true },
      { pubkey: deployAuthority, isSigner: false, isWritable: false },
      { pubkey: SYSTEM_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data,
  });
}

/**
 * Creates an UpdateDeployer instruction
 * - Manager authority: can update deploy_authority, expected_bps_fee, expected_flat_fee, max_per_round
 * - Deploy authority: can update deploy_authority, bps_fee, flat_fee
 * @param {PublicKey} signer - Manager authority or deploy authority
 * @param {PublicKey} manager - Manager account
 * @param {PublicKey} newDeployAuthority - New deploy authority
 * @param {bigint} newBpsFee - Actual bps fee charged (deploy authority only, must be <= expected)
 * @param {bigint} newFlatFee - Actual flat fee charged (deploy authority only, must be <= expected)
 * @param {bigint} newExpectedBpsFee - Max bps fee user accepts (manager only, 0 = accept any)
 * @param {bigint} newExpectedFlatFee - Max flat fee user accepts (manager only, 0 = accept any)
 * @param {bigint} newMaxPerRound - Maximum lamports to deploy per round (manager only, 0 = unlimited)
 * @returns {TransactionInstruction}
 */
function updateDeployerInstruction(
  signer,
  manager,
  newDeployAuthority,
  newBpsFee,
  newFlatFee = 0n,
  newExpectedBpsFee = 0n,
  newExpectedFlatFee = 0n,
  newMaxPerRound = 1_000_000_000n
) {
  const [deployerPda] = getDeployerPda(manager);

  const data = Buffer.alloc(41);
  data[0] = EvoreInstruction.UpdateDeployer;
  data.writeBigUInt64LE(newBpsFee, 1);
  data.writeBigUInt64LE(newFlatFee, 9);
  data.writeBigUInt64LE(newExpectedBpsFee, 17);
  data.writeBigUInt64LE(newExpectedFlatFee, 25);
  data.writeBigUInt64LE(newMaxPerRound, 33);

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys: [
      { pubkey: signer, isSigner: true, isWritable: true },
      { pubkey: manager, isSigner: false, isWritable: true },
      { pubkey: deployerPda, isSigner: false, isWritable: true },
      { pubkey: newDeployAuthority, isSigner: false, isWritable: false },
      { pubkey: SYSTEM_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data,
  });
}

// =============================================================================
// Autodeploy Balance Instructions (Manager Authority Required)
// =============================================================================

/**
 * Creates a DepositAutodeployBalance instruction
 * Deposits SOL into the managed_miner_auth PDA for a specific miner
 * @param {PublicKey} signer - Manager authority
 * @param {PublicKey} manager - Manager account
 * @param {bigint} authId - Auth ID of the managed miner
 * @param {bigint} amount - Amount to deposit in lamports
 * @returns {TransactionInstruction}
 */
function depositAutodeployBalanceInstruction(signer, manager, authId, amount) {
  const [managedMinerAuth] = getManagedMinerAuthPda(manager, authId);
  
  const data = Buffer.alloc(17);
  data[0] = EvoreInstruction.DepositAutodeployBalance;
  data.writeBigUInt64LE(authId, 1);
  data.writeBigUInt64LE(amount, 9);

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys: [
      { pubkey: signer, isSigner: true, isWritable: true },
      { pubkey: manager, isSigner: false, isWritable: true },
      { pubkey: managedMinerAuth, isSigner: false, isWritable: true },
      { pubkey: SYSTEM_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data,
  });
}

/**
 * Creates a WithdrawAutodeployBalance instruction
 * Withdraws SOL from the managed_miner_auth PDA
 * @param {PublicKey} signer - Manager authority
 * @param {PublicKey} manager - Manager account
 * @param {bigint} authId - Auth ID of the managed miner
 * @param {bigint} amount - Amount to withdraw in lamports
 * @returns {TransactionInstruction}
 */
function withdrawAutodeployBalanceInstruction(signer, manager, authId, amount) {
  const [managedMinerAuth] = getManagedMinerAuthPda(manager, authId);
  
  const data = Buffer.alloc(17);
  data[0] = EvoreInstruction.WithdrawAutodeployBalance;
  data.writeBigUInt64LE(authId, 1);
  data.writeBigUInt64LE(amount, 9);

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys: [
      { pubkey: signer, isSigner: true, isWritable: true },
      { pubkey: manager, isSigner: false, isWritable: true },
      { pubkey: managedMinerAuth, isSigner: false, isWritable: true },
      { pubkey: SYSTEM_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data,
  });
}

// =============================================================================
// Autodeploy Instructions (Deploy Authority Required - for Executors/Cranks)
// =============================================================================

/**
 * Creates an MMAutodeploy instruction
 * Deploys from managed_miner_auth balance (deploy_authority signs, NOT manager authority)
 * Fees are read from the Deployer account and validated against expected_fees stored there
 * @param {PublicKey} signer - Deploy authority (executor)
 * @param {PublicKey} manager - Manager account
 * @param {bigint} authId - Auth ID for the managed miner
 * @param {bigint} roundId - Current round ID
 * @param {bigint} amount - Amount to deploy per selected square
 * @param {number} squaresMask - Bitmask of squares to deploy to (bits 0-24)
 * @returns {TransactionInstruction}
 */
function mmAutodeployInstruction(
  signer,
  manager,
  authId,
  roundId,
  amount,
  squaresMask
) {
  const [managedMinerAuth] = getManagedMinerAuthPda(manager, authId);
  const [deployerPda] = getDeployerPda(manager);
  const [oreMiner] = getOreMinerPda(managedMinerAuth);
  const [oreBoard] = getOreBoardPda();
  const [oreConfig] = getOreConfigPda();
  const [oreRound] = getOreRoundPda(roundId);
  const [oreAutomation] = getOreAutomationPda(managedMinerAuth);
  const [entropyVar] = getEntropyVarPda(oreBoard, 0n);

  // Build instruction data (25 bytes total: 1 discriminator + 8 auth_id + 8 amount + 4 squares_mask + 4 pad)
  const data = Buffer.alloc(25);
  
  data[0] = EvoreInstruction.MMAutodeploy;
  data.writeBigUInt64LE(authId, 1);           // auth_id: [u8; 8]
  data.writeBigUInt64LE(amount, 9);           // amount: [u8; 8]
  data.writeUInt32LE(squaresMask, 17);        // squares_mask: [u8; 4]
  // _pad: [u8; 4] at bytes 21-24 (already zeros)

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys: [
      { pubkey: signer, isSigner: true, isWritable: true },           // 0: deploy_authority
      { pubkey: manager, isSigner: false, isWritable: true },         // 1: manager
      { pubkey: deployerPda, isSigner: false, isWritable: true },     // 2: deployer PDA
      { pubkey: managedMinerAuth, isSigner: false, isWritable: true }, // 3: managed_miner_auth (funds source)
      { pubkey: oreMiner, isSigner: false, isWritable: true },        // 4: ore_miner
      { pubkey: FEE_COLLECTOR, isSigner: false, isWritable: true },   // 5: fee_collector
      { pubkey: oreAutomation, isSigner: false, isWritable: true },   // 6: automation
      { pubkey: oreConfig, isSigner: false, isWritable: true },       // 7: config
      { pubkey: oreBoard, isSigner: false, isWritable: true },        // 8: board
      { pubkey: oreRound, isSigner: false, isWritable: true },        // 9: round
      { pubkey: entropyVar, isSigner: false, isWritable: true },      // 10: entropy_var
      { pubkey: ORE_PROGRAM_ID, isSigner: false, isWritable: false }, // 11: ore_program
      { pubkey: ENTROPY_PROGRAM_ID, isSigner: false, isWritable: false }, // 12: entropy_program
      { pubkey: SYSTEM_PROGRAM_ID, isSigner: false, isWritable: false }, // 13: system_program
    ],
    data,
  });
}

/**
 * Creates an MMAutocheckpoint instruction
 * Checkpoint callable by deploy_authority (for executors/cranks)
 * @param {PublicKey} signer - Deploy authority (executor)
 * @param {PublicKey} manager - Manager account
 * @param {bigint} roundId - Round to checkpoint
 * @param {bigint} authId - Auth ID for the managed miner
 * @returns {TransactionInstruction}
 */
function mmAutocheckpointInstruction(signer, manager, roundId, authId) {
  const [deployerPda] = getDeployerPda(manager);
  const [managedMinerAuth, bump] = getManagedMinerAuthPda(manager, authId);
  const [oreMiner] = getOreMinerPda(managedMinerAuth);
  const [oreBoard] = getOreBoardPda();
  const [oreRound] = getOreRoundPda(roundId);

  const data = Buffer.alloc(10);
  data[0] = EvoreInstruction.MMAutocheckpoint;
  data.writeBigUInt64LE(authId, 1);
  data[9] = bump;

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys: [
      { pubkey: signer, isSigner: true, isWritable: true },           // 0: deploy_authority
      { pubkey: manager, isSigner: false, isWritable: true },         // 1: manager
      { pubkey: deployerPda, isSigner: false, isWritable: true },     // 2: deployer PDA
      { pubkey: managedMinerAuth, isSigner: false, isWritable: true }, // 3: managed_miner_auth
      { pubkey: oreMiner, isSigner: false, isWritable: true },        // 4: ore_miner
      { pubkey: ORE_TREASURY_ADDRESS, isSigner: false, isWritable: true }, // 5: treasury
      { pubkey: oreBoard, isSigner: false, isWritable: true },        // 6: board
      { pubkey: oreRound, isSigner: false, isWritable: true },        // 7: round
      { pubkey: SYSTEM_PROGRAM_ID, isSigner: false, isWritable: false }, // 8: system_program
      { pubkey: ORE_PROGRAM_ID, isSigner: false, isWritable: false }, // 9: ore_program
    ],
    data,
  });
}

/**
 * Creates a RecycleSol instruction
 * Claims SOL rewards from miner account (stays in managed_miner_auth)
 * @param {PublicKey} signer - Deploy authority (executor)
 * @param {PublicKey} manager - Manager account
 * @param {bigint} authId - Auth ID for the managed miner
 * @returns {TransactionInstruction}
 */
function recycleSolInstruction(signer, manager, authId) {
  const [managedMinerAuth] = getManagedMinerAuthPda(manager, authId);
  const [oreMiner] = getOreMinerPda(managedMinerAuth);
  const [deployerPda] = getDeployerPda(manager);

  const data = Buffer.alloc(9);
  data[0] = EvoreInstruction.RecycleSol;
  data.writeBigUInt64LE(authId, 1);

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys: [
      { pubkey: signer, isSigner: true, isWritable: true },           // 0: deploy_authority
      { pubkey: manager, isSigner: false, isWritable: true },         // 1: manager
      { pubkey: deployerPda, isSigner: false, isWritable: true },     // 2: deployer PDA
      { pubkey: managedMinerAuth, isSigner: false, isWritable: true }, // 3: managed_miner_auth
      { pubkey: oreMiner, isSigner: false, isWritable: true },        // 4: ore_miner
      { pubkey: ORE_PROGRAM_ID, isSigner: false, isWritable: false }, // 5: ore_program
    ],
    data,
  });
}

/**
 * Creates an MMFullAutodeploy instruction
 * Combined checkpoint + recycle + deploy in one instruction
 * @param {PublicKey} signer - Deploy authority (executor)
 * @param {PublicKey} manager - Manager account
 * @param {bigint} authId - Auth ID for the managed miner
 * @param {bigint} roundId - Current round ID for deploying
 * @param {bigint} checkpointRoundId - Round ID that needs checkpointing (usually roundId - 1, or same as roundId if no checkpoint needed)
 * @param {bigint} amount - Amount to deploy per selected square
 * @param {number} squaresMask - Bitmask of squares to deploy to (bits 0-24)
 * @returns {TransactionInstruction}
 */
function mmFullAutodeployInstruction(
  signer,
  manager,
  authId,
  roundId,
  checkpointRoundId,
  amount,
  squaresMask
) {
  const [managedMinerAuth] = getManagedMinerAuthPda(manager, authId);
  const [deployerPda] = getDeployerPda(manager);
  const [oreMiner] = getOreMinerPda(managedMinerAuth);
  const [oreBoard] = getOreBoardPda();
  const [oreConfig] = getOreConfigPda();
  const [oreRound] = getOreRoundPda(roundId);
  const [checkpointRound] = getOreRoundPda(checkpointRoundId);
  const [oreAutomation] = getOreAutomationPda(managedMinerAuth);
  const [entropyVar] = getEntropyVarPda(oreBoard, 0n);

  // Build instruction data (25 bytes total: 1 discriminator + 8 auth_id + 8 amount + 4 squares_mask + 4 pad)
  const data = Buffer.alloc(25);
  
  data[0] = EvoreInstruction.MMFullAutodeploy;
  data.writeBigUInt64LE(authId, 1);           // auth_id: [u8; 8]
  data.writeBigUInt64LE(amount, 9);           // amount: [u8; 8]
  data.writeUInt32LE(squaresMask, 17);        // squares_mask: [u8; 4]
  // _pad: [u8; 4] at bytes 21-24 (already zeros)

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys: [
      { pubkey: signer, isSigner: true, isWritable: true },           // 0: deploy_authority
      { pubkey: manager, isSigner: false, isWritable: true },         // 1: manager
      { pubkey: deployerPda, isSigner: false, isWritable: true },     // 2: deployer PDA
      { pubkey: managedMinerAuth, isSigner: false, isWritable: true }, // 3: managed_miner_auth (funds source)
      { pubkey: oreMiner, isSigner: false, isWritable: true },        // 4: ore_miner
      { pubkey: FEE_COLLECTOR, isSigner: false, isWritable: true },   // 5: fee_collector
      { pubkey: oreAutomation, isSigner: false, isWritable: true },   // 6: automation
      { pubkey: oreConfig, isSigner: false, isWritable: true },       // 7: config
      { pubkey: oreBoard, isSigner: false, isWritable: true },        // 8: board
      { pubkey: oreRound, isSigner: false, isWritable: true },        // 9: round (current round for deploy)
      { pubkey: checkpointRound, isSigner: false, isWritable: true }, // 10: checkpoint_round (for checkpoint CPI)
      { pubkey: ORE_TREASURY_ADDRESS, isSigner: false, isWritable: true }, // 11: treasury (for checkpoint)
      { pubkey: entropyVar, isSigner: false, isWritable: true },      // 12: entropy_var
      { pubkey: ORE_PROGRAM_ID, isSigner: false, isWritable: false }, // 13: ore_program
      { pubkey: ENTROPY_PROGRAM_ID, isSigner: false, isWritable: false }, // 14: entropy_program
      { pubkey: SYSTEM_PROGRAM_ID, isSigner: false, isWritable: false }, // 15: system_program
    ],
    data,
  });
}

// =============================================================================
// Helper Functions
// =============================================================================

/**
 * Build deploy accounts for MMDeploy instruction
 * @private
 */
function buildDeployAccounts(signer, manager, authId, roundId) {
  const [managedMinerAuth, bump] = getManagedMinerAuthPda(manager, authId);
  const [oreMiner] = getOreMinerPda(managedMinerAuth);
  const [oreBoard] = getOreBoardPda();
  const [oreConfig] = getOreConfigPda();
  const [oreRound] = getOreRoundPda(roundId);
  const [oreAutomation] = getOreAutomationPda(managedMinerAuth);
  const [entropyVar] = getEntropyVarPda(oreBoard, 0n);

  const keys = [
    { pubkey: signer, isSigner: true, isWritable: true },
    { pubkey: manager, isSigner: false, isWritable: true },
    { pubkey: managedMinerAuth, isSigner: false, isWritable: true },
    { pubkey: oreMiner, isSigner: false, isWritable: true },
    { pubkey: FEE_COLLECTOR, isSigner: false, isWritable: true },
    { pubkey: oreAutomation, isSigner: false, isWritable: true },
    { pubkey: oreConfig, isSigner: false, isWritable: true },
    { pubkey: oreBoard, isSigner: false, isWritable: true },
    { pubkey: oreRound, isSigner: false, isWritable: true },
    { pubkey: entropyVar, isSigner: false, isWritable: true },
    { pubkey: ORE_PROGRAM_ID, isSigner: false, isWritable: false },
    { pubkey: ENTROPY_PROGRAM_ID, isSigner: false, isWritable: false },
    { pubkey: SYSTEM_PROGRAM_ID, isSigner: false, isWritable: false },
  ];

  return { keys, bump };
}

/**
 * Converts an array of 25 booleans to a squares bitmask
 * @param {boolean[]} squares - Array of 25 booleans (true = deploy to square)
 * @returns {number} - 32-bit bitmask
 */
function squaresToMask(squares) {
  if (squares.length !== 25) {
    throw new Error("squares array must have exactly 25 elements");
  }
  let mask = 0;
  for (let i = 0; i < 25; i++) {
    if (squares[i]) {
      mask |= (1 << i);
    }
  }
  return mask;
}

/**
 * Converts a squares bitmask to an array of 25 booleans
 * @param {number} mask - 32-bit bitmask
 * @returns {boolean[]} - Array of 25 booleans
 */
function maskToSquares(mask) {
  const squares = [];
  for (let i = 0; i < 25; i++) {
    squares.push((mask & (1 << i)) !== 0);
  }
  return squares;
}

// =============================================================================
// MMCreateMiner Instruction (Manager Authority Required)
// =============================================================================

/**
 * Creates an MMCreateMiner instruction
 * Creates an ORE miner account by CPIing to automate twice (open then close)
 * @param {PublicKey} signer - Manager authority
 * @param {PublicKey} manager - Manager account
 * @param {bigint} authId - Auth ID for the managed miner (default: 0)
 * @returns {TransactionInstruction}
 */
function mmCreateMinerInstruction(signer, manager, authId = 0n) {
  const [managedMinerAuth, bump] = getManagedMinerAuthPda(manager, authId);
  const [oreAutomation] = getOreAutomationPda(managedMinerAuth);
  const [oreMiner] = getOreMinerPda(managedMinerAuth);

  const data = Buffer.alloc(10);
  data[0] = EvoreInstruction.MMCreateMiner;
  data.writeBigUInt64LE(authId, 1);
  data[9] = bump;

  // executor_1 = signer (for first automate CPI - open)
  // executor_2 = Pubkey::default() (for second automate CPI - close)
  // Note: executor_2 is readonly to avoid privilege conflicts with system_program
  // (they're the same pubkey). ORE doesn't actually check executor is writable.
  const executor1 = signer;
  const executor2 = PublicKey.default;

  return new TransactionInstruction({
    programId: EVORE_PROGRAM_ID,
    keys: [
      { pubkey: signer, isSigner: true, isWritable: true },
      { pubkey: manager, isSigner: false, isWritable: true },
      { pubkey: managedMinerAuth, isSigner: false, isWritable: true },
      { pubkey: oreAutomation, isSigner: false, isWritable: true },
      { pubkey: oreMiner, isSigner: false, isWritable: true },
      { pubkey: executor1, isSigner: false, isWritable: true },
      { pubkey: executor2, isSigner: false, isWritable: false }, // readonly to match system_program
      { pubkey: SYSTEM_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: ORE_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data,
  });
}

module.exports = {
  // Manager
  createManagerInstruction,
  transferManagerInstruction,

  // Deploy (manager authority)
  evDeployInstruction,
  percentageDeployInstruction,
  manualDeployInstruction,
  splitDeployInstruction,

  // Checkpoint & Claim (manager authority)
  mmCheckpointInstruction,
  mmClaimSolInstruction,
  mmClaimOreInstruction,

  // Deployer (manager authority)
  createDeployerInstruction,
  updateDeployerInstruction,

  // Autodeploy Balance (manager authority)
  depositAutodeployBalanceInstruction,
  withdrawAutodeployBalanceInstruction,

  // Autodeploy (deploy authority - for executors)
  mmAutodeployInstruction,
  mmAutocheckpointInstruction,
  recycleSolInstruction,
  mmFullAutodeployInstruction,

  // Miner Creation (manager authority)
  mmCreateMinerInstruction,

  // Helpers
  squaresToMask,
  maskToSquares,
};
