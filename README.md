# Evore

**The cheapest and most secure managed miner solution for Solana automining.**

Evore is an on-chain program that enables automated ORE v3 mining deployments while keeping users in full control of their assets. No private key exports, no custodial risk—just secure, permissioned automation.

## Repository Structure

```
evore/
├── program/        # Solana on-chain program (Rust)
├── sdk/            # JavaScript SDK for integration (evore-sdk)
├── crank/          # Rust crank for executing autodeploys
├── js-crank/       # JavaScript crank (Node.js alternative)
├── frontend/       # Example Next.js frontend
└── bot/            # Trading bot configuration
```

## Components

### 🔧 Program (`/program`)

The Solana on-chain program written in Rust. Handles:
- Manager account creation and management
- Deployer configuration with fee settings
- Autodeploy execution with fee collection
- Checkpoint and reward claiming
- SOL recycling for autominer

**Build:**
```bash
cd program
cargo build-sbf
```

### 📦 SDK (`/sdk`)

JavaScript SDK (`evore-sdk`) for integrating Evore into your application.

**Features:**
- All program instructions
- PDA derivation helpers
- Account decoders
- Transaction builders
- TypeScript type definitions

**Install:**
```bash
npm install evore-sdk @solana/web3.js
```

**Documentation:** See [`sdk/README.md`](./sdk/README.md) for full API documentation.

### ⚙️ Rust Crank (`/crank`)

Production-ready Rust crank for executing autodeploys. Features:
- SQLite state persistence
- Address Lookup Table (LUT) support for batching up to 7 deploys/tx
- Automatic LUT creation and discovery
- Configurable deployment strategies
- Expected fee management via `set-expected-fees` command

**Run:**
```bash
cd crank
cargo run -- run
```

**Documentation:** See [`crank/README.md`](./crank/README.md) for setup and configuration.

### 🟢 JS Crank (`/js-crank`)

JavaScript reference implementation of the crank using Node.js.

**Features:**
- Full LUT support (shared with Rust crank)
- Up to 7 deployers per transaction with LUT
- Simple configuration via `.env`

**Run:**
```bash
cd js-crank
npm install
npm start
```

**Documentation:** See [`js-crank/README.md`](./js-crank/README.md) for setup and commands.

### 🖥️ Frontend (`/frontend`)

Example Next.js frontend demonstrating:
- Wallet connection
- Manager/Deployer creation
- Deposit/Withdraw flows
- Miner status display

**Run:**
```bash
cd frontend
npm install
npm run dev
```

## Why Evore?

### Lowest Fees
- **Base protocol fee**: Just 1,000 lamports per deploy (~$0.00015)
- Cheaper than any wallet managing service

### On-Chain Security
- Users keep **full control** through their existing wallet
- **No private key exports** required
- All permissions enforced on-chain

### Limited Executor Permissions
The executor (crank) can **ONLY**:
- Deploy from deposited autodeploy balance
- Checkpoint rounds
- Recycle SOL (compound winnings)

The executor **CANNOT**:
- Claim rewards
- Withdraw funds
- Change fee settings

### User-Controlled Fees
- Only users (manager authority) can set the `bpsFee` and `flatFee` on the Deployer
- Fee changes require user signature

### Executor Fee Protection
- The Deployer stores `expectedBpsFee` and `expectedFlatFee` fields
- Only the executor (deploy_authority) can set expected fees via `updateDeployer`
- If expected fee > 0, the actual fee must match for deploys to succeed
- This protects executors from users changing fees mid-flight
- Using account fields instead of instruction args reduces transaction size

### Transferring Manager Authority
- Use `transferManagerInstruction` to transfer manager authority to a new public key
- **Important**: This transfers all associated mining accounts (deployer, miner, automation, etc.)
- The new authority gains full control over claims, withdrawals, and fee settings
- This operation is irreversible without the new authority's cooperation

## Quick Start

### For Users

1. Connect wallet to a platform using Evore
2. Create a Manager account (your miner container)
3. Deposit SOL to autodeploy balance
4. Platform's crank handles deployments automatically

### For Platforms

1. Deploy or use existing Evore program
2. Set up your executor crank (Rust or JS)
3. Integrate SDK into your frontend
4. Configure fee structure (bpsFee + flatFee)

```javascript
const { buildCreateAutoMinerInstructions } = require('evore-sdk');

// Create miner for user
const instructions = buildCreateAutoMinerInstructions(
  userWallet,
  managerKeypair.publicKey,
  platformExecutor,
  bpsFee,
  flatFee
);
```

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   User      │     │   Platform  │     │   ORE v3    │
│   Wallet    │     │   Crank     │     │   Program   │
└──────┬──────┘     └──────┬──────┘     └──────┬──────┘
       │                   │                   │
       │  Create Manager   │                   │
       │  + Deployer       │                   │
       │───────────────────>                   │
       │                   │                   │
       │  Deposit SOL      │                   │
       │───────────────────>                   │
       │                   │                   │
       │                   │  Autodeploy       │
       │                   │───────────────────>
       │                   │                   │
       │                   │  Checkpoint       │
       │                   │───────────────────>
       │                   │                   │
       │  Claim Rewards    │                   │
       │───────────────────>                   │
       └───────────────────┴───────────────────┘
```

## Development

### Prerequisites
- Rust 1.75+
- Solana CLI 1.18+
- Node.js 18+

### Building the Program
```bash
cd program
cargo build-sbf
```

### Running Tests
```bash
cd program
cargo test-sbf
```

### Local Development
```bash
# Start local validator
solana-test-validator

# Deploy program
solana program deploy target/deploy/evore.so
```

## License

MIT License - see [LICENSE](./LICENSE) for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Links

- [SDK Documentation](./sdk/README.md)
- [Rust Crank Documentation](./crank/README.md)
- [JS Crank Documentation](./js-crank/README.md)
