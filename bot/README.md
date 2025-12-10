# Evore Bot

Deployment bot for the Evore managed miner program on ORE v3. Features a multi-bot TUI dashboard with real-time monitoring, multiple deployment strategies, play/pause control, and a comprehensive miner management interface.

## Features

- **Multi-Bot Dashboard**: Run multiple bots simultaneously with different strategies
- **Deployment Strategies**: EV-based, Percentage-based, or Manual deployment
- **Play/Pause Control**: Pause individual bots without stopping the dashboard
- **Miner Management TUI**: Manage all your miners across multiple signers with checkpoint/claim actions
- **Legacy Program Support**: Claim rewards from old program versions
- **Real-time Monitoring**: WebSocket slot tracking, RPS metrics, transaction counters
- **Fast Transaction Sending**: Helius endpoints (East/West), Jito tips, auto-retry
- **TOML Configuration**: Define all bots in a single config file
- **Hot Config Reload**: Reload bot parameters without restarting

## Setup

### Environment Variables

Create a `.env` file or set these environment variables:

```bash
RPC_URL=https://your-rpc-url.com
WS_URL=wss://your-websocket-url.com
KEYPAIR_PATH=/path/to/signer/keypair.json
MANAGER_PATH=/path/to/manager/keypair.json
```

- **RPC_URL**: Solana RPC endpoint (HTTP)
- **WS_URL**: WebSocket endpoint for slot subscriptions (optional, derived from RPC_URL)
- **KEYPAIR_PATH**: Signer keypair - pays fees and signs transactions
- **MANAGER_PATH**: Manager keypair - owns the Manager account

### Build

```bash
cd bot && cargo build --release
```

## Commands

### Dashboard (Multi-Bot)

The primary way to run the bot. Uses a TOML config file to define multiple bots:

```bash
cargo run -- dashboard --config app-config.toml
```

**Keyboard Shortcuts:**
| Key | Action |
|-----|--------|
| `↑/↓` or `j/k` | Navigate between bots and elements |
| `Enter` | Execute selected action (pause toggle, etc.) |
| `P` | Toggle pause for selected bot |
| `R` | Reload config for selected bot |
| `S` | Reset session stats |
| `C` | Copy selected value to clipboard |
| `T` | Toggle transaction log view |
| `Q` or `Esc` | Quit dashboard |

**Bot States:**
- ▶️ **Running** - Bot is actively monitoring and deploying
- ⏸️ **Paused** - Bot is paused, no activity
- 🔄 **Loading** - Bot is loading data after unpause

### Manage (Miner Management TUI)

TUI for managing miners across all your signers. Automatically discovers managers/miners and allows executing checkpoint, claim SOL, and claim ORE actions:

```bash
cargo run -- manage --config app-config.toml
```

**Features:**
- Discovers all Manager accounts for each signer keypair
- Finds all Miner accounts associated with each Manager
- Shows claimable SOL and ORE amounts
- Supports legacy program miners (claim-only)
- One-click checkpoint and claim actions

**Keyboard Shortcuts:**
| Key | Action |
|-----|--------|
| `↑/↓` or `j/k` | Navigate miners and actions |
| `Enter` | Execute selected action |
| `R` | Refresh miner data |
| `PageUp/PageDown` | Scroll faster |
| `Q` or `Esc` | Quit |

**Actions:**
- **[✓Chk]** - Checkpoint (update rewards, required before claiming)
- **[💰SOL]** - Claim SOL rewards
- **[⛏ORE]** - Claim ORE rewards

### Other Commands

#### Create Manager

Create a new Manager account (required before deploying):

```bash
cargo run -- create-manager
```

#### Info

Show managed miner auth PDA info:

```bash
cargo run -- info --auth-id 1
```

#### Status

Show current round status:

```bash
cargo run -- status
```

#### Single Deploy

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

#### Continuous Deploy (Run)

Continuous deployment loop with auto checkpoint and auto claim:

```bash
cargo run -- run \
  --bankroll 100000000 \
  --max-per-square 10000000 \
  --slots-left 2 \
  --auth-id 1
```

#### Checkpoint

Checkpoint a round to enable reward claims:

```bash
cargo run -- checkpoint --auth-id 1
# Or specify round manually:
cargo run -- checkpoint --round-id 123 --auth-id 1
```

#### Claim SOL

Claim SOL rewards from managed miner:

```bash
cargo run -- claim-sol --auth-id 1
```

## Configuration

### Multi-Bot Config (app-config.toml)

```toml
# Evore Bot Configuration
# priority_fee: micro-lamports per CU (5000 = ~0.000007 SOL @ 1.4M CU)
# jito_tip: lamports for Jito tip (200_000 = 0.0002 SOL)

[[bots]]
name = "EV Bot"
auth_id = 1
strategy = "ev"
slots_left = 2                  # Start deploying when N slots remain
bankroll = 100_000_000          # 0.1 SOL
attempts = 3                    # Transaction retry attempts
priority_fee = 40_000           # Micro-lamports per CU
jito_tip = 300_000              # Jito tip in lamports
paused_on_startup = false       # Start paused if true
signer_path = "/path/to/signer.json"
manager_path = "/path/to/manager.json"

[bots.strategy_params]
type = "ev"
max_per_square = 20_000_000     # Max per square (0.02 SOL)
min_bet = 10_000                # Minimum bet (0.00001 SOL)
ore_value = 500_000_000         # ORE value for EV calc (0.5 SOL)

[[bots]]
name = "Percentage Bot"
auth_id = 1
strategy = "percentage"
slots_left = 15
bankroll = 50_000_000
attempts = 2
priority_fee = 4_000
jito_tip = 200_000
signer_path = "/path/to/signer2.json"
manager_path = "/path/to/manager2.json"

[bots.strategy_params]
type = "percentage"
percentage = 5000               # 50% (basis points, 10000 = 100%)
squares_count = 25              # Number of squares to fill

[[bots]]
name = "Pure SOL EV"
auth_id = 1
strategy = "ev"
slots_left = 4
bankroll = 150_000_000
attempts = 6
priority_fee = 5_000
jito_tip = 200_000
signer_path = "/path/to/signer3.json"
manager_path = "/path/to/manager3.json"

[bots.strategy_params]
type = "ev"
max_per_square = 30_000_000
min_bet = 10_000
ore_value = 0                   # Set to 0 for pure SOL EV (ignores ORE)

# Manage command configuration
[manage]
signers_path = "/path/to/signers/directory"
# secondary_program_id = "OLD_PROGRAM_ID"  # Optional: for legacy program claims
```

### Bot Configuration Fields

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `name` | Yes | - | Display name for the bot |
| `auth_id` | Yes | - | Auth ID for managed miner |
| `strategy` | Yes | - | `"ev"`, `"percentage"`, or `"manual"` |
| `slots_left` | No | 2 | Start deploying when N slots remain |
| `bankroll` | Yes | - | Total bankroll in lamports |
| `attempts` | No | 4 | Transaction retry attempts |
| `priority_fee` | No | 5000 | Priority fee (micro-lamports/CU) |
| `jito_tip` | No | 200000 | Jito tip in lamports (0 to disable) |
| `paused_on_startup` | No | false | Start bot in paused state |
| `signer_path` | No | default | Path to signer keypair |
| `manager_path` | No | default | Path to manager keypair |

### Deployment Strategies

#### EV (Expected Value)

Calculates optimal bet sizes based on expected value:

```toml
[bots.strategy_params]
type = "ev"
max_per_square = 20_000_000  # Max lamports per square
min_bet = 10_000             # Minimum bet size
ore_value = 500_000_000      # ORE value in lamports for EV calculation
```

- Set `ore_value = 0` for pure SOL EV calculation (ignores ORE rewards)
- Higher `ore_value` = more aggressive betting (expects more ORE value)
- `max_per_square` caps individual square bets

#### Percentage

Fills squares based on percentage of bankroll:

```toml
[bots.strategy_params]
type = "percentage"
percentage = 5000            # 50% of bankroll (in basis points)
squares_count = 25           # Number of squares to fill
```

- `percentage` is in basis points (10000 = 100%)
- Distributes total amount across `squares_count` squares

#### Manual

Fixed bet amount:

```toml
[bots.strategy_params]
type = "manual"
amount = 10_000_000          # Fixed amount per square
```

### Manage Configuration

The `[manage]` section configures the miner management TUI:

```toml
[manage]
signers_path = "/path/to/signers/directory"  # Directory with *.json keypairs
secondary_program_id = "6kJM..."              # Optional: legacy program ID
```

- **signers_path**: Directory containing signer keypair files (*.json)
- **secondary_program_id**: Optional legacy program ID for claim-only operations

**How it works:**
1. Loads all keypair files (*.json) from the signers directory
2. For each signer, discovers Manager accounts (using memcmp filter on authority)
3. For each Manager, discovers Miner accounts (iterating auth_id: 1, 2, 3...)
4. If `secondary_program_id` configured, repeats discovery for legacy program
5. Displays all miners with their claimable amounts and action buttons

**Legacy miners** are displayed with a `LEGACY` label and program ID prefix. They support Claim SOL and Claim ORE only (no Checkpoint).

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

## Example Workflows

### Initial Setup

1. **Create manager account** (one-time per signer/manager pair):
   ```bash
   cargo run -- create-manager
   ```

2. **Check info**:
   ```bash
   cargo run -- info
   ```

### Multi-Bot Operation

1. **Create config file** (`app-config.toml`) with your bots

2. **Run dashboard**:
   ```bash
   cargo run -- dashboard --config app-config.toml
   ```

3. **Monitor and control**:
   - Use `↑/↓` to navigate between bots
   - Press `P` to pause/unpause a bot
   - Press `R` to reload bot config from file
   - Press `T` to view transaction log
   - Press `C` to copy values (pubkeys, etc.)

### Miner Management

1. **Configure signers path** in `[manage]` section of config

2. **Run manage TUI**:
   ```bash
   cargo run -- manage --config app-config.toml
   ```

3. **Execute actions**:
   - Navigate to miner with `↑/↓`
   - Select action (Checkpoint, Claim SOL, Claim ORE)
   - Press `Enter` to execute
   - Press `R` to refresh data after claiming

### Claiming from Legacy Program

1. Add `secondary_program_id` to config:
   ```toml
   [manage]
   signers_path = "/path/to/signers"
   secondary_program_id = "YOUR_OLD_PROGRAM_ID"
   ```

2. Run manage TUI - legacy miners appear with `LEGACY` label

3. Execute Claim SOL or Claim ORE (no checkpoint needed for legacy)

## Dashboard Features

### Bot Status Display

Each bot shows:
- ▶️/⏸️ Play/Pause toggle (selectable)
- Current phase (Waiting, Deploying, Checkpoint, Claiming, etc.)
- Signer balance and manager pubkey
- Miner data (round ID, checkpoint status, claimable rewards)
- Strategy parameters and config
- Session statistics (rounds played, deployed, PnL)

### Bot Phases

| Phase | Description |
|-------|-------------|
| Waiting | Waiting for round end (showing slots left) |
| Deploying | Sending deploy transactions |
| Deployed | Successfully deployed this round |
| Checkpoint | Checkpointing previous round |
| Claiming | Claiming SOL/ORE rewards |
| Paused | Bot is paused |
| Loading | Loading data after unpause |

### Network Monitoring

- WebSocket connection status (🟢 Connected / 🔴 Disconnected)
- RPC connection status
- Requests per second (RPS)
- Transaction counters (sent/confirmed/failed)

### Transaction Log

Toggle with `T` to view recent transaction activity including signatures and results.

## Lamport Conversion

| SOL | Lamports |
|-----|----------|
| 0.00001 | 10,000 |
| 0.0001 | 100,000 |
| 0.001 | 1,000,000 |
| 0.01 | 10,000,000 |
| 0.1 | 100,000,000 |
| 0.5 | 500,000,000 |
| 1.0 | 1,000,000,000 |

## Program Info

- **Program ID**: `6kJMMw6psY1MjH3T3yK351uw1FL1aE7rF3xKFz4prHb`
- **Intermission Slots**: 35 (between round end and reset)
- **Checkpoint Fee**: 10,000 lamports
- **Deploy CU Limit**: 1,400,000

## Round Lifecycle

```
State 1: end_slot == u64::MAX
  → Round reset, waiting for first deployer

State 2: current_slot < end_slot
  → Round active, deployments allowed

State 3: current_slot >= end_slot && (current_slot - end_slot) < 35
  → Intermission, waiting for reset

State 4: current_slot >= end_slot + 35
  → Ready for reset
```

## Architecture

```
bot/
├── src/
│   ├── main.rs             # CLI entry point & command handlers
│   ├── config.rs           # TOML config parsing (BotConfig, ManageConfig)
│   ├── coordinator.rs      # Multi-bot orchestration & lifecycle
│   ├── bot_runner.rs       # Individual bot main loop
│   ├── bot_state.rs        # Bot state machine (phases, pause)
│   ├── bot_task.rs         # Legacy single-bot task
│   ├── tui.rs              # Dashboard TUI (ratatui)
│   ├── manage.rs           # Miner discovery & account fetching
│   ├── manage_tui.rs       # Manage TUI
│   ├── deploy.rs           # Transaction building (deploy, checkpoint, claim)
│   ├── sender.rs           # FastSender (Helius East/West, Jito tips)
│   ├── client.rs           # EvoreClient (RPC wrapper with RPS tracking)
│   ├── ev_calculator.rs    # EV calculation logic
│   ├── slot_tracker.rs     # WebSocket slot subscription
│   ├── blockhash_cache.rs  # Recent blockhash caching
│   ├── board_tracker.rs    # Board state tracking
│   ├── round_tracker.rs    # Round state tracking
│   ├── miner_tracker.rs    # Miner state tracking
│   ├── treasury_tracker.rs # Treasury state tracking
│   ├── tx_pipeline.rs      # Transaction sending pipeline
│   └── shutdown.rs         # Graceful shutdown handling
└── app-config.toml         # Bot configuration file
```

## Transaction Sending

The bot uses `FastSender` for optimized transaction delivery:

- **Helius Endpoints**: Sends to both East and West coast endpoints
- **Jito Tips**: Configurable tips for priority inclusion
- **Auto-Retry**: Automatically retries failed transactions
- **Skip Preflight**: Faster submission by skipping simulation

Configure via bot config:
- `priority_fee`: Compute unit price (micro-lamports per CU)
- `jito_tip`: Jito tip amount in lamports (0 to disable)
- `attempts`: Number of retry attempts
