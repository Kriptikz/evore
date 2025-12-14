#!/usr/bin/env node
/**
 * Evore Autodeploy Crank (JavaScript)
 * 
 * Reference implementation for automated deploying via the Evore program.
 * Built with @solana/web3.js v1.x
 * 
 * Users should customize the DEPLOYMENT STRATEGY constants based on their
 * specific requirements.
 */

require('dotenv').config();
const { program } = require('commander');
const fs = require('fs');
const {
  Connection,
  Keypair,
  PublicKey,
  Transaction,
  VersionedTransaction,
  TransactionMessage,
  ComputeBudgetProgram,
  SystemProgram,
  AddressLookupTableProgram,
  AddressLookupTableAccount,
} = require('@solana/web3.js');
const {
  EVORE_PROGRAM_ID,
  ORE_PROGRAM_ID,
  ENTROPY_PROGRAM_ID,
  FEE_COLLECTOR,
  SYSTEM_PROGRAM_ID,
  DEPLOYER_DISCRIMINATOR,
  DEPLOY_FEE,
  ORE_CHECKPOINT_FEE,
  getDeployerPda,
  getAutodeployBalancePda,
  getManagedMinerAuthPda,
  getOreMinerPda,
  getOreBoardPda,
  getOreRoundPda,
  getOreConfigPda,
  getOreAutomationPda,
  decodeDeployer,
  decodeOreBoard,
  decodeOreMiner,
  mmAutodeployInstruction,
  mmAutocheckpointInstruction,
  recycleSolInstruction,
  formatSol,
  formatFee,
} = require('evore-sdk');

// =============================================================================
// DEPLOYMENT STRATEGY - Customize these for your use case
// =============================================================================

/** Amount to deploy per square in lamports (0.00001 SOL = 10,000 lamports) */
const DEPLOY_AMOUNT_LAMPORTS = 10_000n;

/** Which auth_id to deploy for (each manager can have multiple managed miners) */
const AUTH_ID = 0n;

/** Squares mask - which squares to deploy to (0x1FFFFFF = all 25 squares) */
const SQUARES_MASK = 0x1FFFFFF;

/** How many slots before round end to trigger deployment */
const DEPLOY_SLOTS_BEFORE_END = 150n;

/** Minimum slots remaining to attempt deployment (don't deploy too close to end) */
const MIN_SLOTS_TO_DEPLOY = 10n;

/** Maximum deployers to batch in one transaction without LUT */
const MAX_BATCH_SIZE_NO_LUT = 2;

/** Maximum deployers to batch in one transaction with LUT */
const MAX_BATCH_SIZE_WITH_LUT = 5;

// =============================================================================
// RENT CONSTANTS
// =============================================================================
const AUTH_PDA_RENT = 890_880n;
const AUTODEPLOY_BALANCE_RENT = 890_880n;
const MINER_RENT_ESTIMATE = 2_500_000n;

// =============================================================================
// LUT MANAGER
// =============================================================================

class LutManager {
  constructor(connection, authority) {
    this.connection = connection;
    this.authority = authority;
    this.lutAddress = null;
    this.cachedAddresses = new Set();
    this.lutAccount = null;
  }

  /**
   * Get shared accounts that are always included in the LUT
   */
  static getSharedAccounts(roundId) {
    const [boardAddress] = getOreBoardPda();
    const [roundAddress] = getOreRoundPda(roundId);
    const [configAddress] = getOreConfigPda();

    return [
      SYSTEM_PROGRAM_ID,
      ORE_PROGRAM_ID,
      ENTROPY_PROGRAM_ID,
      FEE_COLLECTOR,
      boardAddress,
      roundAddress,
      configAddress,
      EVORE_PROGRAM_ID,
    ];
  }

  /**
   * Get all accounts needed for a single deployer
   */
  static getDeployerAccounts(manager, authId) {
    const [deployerAddr] = getDeployerPda(manager);
    const [autodeployBalanceAddr] = getAutodeployBalancePda(deployerAddr);
    const [managedMinerAuth] = getManagedMinerAuthPda(manager, authId);
    const [oreMiner] = getOreMinerPda(managedMinerAuth);
    const [automation] = getOreAutomationPda(oreMiner);

    return [
      manager,
      deployerAddr,
      autodeployBalanceAddr,
      managedMinerAuth,
      oreMiner,
      automation,
    ];
  }

  /**
   * Load an existing LUT from address
   */
  async loadLut(lutAddress) {
    this.lutAddress = lutAddress;

    try {
      const accountInfo = await this.connection.getAccountInfo(lutAddress);
      if (!accountInfo) {
        throw new Error('LUT account not found');
      }

      // Deserialize the lookup table state
      const state = AddressLookupTableAccount.deserialize(accountInfo.data);

      const lutAccount = {
        key: lutAddress,
        state: state,
      };

      // Cache the addresses
      this.cachedAddresses.clear();
      for (const addr of state.addresses) {
        this.cachedAddresses.add(addr.toBase58());
      }

      this.lutAccount = lutAccount;
      console.log(`Loaded LUT ${lutAddress.toBase58()} with ${state.addresses.length} addresses`);

      return lutAccount;
    } catch (err) {
      throw new Error(`Failed to load LUT: ${err.message}`);
    }
  }

  /**
   * Get the current LUT account
   */
  getLutAccount() {
    return this.lutAccount;
  }

  /**
   * Create a new LUT
   */
  async createLut(keypair) {
    const slot = await this.connection.getSlot();

    const [createIx, lutAddress] = AddressLookupTableProgram.createLookupTable({
      authority: this.authority,
      payer: this.authority,
      recentSlot: slot,
    });

    const tx = new Transaction().add(createIx);
    tx.feePayer = this.authority;
    tx.recentBlockhash = (await this.connection.getLatestBlockhash()).blockhash;

    tx.sign(keypair);
    const signature = await this.connection.sendRawTransaction(tx.serialize());
    
    // Wait for confirmation
    await this.connection.confirmTransaction(signature);

    this.lutAddress = lutAddress;
    console.log(`Created LUT: ${lutAddress.toBase58()}`);
    console.log(`Transaction: ${signature}`);

    return lutAddress;
  }

  /**
   * Extend LUT with new addresses
   */
  async extendLut(keypair, newAddresses) {
    if (!this.lutAddress) {
      throw new Error('No LUT address set');
    }

    if (newAddresses.length === 0) {
      console.log('No new addresses to add');
      return null;
    }

    // LUT extension has max ~20 addresses per tx
    const chunks = [];
    for (let i = 0; i < newAddresses.length; i += 20) {
      chunks.push(newAddresses.slice(i, i + 20));
    }

    const signatures = [];
    for (const chunk of chunks) {
      const extendIx = AddressLookupTableProgram.extendLookupTable({
        lookupTable: this.lutAddress,
        authority: this.authority,
        payer: this.authority,
        addresses: chunk,
      });

      const tx = new Transaction().add(extendIx);
      tx.feePayer = this.authority;
      tx.recentBlockhash = (await this.connection.getLatestBlockhash()).blockhash;

      tx.sign(keypair);
      const signature = await this.connection.sendRawTransaction(tx.serialize());
      signatures.push(signature);

      // Add to cache
      for (const addr of chunk) {
        this.cachedAddresses.add(addr.toBase58());
      }

      console.log(`Extended LUT with ${chunk.length} addresses: ${signature}`);
      
      // Wait for confirmation
      await this.connection.confirmTransaction(signature);
    }

    return signatures;
  }

  /**
   * Get addresses that are missing from the LUT
   */
  getMissingAddresses(addresses) {
    return addresses.filter(addr => !this.cachedAddresses.has(addr.toBase58()));
  }

  /**
   * Get missing deployer addresses for a list of deployers
   */
  getMissingDeployerAddresses(deployers, authId, roundId) {
    const needed = [];
    const seen = new Set();

    // Add shared accounts
    const shared = LutManager.getSharedAccounts(roundId);
    for (const addr of shared) {
      const key = addr.toBase58();
      if (!this.cachedAddresses.has(key) && !seen.has(key)) {
        needed.push(addr);
        seen.add(key);
      }
    }

    // Add deployer-specific accounts
    for (const deployer of deployers) {
      const accounts = LutManager.getDeployerAccounts(deployer.managerAddress, authId);
      for (const addr of accounts) {
        const key = addr.toBase58();
        if (!this.cachedAddresses.has(key) && !seen.has(key)) {
          needed.push(addr);
          seen.add(key);
        }
      }
    }

    return needed;
  }

  /**
   * Get the LUT for use in VersionedTransaction
   */
  getAddressLookupTableAccount() {
    if (!this.lutAccount) return null;
    
    return {
      key: this.lutAddress,
      state: this.lutAccount.state,
    };
  }
}

// =============================================================================
// CRANK CLASS
// =============================================================================

class Crank {
  constructor(connection, keypair, pubkey, config) {
    this.connection = connection;
    this.keypair = keypair;
    this.pubkey = pubkey;
    this.config = config;
    this.lutManager = null;
  }

  /**
   * Initialize LUT manager and optionally load existing LUT
   */
  async initLut(lutAddress = null) {
    this.lutManager = new LutManager(this.connection, this.pubkey);
    
    if (lutAddress) {
      await this.lutManager.loadLut(lutAddress);
    }
    
    return this.lutManager;
  }

  /**
   * Find all deployer accounts where we are the deploy_authority
   */
  async findDeployers() {
    console.log(`Scanning for deployers with deploy_authority: ${this.pubkey.toBase58()}`);

    const accounts = await this.connection.getProgramAccounts(EVORE_PROGRAM_ID, {
      filters: [
        {
          memcmp: {
            offset: 0,
            bytes: Buffer.from([DEPLOYER_DISCRIMINATOR, 0, 0, 0, 0, 0, 0, 0]).toString('base64'),
            encoding: 'base64',
          },
        },
      ],
    });

    const deployers = [];
    for (const { pubkey: deployerAddress, account } of accounts) {
      try {
        const deployer = decodeDeployer(account.data);
        
        // Check if we are the deploy authority
        if (deployer.deployAuthority.toBase58() !== this.pubkey.toBase58()) {
          continue;
        }

        const [autodeployBalanceAddress] = getAutodeployBalancePda(deployerAddress);

        deployers.push({
          deployerAddress,
          managerAddress: deployer.managerKey,
          autodeployBalanceAddress,
          bpsFee: deployer.bpsFee,
          flatFee: deployer.flatFee,
        });

        console.log(`  Found: ${deployerAddress.toBase58()} for manager: ${deployer.managerKey.toBase58()} (fee: ${formatFee(deployer.bpsFee, deployer.flatFee)})`);
      } catch (err) {
        console.warn(`  Warning: Failed to decode deployer ${deployerAddress.toBase58()}: ${err.message}`);
      }
    }

    console.log(`Found ${deployers.length} deployers`);
    return deployers;
  }

  /**
   * Get current ORE board state
   */
  async getBoard() {
    const [boardAddress] = getOreBoardPda();
    const accountInfo = await this.connection.getAccountInfo(boardAddress);
    
    if (!accountInfo) {
      throw new Error('Board account not found');
    }

    const board = decodeOreBoard(accountInfo.data);
    const currentSlot = BigInt(await this.connection.getSlot());

    return { ...board, currentSlot };
  }

  /**
   * Get autodeploy balance for a deployer
   */
  async getAutodeployBalance(deployer) {
    const balance = await this.connection.getBalance(deployer.autodeployBalanceAddress);
    return BigInt(balance);
  }

  /**
   * Get miner checkpoint status
   */
  async getMinerStatus(managerAddress, authId) {
    const [managedMinerAuth] = getManagedMinerAuthPda(managerAddress, authId);
    const [oreMinerAddress] = getOreMinerPda(managedMinerAuth);

    try {
      const accountInfo = await this.connection.getAccountInfo(oreMinerAddress);
      if (!accountInfo) return null;

      const miner = decodeOreMiner(accountInfo.data);
      return {
        checkpointId: miner.checkpointId,
        roundId: miner.roundId,
        rewardsSol: miner.rewardsSol,
      };
    } catch {
      return null;
    }
  }

  /**
   * Check if deployer needs checkpoint
   */
  async needsCheckpoint(deployer, authId) {
    const status = await this.getMinerStatus(deployer.managerAddress, authId);
    if (!status) return null;
    
    if (status.checkpointId < status.roundId) {
      return status.roundId;
    }
    return null;
  }

  /**
   * Calculate required balance for a deploy
   */
  async calculateRequiredBalance(deployer, authId, amountPerSquare, squaresMask) {
    const numSquares = BigInt(countBits(squaresMask));
    const totalDeployed = amountPerSquare * numSquares;

    const bpsFeeAmount = (totalDeployed * deployer.bpsFee) / 10000n;
    const deployerFee = bpsFeeAmount + deployer.flatFee;
    const protocolFee = DEPLOY_FEE;

    const [managedMinerAuth] = getManagedMinerAuthPda(deployer.managerAddress, authId);
    let currentAuthBalance = 0n;
    try {
      currentAuthBalance = BigInt(await this.connection.getBalance(managedMinerAuth));
    } catch {}

    const [oreMinerAddress] = getOreMinerPda(managedMinerAuth);
    let minerRent = 0n;
    try {
      const acct = await this.connection.getAccountInfo(oreMinerAddress);
      if (!acct) minerRent = MINER_RENT_ESTIMATE;
    } catch {
      minerRent = MINER_RENT_ESTIMATE;
    }

    const requiredMinerBalance = AUTH_PDA_RENT + ORE_CHECKPOINT_FEE + totalDeployed + minerRent;
    const transferToMiner = requiredMinerBalance > currentAuthBalance 
      ? requiredMinerBalance - currentAuthBalance 
      : 0n;

    const totalNeeded = transferToMiner + deployerFee + protocolFee + AUTODEPLOY_BALANCE_RENT;
    return totalNeeded;
  }

  /**
   * Send a test transaction
   */
  async sendTestTransaction() {
    console.log(`Sending test transaction from ${this.pubkey.toBase58()}`);

    const tx = new Transaction()
      .add(ComputeBudgetProgram.setComputeUnitLimit({ units: 5000 }))
      .add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: this.config.priorityFee }))
      .add(SystemProgram.transfer({
        fromPubkey: this.pubkey,
        toPubkey: this.pubkey,
        lamports: 0,
      }));

    tx.feePayer = this.pubkey;
    tx.recentBlockhash = (await this.connection.getLatestBlockhash()).blockhash;

    tx.sign(this.keypair);
    const signature = await this.connection.sendRawTransaction(tx.serialize());
    
    // Wait for confirmation
    await this.connection.confirmTransaction(signature);
    
    return signature;
  }

  /**
   * Build instructions for a batch of autodeploys
   */
  buildAutodeployInstructions(deploys, roundId) {
    const instructions = [];

    // Compute budget
    const cuLimit = Math.min(deploys.length * 400_000, 1_400_000);
    instructions.push(ComputeBudgetProgram.setComputeUnitLimit({ units: cuLimit }));
    instructions.push(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: this.config.priorityFee }));

    for (const deploy of deploys) {
      // Checkpoint if needed
      if (deploy.checkpointRound) {
        const checkpointIx = mmAutocheckpointInstruction(
          this.pubkey,
          deploy.deployer.managerAddress,
          deploy.checkpointRound,
          deploy.authId
        );
        instructions.push(checkpointIx);
        
        // Recycle after checkpoint
        const recycleIx = recycleSolInstruction(
          this.pubkey,
          deploy.deployer.managerAddress,
          deploy.authId
        );
        instructions.push(recycleIx);
      }

      // Autodeploy
      const autodeployIx = mmAutodeployInstruction(
        this.pubkey,
        deploy.deployer.managerAddress,
        deploy.authId,
        roundId,
        deploy.amount,
        deploy.squaresMask,
        deploy.deployer.bpsFee,
        deploy.deployer.flatFee
      );
      instructions.push(autodeployIx);
    }

    return instructions;
  }

  /**
   * Execute batched autodeploys using versioned transaction with LUT
   */
  async executeBatchedAutoDeploysVersioned(deploys, roundId) {
    if (!this.lutManager || !this.lutManager.getLutAccount()) {
      throw new Error('LUT not loaded');
    }

    const instructions = this.buildAutodeployInstructions(deploys, roundId);
    const { blockhash } = await this.connection.getLatestBlockhash();

    // Build versioned transaction with LUT
    const lutAccount = this.lutManager.getAddressLookupTableAccount();
    
    const messageV0 = new TransactionMessage({
      payerKey: this.pubkey,
      recentBlockhash: blockhash,
      instructions,
    }).compileToV0Message([lutAccount]);

    const versionedTx = new VersionedTransaction(messageV0);
    versionedTx.sign([this.keypair]);

    const signature = await this.connection.sendRawTransaction(versionedTx.serialize());

    return signature;
  }

  /**
   * Execute batched autodeploys (legacy transaction, no LUT)
   */
  async executeBatchedAutodeploys(deploys, roundId) {
    const instructions = this.buildAutodeployInstructions(deploys, roundId);
    
    const tx = new Transaction().add(...instructions);
    tx.feePayer = this.pubkey;
    tx.recentBlockhash = (await this.connection.getLatestBlockhash()).blockhash;

    tx.sign(this.keypair);
    const signature = await this.connection.sendRawTransaction(tx.serialize());

    return signature;
  }

  /**
   * Execute checkpoint + recycle only
   */
  async executeCheckpointRecycle(deployer, authId, checkpointRound) {
    const tx = new Transaction()
      .add(ComputeBudgetProgram.setComputeUnitLimit({ units: 200_000 }))
      .add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: this.config.priorityFee }))
      .add(mmAutocheckpointInstruction(this.pubkey, deployer.managerAddress, checkpointRound, authId))
      .add(recycleSolInstruction(this.pubkey, deployer.managerAddress, authId));

    tx.feePayer = this.pubkey;
    tx.recentBlockhash = (await this.connection.getLatestBlockhash()).blockhash;

    tx.sign(this.keypair);
    const signature = await this.connection.sendRawTransaction(tx.serialize());

    return signature;
  }
}

// =============================================================================
// STRATEGY
// =============================================================================

async function runStrategy(crank, deployers, state) {
  const board = await crank.getBoard();

  if (board.endSlot === BigInt('18446744073709551615')) {
    return;
  }

  const slotsRemaining = board.endSlot - board.currentSlot;

  if (state.lastRoundId !== board.roundId) {
    console.log(`\nNew round detected: ${board.roundId} (ends in ${slotsRemaining} slots)`);
    state.lastRoundId = board.roundId;
    state.deployedRounds.clear();
  }

  if (slotsRemaining < MIN_SLOTS_TO_DEPLOY) {
    return;
  }

  if (slotsRemaining > DEPLOY_SLOTS_BEFORE_END) {
    return;
  }

  const toDeploy = [];

  for (const deployer of deployers) {
    const deployKey = `${deployer.deployerAddress.toBase58()}-${board.roundId}`;

    if (state.deployedRounds.has(deployKey)) {
      continue;
    }

    const checkpointRound = await crank.needsCheckpoint(deployer, AUTH_ID);

    const required = await crank.calculateRequiredBalance(
      deployer,
      AUTH_ID,
      DEPLOY_AMOUNT_LAMPORTS,
      SQUARES_MASK
    );

    const balance = await crank.getAutodeployBalance(deployer);

    if (balance >= required) {
      const checkpointInfo = checkpointRound ? ` (will checkpoint round ${checkpointRound})` : '';
      console.log(`  Adding ${deployer.managerAddress.toBase58()}: balance ${formatSol(balance)} >= required ${formatSol(required)}${checkpointInfo}`);
      
      toDeploy.push({
        deployer,
        authId: AUTH_ID,
        amount: DEPLOY_AMOUNT_LAMPORTS,
        squaresMask: SQUARES_MASK,
        checkpointRound,
      });
    } else if (checkpointRound) {
      console.log(`  ${deployer.managerAddress.toBase58()} needs checkpoint but insufficient balance for deploy`);
      try {
        const sig = await crank.executeCheckpointRecycle(deployer, AUTH_ID, checkpointRound);
        console.log(`  ✓ Checkpoint+recycle: ${sig}`);
      } catch (err) {
        console.error(`  ✗ Checkpoint+recycle failed: ${err.message}`);
      }
    } else {
      console.log(`  Skipping ${deployer.managerAddress.toBase58()}: insufficient balance (${formatSol(balance)} < ${formatSol(required)})`);
    }
  }

  if (toDeploy.length > 0) {
    console.log(`\nDeploying for ${toDeploy.length} managers (round ${board.roundId})`);

    const hasLut = crank.lutManager && crank.lutManager.getLutAccount();
    const batchSize = hasLut ? MAX_BATCH_SIZE_WITH_LUT : MAX_BATCH_SIZE_NO_LUT;

    for (let i = 0; i < toDeploy.length; i += batchSize) {
      const batch = toDeploy.slice(i, i + batchSize);
      const deployerKeys = batch.map(d => d.deployer.deployerAddress.toBase58());
      const checkpointsInBatch = batch.filter(d => d.checkpointRound).length;

      try {
        let sig;
        if (hasLut) {
          sig = await crank.executeBatchedAutoDeploysVersioned(batch, board.roundId);
          console.log(`  ✓ Versioned autodeploy (${batch.length} deployers, ${checkpointsInBatch} checkpoints, with LUT): ${sig}`);
        } else {
          sig = await crank.executeBatchedAutodeploys(batch, board.roundId);
          console.log(`  ✓ Batched autodeploy (${batch.length} deployers): ${sig}`);
        }

        for (const key of deployerKeys) {
          state.deployedRounds.add(`${key}-${board.roundId}`);
        }
      } catch (err) {
        console.error(`  ✗ Autodeploy failed: ${err.message}`);
      }
    }
  }
}

// =============================================================================
// HELPERS
// =============================================================================

function countBits(n) {
  let count = 0;
  while (n) {
    count += n & 1;
    n >>>= 1;
  }
  return count;
}

function loadKeypair(path) {
  const data = fs.readFileSync(path, 'utf8');
  const secretKey = Uint8Array.from(JSON.parse(data));
  return Keypair.fromSecretKey(secretKey);
}

// =============================================================================
// MAIN
// =============================================================================

async function main() {
  program
    .name('evore-js-crank')
    .description('Automated deployer crank for Evore (built with @solana/web3.js)')
    .version('0.1.0');

  program
    .command('run')
    .description('Run the main crank loop (default)')
    .action(runCrank);

  program
    .command('list')
    .description('Show deployer accounts we manage')
    .action(listDeployers);

  program
    .command('test')
    .description('Send a test transaction to verify connectivity')
    .action(testTransaction);

  program
    .command('create-lut')
    .description('Create a new Address Lookup Table')
    .action(createLut);

  program
    .command('extend-lut')
    .description('Extend LUT with deployer accounts')
    .action(extendLut);

  program
    .command('show-lut')
    .description('Show LUT contents')
    .action(showLut);

  if (process.argv.length === 2) {
    process.argv.push('run');
  }

  await program.parseAsync();
}

async function createCrank() {
  const rpcUrl = process.env.RPC_URL || 'https://api.mainnet-beta.solana.com';
  const keypairPath = process.env.DEPLOY_AUTHORITY_KEYPAIR;
  const priorityFee = parseInt(process.env.PRIORITY_FEE || '100000');
  const pollIntervalMs = parseInt(process.env.POLL_INTERVAL_MS || '400');
  const lutAddress = process.env.LUT_ADDRESS || null;

  if (!keypairPath) {
    console.error('Error: DEPLOY_AUTHORITY_KEYPAIR environment variable is required');
    process.exit(1);
  }

  const connection = new Connection(rpcUrl, 'confirmed');
  const keypair = loadKeypair(keypairPath);
  const pubkey = keypair.publicKey;

  console.log('Evore JS Crank (built with @solana/web3.js)');
  console.log(`RPC URL: ${rpcUrl}`);
  console.log(`Deploy authority: ${pubkey.toBase58()}`);

  return new Crank(connection, keypair, pubkey, { priorityFee, pollIntervalMs, lutAddress });
}

async function listDeployers() {
  const crank = await createCrank();
  
  console.log('\nFinding deployers...');
  const deployers = await crank.findDeployers();

  if (deployers.length === 0) {
    console.log('\nNo deployers found where we are the deploy_authority');
    console.log(`Create a deployer with deploy_authority set to: ${crank.pubkey.toBase58()}`);
    return;
  }

  console.log(`\nManaging ${deployers.length} deployers:`);
  for (const d of deployers) {
    const balance = await crank.getAutodeployBalance(d);
    console.log(`  Manager: ${d.managerAddress.toBase58()}`);
    console.log(`    Deployer: ${d.deployerAddress.toBase58()}`);
    console.log(`    Fee: ${formatFee(d.bpsFee, d.flatFee)}`);
    console.log(`    Balance: ${formatSol(balance)} SOL`);
  }
}

async function testTransaction() {
  const crank = await createCrank();

  console.log('\nSending test transaction...');
  try {
    const sig = await crank.sendTestTransaction();
    console.log(`✓ Test transaction sent: ${sig}`);
  } catch (err) {
    console.error(`✗ Test transaction failed: ${err.message}`);
    process.exit(1);
  }
}

async function createLut() {
  const crank = await createCrank();
  await crank.initLut();

  console.log('\nCreating new Address Lookup Table...');
  try {
    const lutAddress = await crank.lutManager.createLut(crank.keypair);
    console.log(`✓ LUT created: ${lutAddress.toBase58()}`);
    console.log(`Add to .env: LUT_ADDRESS=${lutAddress.toBase58()}`);
  } catch (err) {
    console.error(`✗ Failed to create LUT: ${err.message}`);
    process.exit(1);
  }
}

async function extendLut() {
  const crank = await createCrank();
  
  if (!crank.config.lutAddress) {
    console.error('Error: LUT_ADDRESS not set in .env');
    process.exit(1);
  }

  await crank.initLut(new PublicKey(crank.config.lutAddress));

  console.log('\nFinding deployers to add to LUT...');
  const deployers = await crank.findDeployers();

  if (deployers.length === 0) {
    console.log('No deployers found');
    return;
  }

  const board = await crank.getBoard();
  const missing = crank.lutManager.getMissingDeployerAddresses(deployers, AUTH_ID, board.roundId);

  if (missing.length === 0) {
    console.log('LUT already contains all deployer addresses');
    return;
  }

  console.log(`Adding ${missing.length} addresses to LUT...`);
  try {
    await crank.lutManager.extendLut(crank.keypair, missing);
    console.log(`✓ Added ${missing.length} addresses to LUT`);
  } catch (err) {
    console.error(`✗ Failed to extend LUT: ${err.message}`);
    process.exit(1);
  }
}

async function showLut() {
  const crank = await createCrank();
  
  if (!crank.config.lutAddress) {
    console.error('Error: LUT_ADDRESS not set in .env');
    process.exit(1);
  }

  await crank.initLut(new PublicKey(crank.config.lutAddress));
  const lutAccount = crank.lutManager.getLutAccount();

  console.log(`\nLUT Address: ${crank.config.lutAddress}`);
  console.log(`Contains ${lutAccount.state.addresses.length} addresses:`);
  
  for (let i = 0; i < lutAccount.state.addresses.length; i++) {
    console.log(`  [${i}] ${lutAccount.state.addresses[i].toBase58()}`);
  }
}

async function runCrank() {
  const crank = await createCrank();
  const pollIntervalMs = parseInt(process.env.POLL_INTERVAL_MS || '400');

  // Load LUT if configured
  if (crank.config.lutAddress) {
    try {
      await crank.initLut(new PublicKey(crank.config.lutAddress));
      console.log(`Using LUT: ${crank.config.lutAddress} (enables batching up to ${MAX_BATCH_SIZE_WITH_LUT} deploys/tx)`);
    } catch (err) {
      console.warn(`Failed to load LUT: ${err.message}. Running without LUT.`);
    }
  } else {
    console.log('No LUT configured. Run \'create-lut\' to create one for better batching.');
  }

  const deployers = await crank.findDeployers();

  if (deployers.length === 0) {
    console.log('\nNo deployers found where we are the deploy_authority');
    console.log(`Create a deployer with deploy_authority set to: ${crank.pubkey.toBase58()}`);
    return;
  }

  console.log(`\nManaging ${deployers.length} deployers`);
  console.log(`Strategy: deploy ${formatSol(DEPLOY_AMOUNT_LAMPORTS)} SOL/square, ${countBits(SQUARES_MASK)} squares, ${DEPLOY_SLOTS_BEFORE_END} slots before end`);
  console.log(`Poll interval: ${pollIntervalMs}ms`);
  console.log('\nStarting main loop...\n');

  const state = {
    lastRoundId: null,
    deployedRounds: new Set(),
  };

  while (true) {
    try {
      await runStrategy(crank, deployers, state);
    } catch (err) {
      console.error(`Strategy error: ${err.message}`);
    }

    await new Promise(resolve => setTimeout(resolve, pollIntervalMs));
  }
}

main().catch(err => {
  console.error(err);
  process.exit(1);
});
