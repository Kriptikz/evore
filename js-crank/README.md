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

LUTs are required for efficiently batching multiple deployers in a single transaction. Without a LUT, transactions are limited to ~2 deployers. With a LUT, you can batch up to 5 deployers per transaction.

### LUT Compatibility with Rust Crank

The js-crank uses the same LUT format and account structure as the Rust crank. This means:

- **Shared LUT**: You can use the same `LUT_ADDRESS` in both the Rust crank's `.env` and the js-crank's `.env`
- **Cross-compatibility**: A LUT created by the Rust crank works with the js-crank, and vice versa
- **Same authority**: Both cranks must use the same deploy authority keypair for the same LUT

### Setting Up a LUT

**Option 1: Create with js-crank**
```bash
node src/index.js create-lut
# Copy the output address to your .env: LUT_ADDRESS=<address>
node src/index.js extend-lut
```

**Option 2: Use existing Rust crank LUT**
```bash
# Just copy the LUT_ADDRESS from your Rust crank's .env to the js-crank's .env
```

**Option 3: Create with Rust crank**
```bash
cargo run -- create-lut
# Use the same LUT_ADDRESS in the js-crank's .env
```

### Adding New Deployers to LUT

After adding new deployers, run `extend-lut` to add their accounts to the LUT:

```bash
node src/index.js extend-lut
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

// Max deploys per transaction (without/with LUT)
const MAX_BATCH_SIZE_NO_LUT = 2;
const MAX_BATCH_SIZE_WITH_LUT = 5;
```

## Workflow

1. **Startup**: Scans for all deployer accounts where your keypair is the `deploy_authority`
2. **LUT Loading**: If configured, loads the Address Lookup Table for efficient batching
3. **Monitoring**: Polls the ORE board state every `POLL_INTERVAL_MS` milliseconds
4. **Deployment Window**: When `DEPLOY_SLOTS_BEFORE_END` slots remain, triggers deployments
5. **Checkpointing**: Automatically checkpoints previous rounds if needed
6. **Batching**: Groups multiple deployers using versioned transactions with LUT

## Requirements

- Node.js 18+
- Funded deploy authority keypair (for transaction fees)
- Users must have funded autodeploy balances
- LUT for batching more than 2 deployers

## Dependencies

- `@solana/web3.js` - Solana web3 SDK
- `evore-sdk` - Evore program SDK

## Differences from Rust Crank

This is a simplified reference implementation. The Rust crank includes:

- SQLite database for state persistence
- Jito bundle support
- Helius API integration
- More robust error handling and retries

For production use with many deployers, consider the Rust crank.

## License

MIT
