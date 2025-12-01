# Current Tasks

> Last Updated: 2025-12-01 (Task 25 complete, architecture refactored)

## Active

### Task 26: Multi-Bot Architecture Refactor
**Priority:** 🔴 High

Refactor bot to support multiple parallel bots with shared services and optimized RPC.

See `plan.md` Phase 11 for full architecture diagrams and details.

#### Phase 11a: Shared Services

**Task 26a: BoardTracker**
- Websocket `accountSubscribe` to Board PDA
- Provides: `round_id`, `start_slot`, `end_slot`
- Detects new round started, round ended

**Task 26b: RoundTracker**
- Websocket `accountSubscribe` to current Round PDA
- Provides: `deployed[25]`, `total_deployed`
- Switches subscription when `round_id` changes

**Task 26c: BlockhashCache**
- Periodic RPC fetch (2s normally, 500ms when slots_left < 10)
- Shared via Arc

---

#### Phase 11b: Transaction Pipeline

**Task 26d: TxSender Task**
- Reads from mpsc channel
- Sends instantly (no blocking)
- Queues signature for confirmation

**Task 26e: TxConfirmer Task**
- Collects pending signatures
- Batch `getSignatureStatuses` (up to 256 per call)
- **Fetch transaction error details for failed txs**
- Parse errors into human-readable messages
- Returns `TxResult { signature, status, error, slot_landed }` via oneshot

---

#### Phase 11c: Bot Refactor

**Task 26f: BotConfig Struct**
```rust
struct BotConfig {
    name: String,
    auth_id: u64,
    strategy: DeployStrategy,
    slots_left: u64,
    bankroll: u64,
    strategy_params: StrategyParams,
}
```

**Task 26g: BotState Struct**
```rust
struct BotState {
    config: BotConfig,
    state: BotPhase,  // Idle, Waiting, Deploying, Deployed, Checkpointing
    last_deployed_round: Option<u64>,
    last_checkpointed_round: Option<u64>,
    pending_signatures: Vec<Signature>,
}
```

**Task 26h: Refactor Bot to Use Shared Services**
- Bot receives trackers via Arc
- Bot sends txs via mpsc channel
- Bot receives confirmations via oneshot

---

#### Phase 11d: Multi-Bot Coordination

**Task 26i: RoundCoordinator**
- Holds all bots + shared services
- Main loop checks round lifecycle
- Triggers checkpoint/claim when new round starts
- Triggers deploy when slots_left threshold reached

**Task 26j: Multi-Bot Spawning**
- Load bot configs from file or CLI
- Spawn each bot as async task
- All share same services via Arc

---

**Subtasks Summary:**
- [ ] 26a: BoardTracker (websocket, base64 decode account data)
- [ ] 26b: RoundTracker (websocket, switches on round change)
- [ ] 26c: BlockhashCache (periodic RPC)
- [ ] 26d: TxSender task (instant send)
- [ ] 26e: TxConfirmer task (batch status)
- [ ] 26f: BotConfig struct (with per-bot signer/manager paths)
- [ ] 26g: BotState struct + state machine
- [ ] 26h: Refactor bot to use shared services
- [ ] 26i: RoundCoordinator
- [ ] 26j: Multi-bot spawning from config
- [ ] 26k: Config file parsing (TOML format, see plan.md)
- [ ] 26l: Graceful shutdown (`Ctrl+C` handler)

**Note:** Consider migrating `SlotTracker` from `std::thread::spawn` to `tokio::spawn` for consistency with async architecture (not blocking, current approach works).

---

## Backlog

- Task 27: Frontend UI (web dashboard)
- Add `ClaimOre` CLI command (instruction exists in `mm_claim_ore`, command missing in bot)
- Add inline documentation for all public functions
- Create client SDK documentation
- Enable priority fee when needed (add `--priority-fee` CLI flag)

---

## Completed

### ✅ Task 25: Dashboard TUI
**Completed:** 2025-12-01

**Architecture:**
- Separated UI layer from bot logic using `TuiUpdate` message enum
- Bot runs in separate tokio task, sends updates via mpsc channel
- TUI loop just polls updates and renders (non-blocking)

**Features implemented:**
- Header with round, slot, phase, session time, RPC name, blockhash
- Bot block with:
  - Strategy, bankroll, signer SOL balance
  - Status with countdown (Idle, Waiting, Deploying, Deployed, Checkpointing)
  - This round deployed amount
  - Claimable rewards (SOL + ORE)
  - Session stats with P&L tracking (can go negative)
  - Rounds participated, wins, win rate
- Board grid (5x5) showing round deployment data per square
- Transaction log with timestamps and statuses (Sent, Confirmed, Failed)
- Proper timing logic matching `single_deploy` (wait for slot, spam until end)
- Manager balance updates after claims
- Signer balance monitoring for fee payer

**Files added/modified:**
- `bot/src/bot_task.rs` - New file for bot deployment loop
- `bot/src/tui.rs` - Refactored with TuiUpdate enum and clean architecture
- `bot/src/main.rs` - Spawns bot task, runs TUI loop

---

### ✅ Task 24: Bot - Mainnet Testing & Refinements
**Completed:** 2025-12-01

**Improvements made during testing:**
- Fixed round lifecycle handling (intermission, reset waiting, MAX end_slot)
- Auto-detect checkpoint round from miner account
- Checkpoint command verifies if needed before executing
- Skip preflight + 0 retries for deploy transactions
- Claim SOL only if rewards_sol > 0
- Balance display (signer, managed_miner_auth, miner rewards)
- Live slot countdown while waiting for deploy window
- Start sending 50ms before target slot (configurable)
- Stop sending at end_slot (last deployable is end_slot - 1)
- Continuous deploy calls single_deploy immediately (handles waiting internally)
- Priority fee code ready (disabled for now)
- Bot README with all commands

---

### ✅ Task 23: Bot - Websocket Slot Tracking
**Completed:** 2025-12-01

- Added SlotTracker with websocket subscription
- Real-time slot updates (not 400ms estimates)
- Spam until slot passes end_slot
- Fresh blockhash polling

---

### ✅ Task 22-27: Bot Implementation
**Completed:** 2025-12-01

**Implemented:**
- RPC client with fire-and-forget sending
- Round state fetching (get_board, get_round, get_slot)
- Transaction building (deploy, checkpoint, claim_sol)
- Spam deployment: 10 txs over 1000ms in last 2 slots
- Single deploy command (`deploy`)
- Continuous loop command (`run`) with auto checkpoint & claim SOL
- CLI with subcommands: status, info, deploy, run, checkpoint, claim-sol
- .env support for configuration

---

### ✅ Task 22: Mainnet Deployment
**Completed:** 2025-12-01

- Built program with `cargo build-sbf`
- Deployed to Solana mainnet
- Program ID: `6kJMMw6psY1MjH3T3yK351uw1FL1aE7rF3xKFz4prHb`
- Converted project to Cargo workspace
- Created bot crate structure

---

### ✅ Task 21: Strategy Tests
**Completed:** 2025-12-01

- Added `percentage_deploy` test module (success + edge cases)
- Added `manual_deploy` test module (success + edge cases)

---

### ✅ Task 20: Implement Deployment Strategies
**Completed:** 2025-12-01

- Created `DeployStrategy` enum (EV, Percentage, Manual)
- Implemented `calculate_percentage_deployments` function
- Implemented `calculate_manual_deployments` function
- Refactored to `calculate_ev_deployments` function
- Updated `MMDeploy` struct with strategy discriminant in data array
- Added `ev_deploy`, `percentage_deploy`, `manual_deploy` instruction builders

---

### ✅ Task 16b: Use Custom Errors + Fix Tests
**Completed:** 2025-11-30

---

### ✅ Task 16: Edge Case Tests
**Completed:** 2025-11-30

---

### ✅ Task 11-15: Error Tests
**Completed:** 2025-11-30

---

### ✅ Task 10: Phase 4 - Code Quality
**Completed:** 2025-11-30

---

### ✅ Task 9: Refactor Test Setup
**Completed:** 2025-11-30

---

### ✅ Task 8: Add Writable Account Checks
**Completed:** 2025-11-30

---

### ✅ Task 7: Add Bump Parameter
**Completed:** 2025-11-30

---

### ✅ Task 6: Add Program Verifications
**Completed:** 2025-11-30

---

### ✅ Task 5: Fix Rent Drain (CANCELLED)

---

### ✅ Task 4: Add PDA Address Validation
**Completed:** 2025-11-30

---

### ✅ Task 3: Add PDA Validation (ev_deploy)
**Completed:** 2025-11-30

---

### ✅ Task 2: Add Fee Collector Verification
**Completed:** 2025-11-30

---

### ✅ Task 1: Fix Fee Transfer Bug
**Completed:** 2025-11-30
