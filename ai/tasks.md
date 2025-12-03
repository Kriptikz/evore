# Current Tasks

> Last Updated: 2025-12-01 (Resilience complete, TUI Layout improvements in progress)

## Active

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
- [x] Requests per second (RPS)
- [x] Ping latency to Helius sender endpoints (East/West)
- [x] Transactions: missed vs total count with miss rate %

**Implementation:**
- [x] Add NetworkStats struct to track metrics
- [x] Add ConnectionStatus enum for WS/RPC health
- [x] Add ViewMode enum (TxLog, Board)
- [x] Add TuiUpdate variants for network stats
- [x] Update TUI to render footer with all stats
- [x] Wire up connection status from SlotTracker, BoardTracker, RoundTracker
- [x] Wire up RPC connection status
- [x] Wire up ping stats from sender (East/West latency)
- [x] Track tx counters (sent/confirmed/failed) from TxEventTyped logs

---

### Task 31: Resilience & Error Handling ✅
**Priority:** 🔴 Urgent
**Completed:** 2025-12-01

Make the bot resilient for long-running sessions. No println/eprintln that mess up TUI, quiet retries for all connections.

**Subtasks:**
- [x] Remove all println!/eprintln! from runtime code (replace with TUI status or silent handling)
- [x] Add quiet websocket reconnection with exponential backoff for:
  - SlotTracker
  - BoardTracker
  - RoundTracker
- [x] Ensure RPC errors don't crash or print to stdout
- [x] Add graceful recovery for all error paths (no unexpected halts)
- [ ] Test 24+ hour runtime stability

---

### Task 30: Transaction Sender Improvements ✅
**Priority:** 🟢 High
**Completed:** 2025-12-01

Improved transaction sending for better landing rates.

**Subtasks:**
- [x] Create FastSender with Helius endpoint (http://ewr-sender.helius-rpc.com/fast)
- [x] Add automatic retry queue (4 sends per transaction)
- [x] Use both East (Newark) and West (Salt Lake City) endpoints
- [x] Alternate sends: even → East, odd → West
- [x] Add Jito tip instruction to all deploy transactions
- [x] Randomize tip account per transaction build
- [x] Add jito_tip and priority_fee to config and TUI display

---

### Task 29: Config Hot-Reload ✅
**Priority:** 🟢 High
**Completed:** 2025-12-01

Allow runtime config updates without restarting bots.

**Subtasks:**
- [x] Wrap BotRunConfig in Arc<RwLock<>> for shared access
- [x] Add update_bot_config method to RoundCoordinator
- [x] Config reload updates actual deployment values (not just TUI)
- [x] Update bankroll, slots_left, priority_fee, jito_tip, strategy_params

---

## Up Next

### Task 33: Performance & Reliability Improvements
**Priority:** 🟡 Medium

- [ ] Add retry logic for failed checkpoints
- [ ] Timeout tracking for pending transactions

## Backlog

### Task 35: Tracker Account Failsafes
**Priority:** 🟡 Medium

Add fallback RPC polling for tracker data to ensure reliability when WebSockets fail.

**Implementation:**
- Create `tracker_failsafe.rs` module
- Periodically fetch all necessary accounts in one `getMultipleAccounts` RPC call
- Accounts to fetch: Board, Round, Miner accounts for each bot
- If WebSocket data is stale (>X seconds), use RPC-fetched data instead
- Update trackers with fresh data from RPC as backup
- Configurable polling interval (e.g., every 5-10 seconds)

---

- Task 34: Frontend UI (web dashboard)
- Add `ClaimOre` CLI command (instruction exists in `mm_claim_ore`, command missing in bot)
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
