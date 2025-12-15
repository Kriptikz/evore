#!/usr/bin/env node
/**
 * Evore Autodeploy Crank (JavaScript)
 * 
 * Reference implementation for automated deploying via the Evore program.
 * Built with @solana/web3.js v1.x
 * 
 * LUT Architecture:
 * - One shared LUT for static accounts (10 accounts including deploy authority)
 * - One LUT per miner for their 5 specific accounts
 * - Round addresses are NOT in any LUT (changes each round)
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
  ORE_TREASURY_ADDRESS,
  SYSTEM_PROGRAM_ID,
  DEPLOYER_DISCRIMINATOR,
  DEPLOY_FEE,
  ORE_CHECKPOINT_FEE,
  getDeployerPda,
  getManagedMinerAuthPda,
  getOreMinerPda,
  getOreBoardPda,
  getOreRoundPda,
  getOreConfigPda,
  getOreAutomationPda,
  getEntropyVarPda,
  decodeDeployer,
  decodeOreBoard,
  decodeOreMiner,
  mmFullAutodeployInstruction,
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
const MAX_BATCH_SIZE_WITH_LUT = 7;

// =============================================================================
// RENT CONSTANTS
// =============================================================================
const AUTH_PDA_RENT = 890_880n;
const MINER_RENT_ESTIMATE = 2_500_000n;

// =============================================================================
// LUT HELPERS
// =============================================================================

/**
 * Get static shared accounts (accounts that don't change between rounds)
 * Total: 10 fixed accounts (including deploy authority)
 */
function getStaticSharedAccounts(deployAuthority) {
  const [boardAddress] = getOreBoardPda();
  const [configAddress] = getOreConfigPda();
  const [entropyVarAddress] = getEntropyVarPda(boardAddress, 0n);

  return [
    deployAuthority,        // The crank's signer
    EVORE_PROGRAM_ID,       // Evore program
    SYSTEM_PROGRAM_ID,
    ORE_PROGRAM_ID,
    ENTROPY_PROGRAM_ID,
    FEE_COLLECTOR,
    boardAddress,
    configAddress,
    ORE_TREASURY_ADDRESS,
    entropyVarAddress,
  ];
}

/**
 * Get the 5 accounts specific to a miner (for per-miner LUT)
 */
function getMinerAccounts(manager, authId) {
  const [deployerAddr] = getDeployerPda(manager);
  const [managedMinerAuth] = getManagedMinerAuthPda(manager, authId);
  const [oreMiner] = getOreMinerPda(managedMinerAuth);
  const [automation] = getOreAutomationPda(managedMinerAuth);

  return [
    manager,
    deployerAddr,
    managedMinerAuth,
    oreMiner,
    automation,
  ];
}

/**
 * Get the miner_auth PDA for a manager/authId
 */
function getMinerAuthPda(manager, authId) {
  const [managedMinerAuth] = getManagedMinerAuthPda(manager, authId);
  return managedMinerAuth;
}

// =============================================================================
// LUT REGISTRY
// =============================================================================

/**
 * Registry that manages multiple LUTs:
 * - One shared LUT for static accounts
 * - Per-miner LUTs for miner-specific accounts
 */
class LutRegistry {
  constructor(connection, authority) {
    this.connection = connection;
    this.authority = authority;
    this.sharedLut = null;
    this.sharedLutAccounts = new Set();
    this.minerLuts = new Map(); // miner_auth -> lut_address
    this.lutCache = new Map();  // lut_address -> AddressLookupTableAccount
  }

  /**
   * Load all LUTs owned by our authority
   */
  async loadAllLuts() {
    console.log(`Scanning for LUTs owned by authority ${this.authority.toBase58()}...`);

    const lutProgramId = new PublicKey('AddressLookupTab1e1111111111111111111111111');
    
    // Get all accounts owned by the LUT program with our authority
    const accounts = await this.connection.getProgramAccounts(lutProgramId, {
      filters: [
        {
          memcmp: {
            offset: 22, // Authority offset in AddressLookupTable
            bytes: this.authority.toBase58(),
          },
        },
      ],
    });

    console.log(`Found ${accounts.length} LUTs owned by authority`);

    for (const { pubkey: lutAddress, account } of accounts) {
      try {
        const state = AddressLookupTableAccount.deserialize(account.data);
        const addresses = state.addresses;

        // Cache the LUT
        this.lutCache.set(lutAddress.toBase58(), {
          key: lutAddress,
          state: state,
        });

        // Determine if this is the shared LUT or a miner LUT
        const staticAccounts = getStaticSharedAccounts(this.authority);
        const addressStrings = addresses.map(a => a.toBase58());
        const hasAllStatic = staticAccounts.every(acc => addressStrings.includes(acc.toBase58()));

        if (hasAllStatic && !this.sharedLut) {
          // This looks like the shared LUT
          this.sharedLut = lutAddress;
          this.sharedLutAccounts.clear();
          for (const addr of addresses) {
            this.sharedLutAccounts.add(addr.toBase58());
          }
          console.log(`  Identified shared LUT: ${lutAddress.toBase58()} (${addresses.length} addresses)`);
        } else if (addresses.length === 5) {
          // This looks like a miner LUT (5 accounts per miner)
          // miner_auth is at index 2 (after manager, deployer)
          const minerAuth = addresses[2];
          this.minerLuts.set(minerAuth.toBase58(), lutAddress);
          console.log(`  Identified miner LUT: ${lutAddress.toBase58()} for miner_auth ${minerAuth.toBase58()}`);
        } else {
          console.log(`  Unknown LUT: ${lutAddress.toBase58()} (${addresses.length} addresses)`);
        }
      } catch (err) {
        console.warn(`  Failed to deserialize LUT ${lutAddress.toBase58()}: ${err.message}`);
      }
    }

    console.log(`Loaded ${this.minerLuts.size} miner LUTs`);
    return accounts.length;
  }

  /**
   * Get the shared LUT address
   */
  getSharedLut() {
    return this.sharedLut;
  }

  /**
   * Check if a miner has a LUT
   */
  hasMinerLut(minerAuth) {
    return this.minerLuts.has(minerAuth.toBase58());
  }

  /**
   * Get missing static addresses from the shared LUT
   */
  getMissingSharedAddresses() {
    const staticAccounts = getStaticSharedAccounts(this.authority);
    return staticAccounts.filter(addr => !this.sharedLutAccounts.has(addr.toBase58()));
  }

  /**
   * Get LUT accounts for a list of miner_auth PDAs
   * Returns shared LUT + all relevant miner LUTs
   */
  getLutsForMiners(minerAuths) {
    const luts = [];

    // Always include shared LUT if available
    if (this.sharedLut) {
      const lutAccount = this.lutCache.get(this.sharedLut.toBase58());
      if (lutAccount) {
        luts.push(lutAccount);
      }
    }

    // Add miner-specific LUTs
    for (const minerAuth of minerAuths) {
      const lutAddress = this.minerLuts.get(minerAuth.toBase58());
      if (lutAddress) {
        const lutAccount = this.lutCache.get(lutAddress.toBase58());
        if (lutAccount) {
          luts.push(lutAccount);
        }
      }
    }

    return luts;
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
    await this.connection.confirmTransaction(signature);

    console.log(`Created LUT: ${lutAddress.toBase58()}`);
    console.log(`Transaction: ${signature}`);

    return lutAddress;
  }

  /**
   * Extend LUT with new addresses
   */
  async extendLut(keypair, lutAddress, newAddresses) {
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
        lookupTable: lutAddress,
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

      console.log(`Extended LUT with ${chunk.length} addresses: ${signature}`);
      await this.connection.confirmTransaction(signature);
    }

    return signatures;
  }

  /**
   * Create and extend shared LUT if needed
   */
  async ensureSharedLut(keypair) {
    if (!this.sharedLut) {
      console.log('Creating shared LUT...');
      const lutAddress = await this.createLut(keypair);
      this.sharedLut = lutAddress;
      
      // Wait for LUT to be active
      await new Promise(resolve => setTimeout(resolve, 2000));
    }

    const missing = this.getMissingSharedAddresses();
    if (missing.length > 0) {
      console.log(`Adding ${missing.length} static accounts to shared LUT...`);
      await this.extendLut(keypair, this.sharedLut, missing);
      
      // Update cache
      for (const addr of missing) {
        this.sharedLutAccounts.add(addr.toBase58());
      }
    }

    return this.sharedLut;
  }

  /**
   * Create LUT for a miner if they don't have one
   */
  async ensureMinerLut(keypair, manager, authId) {
    const minerAuth = getMinerAuthPda(manager, authId);
    
    if (this.hasMinerLut(minerAuth)) {
      return this.minerLuts.get(minerAuth.toBase58());
    }

    console.log(`Creating LUT for miner ${minerAuth.toBase58()}...`);
    const lutAddress = await this.createLut(keypair);
    
    // Wait for LUT to be active
    await new Promise(resolve => setTimeout(resolve, 2000));
    
    // Add miner accounts
    const minerAccounts = getMinerAccounts(manager, authId);
    await this.extendLut(keypair, lutAddress, minerAccounts);
    
    // Register in registry
    this.minerLuts.set(minerAuth.toBase58(), lutAddress);
    
    // Cache
    const state = AddressLookupTableAccount.deserialize(
      (await this.connection.getAccountInfo(lutAddress)).data
    );
    this.lutCache.set(lutAddress.toBase58(), { key: lutAddress, state });
    
    return lutAddress;
  }

  /**
   * Build a versioned transaction with LUTs
   */
  buildVersionedTx(keypair, instructions, lutAccounts, recentBlockhash) {
    const messageV0 = new TransactionMessage({
      payerKey: this.authority,
      recentBlockhash,
      instructions,
    }).compileToV0Message(lutAccounts);

    const tx = new VersionedTransaction(messageV0);
    tx.sign([keypair]);
    return tx;
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
    this.registry = null;
  }

  /**
   * Initialize LUT registry and load existing LUTs
   */
  async initRegistry() {
    this.registry = new LutRegistry(this.connection, this.pubkey);
    await this.registry.loadAllLuts();
    return this.registry;
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
        
        if (deployer.deployAuthority.toBase58() !== this.pubkey.toBase58()) {
          continue;
        }

        deployers.push({
          deployerAddress,
          managerAddress: deployer.managerKey,
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
   * Get autodeploy balance (managed_miner_auth balance)
   */
  async getAutodeployBalance(deployer) {
    const [managedMinerAuth] = getManagedMinerAuthPda(deployer.managerAddress, AUTH_ID);
    const balance = await this.connection.getBalance(managedMinerAuth);
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

    const requiredMinerBalance = AUTH_PDA_RENT + ORE_CHECKPOINT_FEE + totalDeployed + minerRent + deployerFee + protocolFee;
    
    return requiredMinerBalance;
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
    await this.connection.confirmTransaction(signature);
    
    return signature;
  }

  /**
   * Build mmFullAutodeploy instructions for a batch of deploys
   */
  buildFullAutodeployInstructions(deploys, roundId) {
    const instructions = [];

    // Compute budget - ~400k CU per full autodeploy
    const cuLimit = Math.min(deploys.length * 400_000, 1_400_000);
    instructions.push(ComputeBudgetProgram.setComputeUnitLimit({ units: cuLimit }));
    instructions.push(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: this.config.priorityFee }));

    for (const deploy of deploys) {
      // Use checkpoint round if needed, otherwise use current round
      const checkpointRoundId = deploy.checkpointRound || roundId;
      
      const ix = mmFullAutodeployInstruction(
        this.pubkey,
        deploy.deployer.managerAddress,
        deploy.authId,
        roundId,
        checkpointRoundId,
        deploy.amount,
        deploy.squaresMask
      );
      instructions.push(ix);
    }

    return instructions;
  }

  /**
   * Execute batched autodeploys using versioned transaction with LUTs
   */
  async executeBatchedAutoDeploysVersioned(deploys, roundId) {
    if (!this.registry || !this.registry.getSharedLut()) {
      throw new Error('LUT registry not initialized or shared LUT not available');
    }

    const instructions = this.buildFullAutodeployInstructions(deploys, roundId);
    const { blockhash } = await this.connection.getLatestBlockhash();

    // Get miner_auths for LUT lookup
    const minerAuths = deploys.map(d => getMinerAuthPda(d.deployer.managerAddress, d.authId));
    const lutAccounts = this.registry.getLutsForMiners(minerAuths);

    const tx = this.registry.buildVersionedTx(this.keypair, instructions, lutAccounts, blockhash);
    
    // Log transaction size
    const txBytes = tx.serialize();
    console.log(`Sending versioned tx: ${txBytes.length} bytes (limit 1232)`);

    const signature = await this.connection.sendRawTransaction(txBytes);
    return signature;
  }

  /**
   * Execute batched autodeploys (legacy transaction, no LUT)
   */
  async executeBatchedAutodeploys(deploys, roundId) {
    const instructions = this.buildFullAutodeployInstructions(deploys, roundId);
    
    const tx = new Transaction().add(...instructions);
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
    } else {
      console.log(`  Skipping ${deployer.managerAddress.toBase58()}: insufficient balance (${formatSol(balance)} < ${formatSol(required)})`);
    }
  }

  if (toDeploy.length > 0) {
    console.log(`\nDeploying for ${toDeploy.length} managers (round ${board.roundId})`);

    const hasLuts = crank.registry && crank.registry.getSharedLut();
    const batchSize = hasLuts ? MAX_BATCH_SIZE_WITH_LUT : MAX_BATCH_SIZE_NO_LUT;

    for (let i = 0; i < toDeploy.length; i += batchSize) {
      const batch = toDeploy.slice(i, i + batchSize);
      const deployerKeys = batch.map(d => d.deployer.deployerAddress.toBase58());
      const checkpointsInBatch = batch.filter(d => d.checkpointRound).length;

      try {
        let sig;
        if (hasLuts) {
          sig = await crank.executeBatchedAutoDeploysVersioned(batch, board.roundId);
          console.log(`  ✓ Full autodeploy (${batch.length} deployers, ${checkpointsInBatch} checkpoints, with LUTs): ${sig}`);
        } else {
          sig = await crank.executeBatchedAutodeploys(batch, board.roundId);
          console.log(`  ✓ Full autodeploy (${batch.length} deployers): ${sig}`);
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
    .command('setup-luts')
    .description('Setup shared LUT and per-miner LUTs')
    .action(setupLuts);

  program
    .command('show-luts')
    .description('Show all LUTs owned by authority')
    .action(showLuts);

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

  return new Crank(connection, keypair, pubkey, { priorityFee, pollIntervalMs });
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

async function setupLuts() {
  const crank = await createCrank();
  
  console.log('\nInitializing LUT registry...');
  await crank.initRegistry();

  console.log('\nEnsuring shared LUT exists...');
  await crank.registry.ensureSharedLut(crank.keypair);
  console.log(`Shared LUT: ${crank.registry.getSharedLut().toBase58()}`);

  console.log('\nFinding deployers...');
  const deployers = await crank.findDeployers();

  if (deployers.length === 0) {
    console.log('No deployers found');
    return;
  }

  console.log(`\nCreating LUTs for ${deployers.length} miners...`);
  for (const deployer of deployers) {
    const minerAuth = getMinerAuthPda(deployer.managerAddress, AUTH_ID);
    
    if (crank.registry.hasMinerLut(minerAuth)) {
      console.log(`  ✓ Miner ${minerAuth.toBase58()} already has LUT`);
    } else {
      try {
        const lutAddress = await crank.registry.ensureMinerLut(crank.keypair, deployer.managerAddress, AUTH_ID);
        console.log(`  ✓ Created LUT for miner ${minerAuth.toBase58()}: ${lutAddress.toBase58()}`);
      } catch (err) {
        console.error(`  ✗ Failed to create LUT for miner ${minerAuth.toBase58()}: ${err.message}`);
      }
    }
  }

  console.log('\n✓ LUT setup complete!');
}

async function showLuts() {
  const crank = await createCrank();
  
  console.log('\nLoading LUTs...');
  await crank.initRegistry();

  const sharedLut = crank.registry.getSharedLut();
  if (sharedLut) {
    console.log(`\nShared LUT: ${sharedLut.toBase58()}`);
    const lutAccount = crank.registry.lutCache.get(sharedLut.toBase58());
    if (lutAccount) {
      console.log(`  Contains ${lutAccount.state.addresses.length} addresses`);
    }
  } else {
    console.log('\nNo shared LUT found. Run "setup-luts" to create one.');
  }

  console.log(`\nMiner LUTs: ${crank.registry.minerLuts.size}`);
  for (const [minerAuth, lutAddress] of crank.registry.minerLuts) {
    console.log(`  ${minerAuth}: ${lutAddress.toBase58()}`);
  }
}

async function runCrank() {
  const crank = await createCrank();
  const pollIntervalMs = parseInt(process.env.POLL_INTERVAL_MS || '400');

  // Initialize LUT registry
  console.log('\nInitializing LUT registry...');
  await crank.initRegistry();

  const sharedLut = crank.registry.getSharedLut();
  if (sharedLut) {
    console.log(`Using shared LUT: ${sharedLut.toBase58()}`);
    console.log(`Miner LUTs: ${crank.registry.minerLuts.size}`);
    console.log(`Max batch size: ${MAX_BATCH_SIZE_WITH_LUT} deploys/tx`);
  } else {
    console.log('No shared LUT found. Run "setup-luts" to create LUTs for better batching.');
    console.log(`Max batch size: ${MAX_BATCH_SIZE_NO_LUT} deploys/tx`);
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
