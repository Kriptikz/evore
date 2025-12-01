# Current Tasks

> Last Updated: 2025-12-01 (Task 26 complete - Multi-Bot Architecture)

## Active

No active tasks. Phase 11 complete.

---

## Up Next

### Task 27: Wire Up Multi-Bot Dashboard
**Priority:** 🟡 Medium

Integrate the new multi-bot architecture with the dashboard command.

**Subtasks:**
- [ ] Add `--config` flag to dashboard command for TOML config file
- [ ] Replace `run_bot_task` with `RoundCoordinator`
- [ ] Update TUI to handle dynamic bot count
- [ ] Test with multi-bot config file

## Backlog

- Task 27: Frontend UI (web dashboard)
- Add `ClaimOre` CLI command (instruction exists in `mm_claim_ore`, command missing in bot)
- Add inline documentation for all public functions
- Create client SDK documentation
- Enable priority fee when needed (add `--priority-fee` CLI flag)

---

## Completed

### ✅ Task 26: Multi-Bot Architecture Refactor
**Completed:** 2025-12-01

**New modules created:**
- `board_tracker.rs` - WebSocket subscription to Board PDA
- `round_tracker.rs` - WebSocket subscription to Round PDA (auto-switches on round change)
- `blockhash_cache.rs` - Periodic RPC blockhash fetch with adaptive rate
- `tx_pipeline.rs` - TxSender (instant send) + TxConfirmer (batch status check)
- `config.rs` - BotConfig with strategy params, TOML config parsing
- `bot_state.rs` - BotPhase state machine with P&L tracking
- `bot_runner.rs` - Refactored bot using shared services
- `coordinator.rs` - RoundCoordinator for multi-bot orchestration
- `shutdown.rs` - Graceful Ctrl+C handler

**Architecture:**
- All bots share services via Arc (SlotTracker, BoardTracker, RoundTracker, BlockhashCache)
- Each bot runs as independent tokio task
- Coordinator spawns and manages multiple bots from TOML config
- Ready for dashboard integration

---

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
