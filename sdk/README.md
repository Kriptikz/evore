# Evore SDK

The **cheapest and most secure** managed miner solution for Solana automining.

Evore replaces expensive custodial wallet services (like Privy) with an on-chain program that gives users direct control of their assets while still enabling automated deployments. No private key exports, no custodial risk—just secure, permissioned automation.

## Why Evore?

### Lowest Fees in the Market

- **Base protocol fee**: Just **1,000 lamports** per deploy (~$0.00015)
- Cheaper than any wallet managing service, especially at scale
- Less fees to wallet managers = more revenue for your platform

### On-Chain Security Guarantees

- Users keep **full control** of their assets through their existing wallet
- **No private key exports** required—ever
- All permissions are enforced on-chain, not by trust

### Limited Executor Permissions

The executor (your crank) can **ONLY**:
- Deploy from the user's deposited autodeploy balance
- Checkpoint rounds (collect winnings to miner)
- Recycle SOL (compound winnings back to autodeploy balance)

The executor **CANNOT**:
- Claim rewards to any wallet
- Withdraw funds
- Change fee settings
- Do anything beyond deploying

### User-Controlled Fees

- Only the **user** (manager authority) can set `bpsFee` and `flatFee` on the Deployer
- Fee changes require the user's signature
- The executor cannot modify the user's fee settings

### Executor Fee Protection

- The Deployer account stores `expectedBpsFee` and `expectedFlatFee` fields
- Only the **executor** (deploy_authority) can set these via `updateDeployer`
- If expected fee > 0, the actual fee must match for deploys to succeed
- Using account fields instead of instruction args reduces transaction size
- Executors should set expected fees once when onboarding, then autodeploys just work

### Familiar User Experience

- Users connect with their existing wallet (Phantom, Backpack, etc.)
- No new accounts or unfamiliar flows
- Standard Solana transaction signing

### Multiple Miners Per Wallet

- Create multiple miners from a single wallet by generating new Manager keypairs
- Each Manager is independent—transfer authority on one doesn't affect others
- **Best practice**: Create a new Manager for each miner for maximum modularity
- The `authId` parameter defaults to `0` and should stay `0` unless you have a specific reason to create multiple miners under the same Manager (note: all miners under a Manager transfer together if authority changes)

### Transferring Manager Authority

Use `transferManagerInstruction` to transfer manager authority to a new public key.

**Important**: Transferring manager authority transfers **all** associated mining accounts (deployer, miner, automation, etc.) since they are derived from the manager. The new authority will have full control over claims, withdrawals, and fee settings.

```javascript
const { transferManagerInstruction } = require("evore-sdk");

// Transfer manager to new authority
const ix = transferManagerInstruction(
  currentAuthority,  // Current owner (must sign)
  managerAccount,    // The manager account
  newAuthority       // New owner pubkey
);
```

## Installation

```bash
npm install evore-sdk @solana/web3.js
```

## Quick Start

### For Users: Create an AutoMiner

```javascript
const { 
  Connection, 
  Keypair, 
  PublicKey, 
  Transaction 
} = require("@solana/web3.js");
const { 
  buildCreateAutoMinerInstructions,
  buildDepositInstructions,
  parseSolToLamports,
} = require("evore-sdk");

// Connect to Solana
const connection = new Connection("https://api.mainnet-beta.solana.com");

// User's wallet (you'd get this from wallet adapter in a real app)
const userWallet = /* wallet adapter or keypair */;

// Generate a new manager keypair (this will be the user's miner container)
const managerKeypair = Keypair.generate();

// Platform's executor pubkey (provided by the automining platform)
const platformExecutor = new PublicKey("...");

// Platform fees (set by the platform)
const bpsFee = 500n;  // 5%
const flatFee = 2000n; // 2000 lamports per deploy

// Build the setup instructions
const instructions = buildCreateAutoMinerInstructions(
  userWallet.publicKey,
  managerKeypair.publicKey,
  platformExecutor,
  bpsFee,
  flatFee
);

// Create transaction
const tx = new Transaction().add(...instructions);
tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
tx.feePayer = userWallet.publicKey;
tx.partialSign(managerKeypair);
// Then have user sign via wallet adapter...

// After creation, deposit funds
const depositInstructions = buildDepositInstructions(
  userWallet.publicKey,
  managerKeypair.publicKey,
  parseSolToLamports("1.0") // Deposit 1 SOL
);
const depositTx = new Transaction().add(...depositInstructions);

// To create another miner, just generate a new Manager keypair
const secondManagerKeypair = Keypair.generate();
const secondMinerInstructions = buildCreateAutoMinerInstructions(
  userWallet.publicKey,
  secondManagerKeypair.publicKey,
  platformExecutor,
  bpsFee,
  flatFee
);
// Each Manager is independent - perfect for multiple miners per user
```

### For Platforms: Execute Autodeploys

```javascript
const { Transaction, Keypair } = require("@solana/web3.js");
const {
  buildBatchedAutodeployInstructions,
  squaresToMask,
  decodeDeployer,
  getDeployerPda,
} = require("evore-sdk");

// Your executor keypair (fund this with SOL for transaction fees)
const executor = Keypair.fromSecretKey(/* your executor private key */);

// Fetch deployer to get current fees
const [deployerPda] = getDeployerPda(userManager);
const deployerAccount = await connection.getAccountInfo(deployerPda);
const deployer = decodeDeployer(deployerAccount.data);

// Build autodeploy for multiple users (up to 7 per tx)
// Each user can have different amounts and square selections
// Note: expectedBpsFee/expectedFlatFee are now stored on the Deployer account,
// not passed as instruction arguments (reduces transaction size)
const deploys = [
  {
    manager: user1Manager,
    authId: 0n,
    amount: 10000000n, // 0.01 SOL per square
    squaresMask: squaresToMask([
      true, true, true, true, true,  // User 1: first 5 squares
      false, false, false, false, false,
      false, false, false, false, false,
      false, false, false, false, false,
      false, false, false, false, false,
    ]),
  },
  {
    manager: user2Manager,
    authId: 0n,
    amount: 5000000n,
    squaresMask: 0b1111111111111111111111111, // User 2: all 25 squares
  },
];

const instructions = buildBatchedAutodeployInstructions(
  executor.publicKey,
  deploys,
  currentRoundId
);

// Create, sign and send
const tx = new Transaction().add(...instructions);
tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
tx.feePayer = executor.publicKey;
tx.sign(executor);
await connection.sendRawTransaction(tx.serialize());
```

## Core Concepts

### Manager Account

The Manager is the user's miner container. It stores:
- `authority` - The user's wallet pubkey (only this wallet can claim/withdraw)

**Multiple miners**: Users create multiple miners by generating new Manager keypairs (each Manager = one miner). The `authId` parameter exists for advanced use cases but should typically be `0`. If you use multiple `authId` values under the same Manager, **all miners transfer together** if the Manager authority changes. For modularity, create a new Manager for each miner.

### Deployer Account

The Deployer links a manager to an executor (deploy_authority). It stores:

| Field | Description | Set By |
|-------|-------------|--------|
| `managerKey` | The manager this deployer is for | System |
| `deployAuthority` | The executor pubkey that can autodeploy | User |
| `bpsFee` | Percentage fee in basis points (500 = 5%) | User |
| `flatFee` | Fixed lamport fee per deploy | User |
| `expectedBpsFee` | Expected bps fee (0 = accept any) | Executor |
| `expectedFlatFee` | Expected flat fee (0 = accept any) | Executor |

**Expected Fee Protection**: The `expectedBpsFee` and `expectedFlatFee` fields protect executors from fee changes. When set to a non-zero value, deploys will fail if the actual fees don't match. This eliminates the need to pass expected fees as instruction arguments, reducing transaction size.

### Autodeploy Balance

The **managed_miner_auth PDA** holds the user's deposited SOL for autodeploys. This is the same PDA that acts as the authority for the ORE miner.

- **Only the user** can deposit or withdraw
- **Only the executor** can deploy from it
- Funds are completely separated from user's main wallet
- Derived via `getManagedMinerAuthPda(manager, authId)`

### Security Model

```
User (Manager Authority) Controls:       Executor (Deploy Authority) Controls:
├── Deposit funds                        └── Deploy from autodeploy balance
├── Withdraw funds                           (amount, squares, timing)
├── Claim SOL/ORE rewards                └── Checkpoint rounds
├── Set bpsFee and flatFee               └── Recycle SOL
└── Change deploy_authority              └── Set expectedBpsFee/expectedFlatFee
```

## For Platform Integrators

### Setting Up Your Platform

1. **Create an Executor Wallet**
   ```javascript
   const executor = Keypair.generate();
   // Fund this wallet with SOL for transaction fees
   // Store the private key securely
   ```

2. **Set Your Fee Structure**
   ```javascript
   const bpsFee = 500n;   // 5% of deployed amount
   const flatFee = 2000n; // 2000 lamports per deploy (covers tx costs)
   ```
   
   Recommendation: Set `flatFee >= 2000` lamports to cover executor transaction costs, then add `bpsFee` for revenue.

3. **User Onboarding Flow**
   - User connects wallet
   - Generate manager keypair
   - Build `CreateAutoMiner` transaction with your executor
   - User signs (both user wallet and manager keypair)
   - Save the manager pubkey to your database

4. **Run Your Crank**
   - Monitor the ORE board for new rounds
   - Fetch all users with autodeploy balance
   - Build batched autodeploy transactions (up to 7 users per tx)
   - Sign and send with your executor

### Migration from Privy/Custodial Services

Evore dramatically reduces your custodial transaction load:

**Before (Privy):**
- Every deploy requires Privy signature
- Every checkpoint requires Privy signature
- High per-transaction costs

**After (Evore):**
- Only these need user signature:
  - Account creation (once)
  - Deposits
  - Withdrawals  
  - Claims
  - Fee updates
- Deploys, checkpoints, and recycling use your executor

You can run both systems in parallel during migration, gradually moving users to Evore accounts.

## Fee Structure

| Fee Type | Amount | Set By | Description |
|----------|--------|--------|-------------|
| Protocol Fee | 1,000 lamports | Evore (fixed) | Base fee for program maintenance |
| BPS Fee | Variable | Platform | Percentage of deployed amount |
| Flat Fee | Variable | Platform | Fixed fee per deploy |

Example: If a platform sets 5% BPS + 2000 lamports flat fee, and deploys 0.1 SOL:
- Protocol fee: 1,000 lamports
- BPS fee: 0.1 SOL × 5% = 5,000 lamports
- Flat fee: 2,000 lamports
- **Total fees**: 8,000 lamports (~$0.0012)

## API Reference

### Constants

```javascript
const {
  EVORE_PROGRAM_ID,      // Main program ID
  ORE_PROGRAM_ID,        // ORE v3 program
  FEE_COLLECTOR,         // Protocol fee collector
  DEPLOY_FEE,            // Base deploy fee (1000n lamports)
  LAMPORTS_PER_SOL,      // 1_000_000_000n
} = require("evore-sdk");
```

### PDA Functions

```javascript
const {
  getManagedMinerAuthPda,   // (manager, authId) => [pda, bump] - holds autodeploy balance
  getDeployerPda,           // (manager) => [pda, bump]
  getOreMinerPda,           // (authority) => [pda, bump]
  getOreBoardPda,           // () => [pda, bump]
  getOreRoundPda,           // (roundId) => [pda, bump]
} = require("evore-sdk");
```

### Account Decoders

```javascript
const {
  decodeManager,    // (data) => { authority }
  decodeDeployer,   // (data) => { managerKey, deployAuthority, bpsFee, flatFee, expectedBpsFee, expectedFlatFee }
  decodeOreBoard,   // (data) => { roundId, startSlot, endSlot, epochId }
  decodeOreMiner,   // (data) => { authority, deployed, rewardsSol, ... }
} = require("evore-sdk");
```

### Utility Functions

```javascript
const {
  formatSol,              // (lamports, decimals?) => "1.2345"
  formatBps,              // (bps) => "5%"
  formatFee,              // (bpsFee, flatFee) => "5% + 2000 lamports"
  parseSolToLamports,     // (sol) => bigint
  parsePercentToBps,      // (percent) => bigint
  calculateDeployerFee,   // (totalDeployed, bpsFee, flatFee) => bigint
  squaresToMask,          // (boolean[25]) => number
  maskToSquares,          // (number) => boolean[25]
} = require("evore-sdk");
```

### Instructions

```javascript
const {
  // Manager (user signs)
  createManagerInstruction,
  
  // Deploy strategies (user signs)
  evDeployInstruction,
  percentageDeployInstruction,
  manualDeployInstruction,
  splitDeployInstruction,
  
  // Checkpoint & Claims (user signs)
  mmCheckpointInstruction,
  mmClaimSolInstruction,
  mmClaimOreInstruction,
  
  // Deployer management
  createDeployerInstruction,      // (user signs) Create deployer with fees
  updateDeployerInstruction,      // (user OR executor signs) Update fees or expected fees
  
  // Balance management (user signs)
  depositAutodeployBalanceInstruction,
  withdrawAutodeployBalanceInstruction,
  
  // Autodeploy (executor signs)
  mmAutodeployInstruction,        // Deploy only
  mmAutocheckpointInstruction,    // Checkpoint only
  recycleSolInstruction,          // Recycle SOL only
  mmFullAutodeployInstruction,    // Combined: checkpoint + recycle + deploy (most efficient)
} = require("evore-sdk");
```

### Transaction Builders

```javascript
const {
  // Setup
  buildCreateAutoMinerTransaction,
  buildSetupAutoMinerTransaction,
  
  // User operations
  buildDepositTransaction,
  buildWithdrawTransaction,
  buildClaimSolTransaction,
  buildClaimOreTransaction,
  buildClaimAllTransaction,
  buildCheckpointAndClaimSolTransaction,
  
  // Executor operations
  buildAutodeployTransaction,
  buildCheckpointAndAutodeployTransaction,
  buildRecycleSolTransaction,
  buildBatchedAutodeployTransaction,
} = require("evore-sdk");
```

## Examples

### Checkpoint and Claim Rewards

```javascript
const { buildCheckpointAndClaimSolTransaction } = require("evore-sdk");

// Checkpoint round 42 and claim SOL rewards
const tx = buildCheckpointAndClaimSolTransaction(
  userWallet.publicKey,
  managerPubkey,
  42n,  // roundId
  0n    // authId
);
```

### Recycle SOL (Auto-compound)

```javascript
const { buildRecycleSolTransaction } = require("evore-sdk");

// Move SOL winnings from miner back to autodeploy balance
const tx = buildRecycleSolTransaction(
  executor.publicKey,
  userManager,
  0n  // authId
);
```

### Read Account Data

```javascript
const { getDeployerPda, decodeDeployer, formatFee } = require("evore-sdk");

const [deployerPda] = getDeployerPda(managerPubkey);
const account = await connection.getAccountInfo(deployerPda);
const deployer = decodeDeployer(account.data);

console.log("Deploy Authority:", deployer.deployAuthority.toBase58());
console.log("Fee:", formatFee(deployer.bpsFee, deployer.flatFee));
console.log("Expected Fee:", formatFee(deployer.expectedBpsFee, deployer.expectedFlatFee));
```

## TypeScript Support

The SDK includes TypeScript type definitions (`.d.ts` files). Types are automatically available when you import the package:

```typescript
import { 
  Deployer, 
  Manager,
  decodeDeployer,
  buildCreateAutoMinerTransaction,
} from "evore-sdk";

const deployer: Deployer = decodeDeployer(accountData);
```

## License

MIT
