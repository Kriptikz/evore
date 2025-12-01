# Evore Bot

Deployment bot for the Evore managed miner program on ORE v3.

## Setup

### Environment Variables

Create a `.env` file in the project root or set these environment variables:

```bash
RPC_URL=https://your-rpc-url.com
WS_URL=wss://your-websocket-url.com
KEYPAIR_PATH=/path/to/signer/keypair.json
MANAGER_PATH=/path/to/manager/keypair.json
```

- **KEYPAIR_PATH**: Signer keypair - pays fees and signs transactions
- **MANAGER_PATH**: Manager keypair - owns the Manager account, controls managed miners

### Build

```bash
cd bot && cargo build --release
```

## Commands

### Create Manager

Create a new Manager account (required before deploying):

```bash
cargo run -- create-manager
```

### Info

Show managed miner auth PDA info:

```bash
cargo run -- info --auth-id 1
```

### Status

Show current round status:

```bash
cargo run -- status
```

### Single Deploy

Single EV deployment with transaction spam at round end:

```bash
cargo run -- deploy \
  --bankroll 100000000 \
  --max-per-square 10000000 \
  --min-bet 1000000 \
  --ore-value 500000000 \
  --slots-left 2 \
  --auth-id 1
```

Parameters:
- `--bankroll`: Total lamports to deploy (e.g., 100000000 = 0.1 SOL)
- `--max-per-square`: Maximum lamports per square (default: 0.1 SOL)
- `--min-bet`: Minimum bet size (default: 0.01 SOL)
- `--ore-value`: ORE value in lamports for EV calculation (default: 0.8 SOL)
- `--slots-left`: Deploy when this many slots remain (default: 2)
- `--auth-id`: Managed miner auth ID (default: 1)

### Continuous Deploy (Run)

Continuous deployment loop with auto checkpoint and auto claim:

```bash
cargo run -- run \
  --bankroll 100000000 \
  --max-per-square 10000000 \
  --min-bet 1000000 \
  --ore-value 500000000 \
  --slots-left 2 \
  --auth-id 1
```

### Checkpoint

Checkpoint a round to enable reward claims. Auto-detects the round if not specified:

```bash
cargo run -- checkpoint --auth-id 1
```

Or specify a round manually:

```bash
cargo run -- checkpoint --round-id 123 --auth-id 1
```

The command will:
- Auto-detect the round to checkpoint from the miner account
- Verify if checkpoint is actually needed
- Check if the round has ended before checkpointing

### Claim SOL

Claim SOL rewards from managed miner:

```bash
cargo run -- claim-sol --auth-id 1
```

### Dashboard

Live TUI dashboard with real-time updates:

```bash
cargo run -- dashboard --bankroll 100000000 --auth-id 1
```

Press `q` to quit the dashboard.

## Global Options

These can be provided to any command:

```bash
cargo run -- \
  --rpc-url https://your-rpc.com \
  --ws-url wss://your-ws.com \
  --keypair /path/to/signer.json \
  --manager-path /path/to/manager.json \
  <command>
```

## Example Workflow

1. **Create manager account** (one-time setup):
   ```bash
   cargo run -- create-manager
   ```

2. **Check info**:
   ```bash
   cargo run -- info
   ```

3. **Start continuous deployment**:
   ```bash
   cargo run -- run --bankroll 500000000
   ```

4. **Or run single deployment manually**:
   ```bash
   cargo run -- deploy --bankroll 100000000 --slots-left 2
   ```

5. **Checkpoint and claim if needed**:
   ```bash
   cargo run -- checkpoint --round-id 123
   cargo run -- claim-sol
   ```

## Lamport Conversion

| SOL | Lamports |
|-----|----------|
| 0.001 | 1,000,000 |
| 0.01 | 10,000,000 |
| 0.1 | 100,000,000 |
| 0.5 | 500,000,000 |
| 1.0 | 1,000,000,000 |

