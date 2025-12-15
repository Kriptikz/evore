# Evore Autodeploy Crank

Reference implementation for automated deploying via the Evore program. This crank scans for deployer accounts where your wallet is the `deploy_authority` and executes autodeploy transactions.

**This is a starting point** - customize the deployment strategy in `src/main.rs` for your specific use case.

## Features

- **Automatic Deployer Discovery**: Scans the Evore program for deployer accounts where you are the deploy_authority
- **Transaction Tracking**: SQLite database tracks all sent transactions with full status history
- **Address Lookup Tables**: Automatic LUT management for batching up to 7 deploys per transaction
- **Fee Protection**: Uses Deployer account's `expectedBpsFee`/`expectedFlatFee` fields (set once, no instruction args needed)

## Quick Start

1. Set environment variables:
   ```bash
   export RPC_URL="https://api.mainnet-beta.solana.com"
   export DEPLOY_AUTHORITY_KEYPAIR="/path/to/your/keypair.json"
   ```

2. Customize the strategy constants in `src/main.rs`:
   ```rust
   const DEPLOY_AMOUNT_LAMPORTS: u64 = 10_000_000;  // 0.01 SOL per square
   const MIN_BALANCE_LAMPORTS: u64 = 100_000_000;   // 0.1 SOL minimum
   const AUTH_ID: u64 = 0;                           // Which managed miner
   const SQUARES_MASK: u32 = 0x1FFFFFF;             // All 25 squares
   const DEPLOY_SLOTS_BEFORE_END: u64 = 5;          // Timing
   ```

3. Build and run:
   ```bash
   cargo build --release
   ./target/release/evore-crank
   ```

## Configuration

| Environment Variable | Description | Default |
|---------------------|-------------|---------|
| `RPC_URL` | Solana RPC URL | `https://api.mainnet-beta.solana.com` |
| `DEPLOY_AUTHORITY_KEYPAIR` | Path to deployer keypair JSON | Required |
| `DATABASE_PATH` | SQLite database path | `crank.db` |
| `PRIORITY_FEE` | Priority fee in microlamports/CU | `100000` |
| `POLL_INTERVAL_MS` | Poll interval in ms | `400` |
| `LUT_ADDRESS` | (Legacy) Manual LUT address | Auto-discovered |

## Commands

```bash
# Run the main crank loop (auto-discovers/creates LUTs)
cargo run -- run

# List deployers where you are deploy_authority
cargo run -- list

# Set expected fees on all deployers (protects against fee changes)
cargo run -- set-expected-fees --expected-bps-fee 0 --expected-flat-fee 5000

# Check all Evore accounts for legacy V1 deployers
cargo run -- check-accounts

# Send test transaction
cargo run -- test
```

## Expected Fee Protection

The Deployer account has `expectedBpsFee` and `expectedFlatFee` fields that only the deploy_authority can set. When non-zero, deploys will fail if actual fees don't match. This protects executors without needing instruction arguments.

**Set expected fees once** after users create their deployers:
```bash
cargo run -- set-expected-fees --expected-bps-fee 500 --expected-flat-fee 2000
```

Use `--expected-bps-fee 0 --expected-flat-fee 0` to accept any fees (not recommended).

## Customizing the Strategy

Edit the `run_strategy()` function in `src/main.rs` to implement your own deployment logic. The default strategy:

1. Waits until `DEPLOY_SLOTS_BEFORE_END` slots before round end
2. Checks each deployer's autodeploy_balance
3. Deploys to all 25 squares if balance is sufficient

You might want to customize:
- Which squares to deploy to based on board state
- Different amounts for different deployers
- More sophisticated timing based on expected value
- Support for multiple auth_ids per manager

## Database Schema

The crank uses SQLite to track all autodeploy transactions:

```sql
CREATE TABLE autodeploy_txs (
    id INTEGER PRIMARY KEY,
    signature TEXT NOT NULL UNIQUE,
    manager_key TEXT NOT NULL,
    deployer_key TEXT NOT NULL,
    auth_id INTEGER NOT NULL,
    round_id INTEGER NOT NULL,
    amount_per_square INTEGER NOT NULL,
    squares_mask INTEGER NOT NULL,
    num_squares INTEGER NOT NULL,
    total_deployed INTEGER NOT NULL,
    deployer_fee INTEGER NOT NULL,
    protocol_fee INTEGER NOT NULL,
    priority_fee INTEGER NOT NULL,
    jito_tip INTEGER NOT NULL,
    last_valid_blockheight INTEGER NOT NULL,
    sent_at INTEGER NOT NULL,
    confirmed_at INTEGER,
    finalized_at INTEGER,
    status INTEGER NOT NULL,  -- 0=pending, 1=confirmed, 2=finalized, 3=failed, 4=expired
    error_message TEXT,
    compute_units_consumed INTEGER,
    slot INTEGER
);
```

## Transaction Status Codes

- `0` - Pending: Transaction sent but not yet confirmed
- `1` - Confirmed: Transaction confirmed (processed)
- `2` - Finalized: Transaction finalized
- `3` - Failed: Transaction failed with error
- `4` - Expired: Transaction blockhash expired

## Querying the Database

View recent transactions:
```bash
sqlite3 crank.db "SELECT signature, status, sent_at FROM autodeploy_txs ORDER BY sent_at DESC LIMIT 10"
```

View failed transactions:
```bash
sqlite3 crank.db "SELECT signature, error_message FROM autodeploy_txs WHERE status = 3"
```

Get stats for the last 24 hours:
```bash
sqlite3 crank.db "SELECT COUNT(*), SUM(CASE WHEN status=2 THEN 1 ELSE 0 END) as finalized FROM autodeploy_txs WHERE sent_at > strftime('%s', 'now', '-1 day')"
```
