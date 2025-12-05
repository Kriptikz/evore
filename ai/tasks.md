# Current Tasks

> Last Updated: 2025-12-05 (Phase 14: Play/Pause, Phase 15: Manage Command)

## Active

### Task 38: Phase 14 - Play/Pause Feature
**Priority:** 🟢 High
**Status:** Not Started (0/3 phases)

Add play/pause functionality to control individual bot activity.

**Phase 14a: Config & State**
- [ ] Add `paused_on_startup: bool` to BotConfig struct
- [ ] Add `Paused` variant to BotPhase enum
- [ ] Add `is_paused: bool` to BotState
- [ ] Parse `paused_on_startup` from TOML config
- [ ] Initialize bot state with paused if configured

**Phase 14b: Bot Runner Updates**
- [ ] Skip all activity in main loop when `is_paused == true`
- [ ] On unpause: trigger data reload (fetch miner, check checkpoint)
- [ ] Add `Loading` state between Paused and normal operation
- [ ] Handle pause during deployment (finish current tx, then pause)

**Phase 14c: TUI Integration**
- [ ] Add ▶️ (play) / ⏸️ (pause) icon to bot block
- [ ] Make icon selectable with cursor
- [ ] Toggle pause on Enter when icon selected
- [ ] Add P hotkey to toggle pause for selected bot
- [ ] Send TuiUpdate::BotPauseToggle event
- [ ] Update bot status display for Paused state

---

### Task 39: Phase 15 - Manage Command (Miner Management TUI)
**Priority:** 🟢 High
**Status:** Not Started (0/5 phases)

New CLI command with TUI for managing mining accounts across multiple signers and programs.

**Phase 15a: Config & Discovery**
- [ ] Add `[manage]` section to config with `signers_path` and `secondary_program_id`
- [ ] Create `manage_config.rs` module for parsing
- [ ] Implement signer keypair loading from directory (glob *.json)
- [ ] Implement `get_managers_by_authority()` with memcmp filter (offset=8)
- [ ] Implement `get_miners_for_manager()` with auth_id loop (1, 2, 3... until not found)
- [ ] Add `old_program_authority_pda()` function for secondary program

**Phase 15b: CLI Command**
- [ ] Add `manage` subcommand to CLI
- [ ] Load manage config section
- [ ] Initialize account discovery
- [ ] Launch manage TUI

**Phase 15c: Manage TUI Layout**
- [ ] Create `manage_tui.rs` module
- [ ] Design miner card widget with selectable actions
- [ ] Grid layout for miner cards (responsive to terminal size)
- [ ] Distinguish legacy miners with program ID marker (start/end chars)
- [ ] Status bar with signer/manager/miner counts

**Phase 15d: Action Execution**
- [ ] Implement checkpoint action (build & send tx)
- [ ] Implement claim_sol action (build & send tx)
- [ ] Implement claim_ore action (build & send tx)
- [ ] Transaction status feedback (spinner, success/fail)
- [ ] Refresh miner data after action completion

**Phase 15e: Legacy Program Support**
- [ ] Parse `secondary_program_id` from config
- [ ] Implement old program account discovery with custom authority_pda
- [ ] Build legacy claim_sol/claim_ore transactions (no checkpoint)
- [ ] Display legacy miners with program ID prefix/suffix

---

## Completed Recently

### Task 37: Phase 12 - Improved Board & Deployment Tracking ✅
**Priority:** 🟢 High
**Status:** Complete (7/7 phases)

Enhanced board visualization, miner tracking, treasury monitoring, and EV display.

**Phase 12a: Bot Icons** ✅
- [x] Create icon pool: 🤖🎯🔥⚡🌟💎🎲🎰🚀🌙🎪🎨🎭🎵🎸
- [x] Unique icon assignment at bot creation (based on bot_index)
- [x] Store icon in BotState
- [x] Show icon in tx log: `[12:34:56] 🎯 bot-1  DEPLOY  OK  5xKj3...`

**Phase 12b: Miner Tracker** ✅
- [x] Create `miner_tracker.rs` module
- [x] Poll each bot's Miner account (managed_miner_auth PDA) every 2 seconds
- [x] Track: `deployed[25]`, `round_id`
- [x] Check if deployed this round: `miner.round_id == board.round_id`
- [x] Add TuiUpdate::MinerDataUpdate variant

**Phase 12c: Per-Bot Board Display** ✅
- [x] Update App state with per-bot deployment arrays (deployed_per_square, miner_round_id)
- [x] Modify draw_board_grid to show bot icons + amounts per square
- [x] Handle multiple bots deploying to same square
- [x] Clear/update on round change (uses miner_round_id == board.round_id)

**Phase 12d: Treasury Tracker** ✅
- [x] Create `treasury_tracker.rs` module
- [x] Poll ORE Treasury (TREASURY_ADDRESS) via RPC every 2 seconds
- [x] Parse fields: `balance`, `motherlode`, `total_unclaimed`, `total_refined`, `total_staked`
- [x] Add TuiUpdate::TreasuryUpdate variant

**Phase 12e: Header Update** ✅
- [x] Add treasury fields to App state
- [x] Display: `Treasury: X◎ | ML: Y ORE` in header
- [x] Format with SOL/ORE units

**Phase 12f: Live SOL EV Board Display** ✅
- [x] Calculate pure SOL EV per square using program's formula (ore_value = 0):
  ```
  EV_sol = x * (891 * L - 24010 * (T + x)) / (25000 * (T + x))
  Optimal stake: x* = sqrt(T * 891 * L / 24010) - T
  ```
- [x] Created `ev_calculator.rs` module with EV calculations
- [x] Per-square display: total deployed, EV indicator (+EV green, -EV red)
- [x] Board totals: +EV square count, total stake, total expected profit
- [x] Bot icons shown for squares where bots deployed
- [x] Color coded: green = +EV, red = -EV
- [x] Real-time updates as round data changes

**Phase 12g: Round Tracker Stability Fix** ✅
- [x] Converted `round_tracker.rs` from WebSocket to RPC polling
- [x] Fixes board `deployed[]` and `total_deployed` jumping values
- [x] Poll Round account every 1 second
- [x] Keep WS for board/slot (fast updates), use RPC polling for Round (stability)

---

### Task 36: TUI Polish & Bug Fixes ✅
**Priority:** 🟢 High
**Completed:** 2025-12-01

- [x] Auth ID displayed next to bot name
- [x] Fixed tx counters for checkpoint/claim
- [x] Deploy transactions now use FastSender
- [x] RPS tracking with timestamp list
- [x] Totals display in footer
- [x] Tx counters: OK/FAIL/MISS categorization

---

### Task 32: TUI Layout & Network Stats ✅
**Priority:** 🟢 High
**Completed:** 2025-12-01

Improve TUI layout with togglable views and add network monitoring footer.

**Layout Changes:**
- [x] Tab key toggles between Board view and Transaction Log view
- [x] Only one view shown at a time (more vertical space for each)
- [x] Visual indicator showing current view mode

**Network Stats Footer:**
- [x] WebSocket connection status (SlotTracker, BoardTracker, RoundTracker)
- [x] RPC connection status
- [x] RPC RPS with total requests
- [x] Sender RPS with total sends
- [x] Ping latency to Helius sender endpoints (East/West)
- [x] Transaction counts: OK/FAIL/MISS with miss rate %

**Implementation:**
- [x] Add NetworkStats struct to track metrics
- [x] Add ConnectionStatus enum for WS/RPC health
- [x] Add ViewMode enum (TxLog, Board)
- [x] Wire up all connection statuses and RPS tracking

---

### Task 31: Resilience & Error Handling ✅
**Priority:** 🔴 Urgent
**Completed:** 2025-12-01

Make the bot resilient for long-running sessions.

**Subtasks:**
- [x] Remove all println!/eprintln! from runtime code
- [x] Add quiet websocket reconnection with exponential backoff
- [x] Ensure RPC errors don't crash or print to stdout
- [x] Add graceful recovery for all error paths

---

### Task 30: Transaction Sender Improvements ✅
**Priority:** 🟢 High
**Completed:** 2025-12-01

**Subtasks:**
- [x] Create FastSender with Helius endpoints (East + West)
- [x] Automatic 4x retry queue (2x East, 2x West per tx)
- [x] Add Jito tip instruction with randomized tip account
- [x] Deploy transactions routed through FastSender
- [x] RPS tracking for sender HTTP requests

---

### Task 29: Config Hot-Reload ✅
**Completed:** 2025-12-01

- [x] BotRunConfig wrapped in Arc<RwLock<>> for runtime updates
- [x] Config reload updates actual deployment values (not just TUI)

---

## Up Next

### Task 38: Phase 14 - Play/Pause Feature
See Active section above.

### Task 39: Phase 15 - Manage Command
See Active section above.

---

## Backlog

### Task 33: Performance & Reliability Improvements
**Priority:** 🟡 Medium

- [ ] Add retry logic for failed checkpoints
- [ ] Timeout tracking for pending transactions

### Task 35: Tracker Account Failsafes
**Priority:** 🟡 Medium

Add fallback RPC polling for tracker data when WebSockets fail.

**Implementation:**
- Create `tracker_failsafe.rs` module
- Periodically fetch accounts via `getMultipleAccounts` RPC call
- Accounts: Board, Round, Miner accounts for each bot
- If WebSocket data stale (>X seconds), use RPC data as backup
- Configurable polling interval (5-10 seconds)

---

### ~~Task 34: Legacy Evore Frontend UI~~
**Priority:** ⚪ Backlog (Deprioritized)

~~- [ ] Dashboard for round monitoring~~
~~- [ ] Manual deployment interface~~
~~- [ ] Wallet connection~~
~~- [ ] Claim interface~~

---

- Add `ClaimOre` CLI command
- Add inline documentation for all public functions
- Create client SDK documentation

---

## Completed

### ✅ Task 28: TUI Interactive Features
**Completed:** 2025-12-01

- [x] Add cursor navigation with arrow keys (↑/↓/j/k)
- [x] Add pubkey display (signer, auth PDA) with shortened format (7...7)
- [x] Add clipboard copy on Enter (pubkeys, tx signatures)
- [x] Show missed rounds for all strategies (not just EV)
- [x] Per-bot SOL cost/spent tracking
- [x] Add config reload icon (🔄) - reload bot config from file on Enter
- [x] Add session refresh icon (🔁) - reset bot session stats on Enter
- [x] Config validation on reload with error indication
- [x] Bot status shows Skipped/Missed/Deployed appropriately

---

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
- `sender.rs` - Helius FastSender with automatic retry queue

---

### ✅ Task 25: Dashboard TUI
**Completed:** 2025-12-01

**Features implemented:**
- Header with round, slot, phase, session time, RPC name, blockhash
- Bot block with strategy, bankroll, signer SOL balance
- Status with countdown (Idle, Waiting, Deploying, Deployed, Skipped, Missed, Checkpointing)
- Session stats with P&L tracking (can go negative)
- Board grid (5x5) showing round deployment data per square
- Transaction log with timestamps and statuses
- Cursor navigation and clipboard copy
- Config reload and session refresh actions

---

### ✅ Task 22-24: Bot Implementation & Mainnet
**Completed:** 2025-12-01

- Program ID: `6kJMMw6psY1MjH3T3yK351uw1FL1aE7rF3xKFz4prHb`
- RPC client with fire-and-forget sending
- WebSocket slot tracking
- Continuous deploy loop with auto checkpoint & claim
- CLI with subcommands: status, info, deploy, run, checkpoint, claim-sol

---

### ✅ Task 20-21: Deployment Strategies
**Completed:** 2025-12-01

- DeployStrategy enum (EV, Percentage, Manual)
- Implemented all three strategy processors
- Strategy tests with edge cases
