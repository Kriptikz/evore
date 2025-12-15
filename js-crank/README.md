# Evore JS Crank

JavaScript reference implementation of the Evore autodeploy crank using the `evore-sdk` and `@solana/web3.js`.

## Overview

This crank automatically deploys to ORE rounds for all deployer accounts where your wallet is set as the `deploy_authority`. It's a JavaScript port of the Rust crank for those who prefer Node.js.

**LUT Compatibility**: This crank uses the same Address Lookup Table format as the Rust crank, so you can share a single LUT between both cranks.

## Setup

1. **Install dependencies:**

```bash
npm install
```

2. **Configure environment:**

Copy `.env.example` to `.env` and update:

```bash
cp .env.example .env
```

Required:
- `DEPLOY_AUTHORITY_KEYPAIR`: Path to your keypair JSON file

Optional:
- `RPC_URL`: Solana RPC endpoint (default: mainnet-beta)
- `PRIORITY_FEE`: Priority fee in microlamports (default: 100000)
- `POLL_INTERVAL_MS`: How often to check board state (default: 400)
- `LUT_ADDRESS`: Address Lookup Table for batched transactions (shared with Rust crank)

## Usage

### Commands

```bash
# Run the main crank loop
npm start
# or
node src/index.js run

# List deployers we manage
npm run list
# or
node src/index.js list

# Send test transaction to verify setup
npm test
# or
node src/index.js test

# Create a new Address Lookup Table
node src/index.js create-lut

# Extend LUT with deployer accounts
node src/index.js extend-lut

# Show LUT contents
node src/index.js show-lut
```

## Address Lookup Tables (LUTs)

The js-crank uses the same LUT architecture as the Rust crank:

- **One shared LUT** for static accounts (10 accounts that never change)
- **One LUT per miner** for miner-specific accounts (5 accounts each)
- **Round addresses NOT in LUT** (they change each round)

This enables batching up to **7 deployers per transaction** with LUTs (vs ~2 without).

### LUT Compatibility with Rust Crank

The js-crank is fully compatible with the Rust crank's LUT setup:

- **Shared LUT**: Both cranks recognize and use the same shared LUT
- **Miner LUTs**: Per-miner LUTs created by either crank work with both
- **Same authority**: Both cranks must use the same deploy authority keypair

### Setting Up LUTs

```bash
# Create shared LUT and per-miner LUTs automatically
node src/index.js setup-luts
```

This will:
1. Create a shared LUT with static accounts (if not exists)
2. Create individual LUTs for each miner (if not exists)

### Viewing LUTs

```bash
node src/index.js show-luts
```

## Customizing the Deployment Strategy

The deployment strategy is configured via constants at the top of `src/index.js`:

```javascript
// Amount to deploy per square (lamports)
const DEPLOY_AMOUNT_LAMPORTS = 10_000n;  // 0.00001 SOL

// Auth ID (0 unless using multiple miners per manager)
const AUTH_ID = 0n;

// Squares to deploy to (0x1FFFFFF = all 25 squares)
const SQUARES_MASK = 0x1FFFFFF;

// When to deploy (slots before round end)
const DEPLOY_SLOTS_BEFORE_END = 150n;

// Don't deploy if fewer slots remaining than this
const MIN_SLOTS_TO_DEPLOY = 10n;

// Max deploys per transaction (without/with LUTs)
const MAX_BATCH_SIZE_NO_LUT = 2;
const MAX_BATCH_SIZE_WITH_LUT = 7;  // With shared + per-miner LUTs
```

## Workflow

1. **Startup**: Scans for all deployer accounts where your keypair is the `deploy_authority`
2. **LUT Loading**: Loads/creates Address Lookup Tables for efficient batching
3. **Monitoring**: Polls the ORE board state every `POLL_INTERVAL_MS` milliseconds
4. **Deployment Window**: When `DEPLOY_SLOTS_BEFORE_END` slots remain, triggers deployments
5. **Full Autodeploy**: Uses `mmFullAutodeploy` which combines checkpoint + recycle + deploy in one instruction
6. **Batching**: Groups up to 7 deployers per tx using shared + per-miner LUTs

## Expected Fee Protection

The Deployer account has `expectedBpsFee` and `expectedFlatFee` fields stored on-chain. When non-zero, deploys will fail if actual fees don't match. This protects executors without needing to pass expected fees as instruction arguments (reduces tx size).

**Note**: Set expected fees via the Rust crank's `set-expected-fees` command or programmatically via `updateDeployerInstruction`.

## Requirements

- Node.js 18+
- Funded deploy authority keypair (for transaction fees)
- Users must have funded autodeploy balances
- Run `setup-luts` for batching more than 2 deployers

## Dependencies

- `@solana/web3.js` - Solana web3 SDK
- `evore-sdk` - Evore program SDK

## Differences from Rust Crank

This is a simplified reference implementation with the same core features:

- Same LUT architecture (shared + per-miner)
- Same `mmFullAutodeploy` instruction (checkpoint + recycle + deploy)
- Same batching limits (7 deployers/tx with LUTs)

The Rust crank additionally includes:

- SQLite database for state persistence
- More robust error handling and retries
- Automatic LUT creation/discovery

## License

MIT
