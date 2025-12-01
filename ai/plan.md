# Evore Development Plan

> Last Updated: 2025-12-01

## Phase 1: Security Fixes (Critical)
> Priority: **IMMEDIATE** - Must complete before any deployment

- [x] Fix fee transfer bug in `process_ev_deploy.rs` (transfers `total_deployed` instead of `fee_amount`)
- [x] Add fee collector address verification in `process_ev_deploy.rs`
- [x] Add PDA address validation in `process_ev_deploy.rs`
- [x] Add PDA address validation in remaining processors (checkpoint, claim_sol, claim_ore)

## Phase 2: Security Hardening (High)
> Priority: **HIGH** - Complete before mainnet

- [x] Add program verifications (entropy, SPL token, SPL ATA)
- [x] Add writable checks for mutable accounts in `process_claim_sol.rs`
- [x] Add writable checks for mutable accounts in `process_claim_ore.rs`
- [x] Add writable checks for mutable accounts in `process_checkpoint.rs`

## Phase 3: Optimization (High - CU Determinism)
> Priority: **HIGH** - Required for predictable CU usage

- [x] Add `bump` parameter to all instruction structs
- [x] Replace `find_program_address` with `create_program_address` + bump verification
- [x] Update instruction builders to accept/compute bump client-side
- [x] Refactor tests for modular unit testing

## Phase 4: Code Quality (Medium)
> Priority: **MEDIUM** - Good practice improvements

- [x] Remove unused imports (`EvDeploy`, `MMClaimSOL`, `size_of`)
- [x] Document magic numbers in EV calculation (NUM, DEN24, C_LAM)
- [x] Add comprehensive error types for each failure mode
- [x] Replace unsafe casts with safe conversions (`From`, `.min()` + cast, `.clamp()`)
- [x] Simplify `calculate_deployments` function signature (u64 instead of u128 params)
- [x] Simplify fee calculation (avoid unnecessary widening/narrowing)

## Phase 5: Testing (High)
> Priority: **HIGH** - Validate fixes and prevent regressions

- [x] Refactor test infrastructure for unit testing
- [x] Add unit tests for CreateManager instruction
- [x] Add unit tests for EvDeploy instruction
- [x] Add security-focused tests (wrong authority)
- [x] Add tests for all error types
- [x] Add edge case tests

## Phase 6: Documentation (Medium)
> Priority: **MEDIUM** - For maintainability

- [x] Create security audit document
- [x] Create program architecture documentation
- [x] Document EV calculation constants
- [x] Create bot README with commands
- [ ] Add inline documentation for all public functions
- [ ] Create client SDK documentation

## Phase 7: Deployment Strategies
> Priority: **HIGH** - Multiple strategy options for deploy instruction

- [x] Create `DeployStrategy` enum (EV, Percentage, Manual)
- [x] Implement percentage-based deployment processor
- [x] Implement manual deployment processor
- [x] Refactor current EV logic into strategy pattern
- [x] Add strategy selection to instruction
- [x] Add tests for each strategy
- [x] Update instruction builders

## Phase 8: Mainnet Deployment
> Priority: **HIGH** - Production deployment

- [x] Mainnet deployment
- [x] Convert to Cargo workspace
- [x] Create bot crate structure

## Phase 9: Evore Bot v1 ✅
> Priority: **HIGH** - Basic automated deployment bot

- [x] Project setup (Cargo workspace, .env support)
- [x] RPC client (skip preflight, 0 retries)
- [x] Websocket slot tracking (real-time slot updates)
- [x] Round state fetching (get_board, get_round, get_miner)
- [x] Transaction building (deploy, checkpoint, claim_sol)
- [x] Single deploy with spam mode + countdown display
- [x] Continuous deploy loop with auto checkpoint & claim SOL
- [x] CLI with subcommands
- [x] Manager keypair loading (separate from signer)
- [x] Balance display and round lifecycle handling
- [x] Priority fee code ready (disabled for now)

## Phase 10: Dashboard TUI
> Priority: **HIGH** - Live monitoring dashboard

### Overview
Ratatui-based terminal UI for real-time monitoring of rounds, deployments, and bot status.

### Layout Design

```
┌─────────────────────────────────────────────────────────────────────────┐
│                              HEADER                                      │
│  Round: 1234  │  Slot: 345678901 / 345679000  │  Slots Left: 99         │
│  Phase: Active  │  Session: 2h 34m  │  RPC: helius                       │
└─────────────────────────────────────────────────────────────────────────┘

┌───────────────────────────────────┐  ┌───────────────────────────────────┐
│  📊 Bot 1 (auth_id=1)             │  │  📐 Bot 2 (auth_id=2)             │
│  Strategy: EV                     │  │  Strategy: Percentage             │
│  Bankroll: 0.5 SOL                │  │  Bankroll: 1.0 SOL                │
│  Status: ⏳ Waiting (87 slots)    │  │  Status: ✅ Deployed              │
│  This Round: 0.15 SOL deployed    │  │  This Round: 0.22 SOL deployed    │
│  Rewards: 0.023 SOL | 1.2 ORE     │  │  Rewards: 0.041 SOL | 2.5 ORE     │
│  ─────── Session Stats ───────    │  │  ─────── Session Stats ───────    │
│  Running: 1h 22m                  │  │  Running: 2h 34m                  │
│  Rounds: 47  │  Wins: 23 (49%)    │  │  Rounds: 52  │  Wins: 31 (60%)    │
│  Earned: +0.234 SOL | +1.5 ORE    │  │  Earned: +0.567 SOL | +3.2 ORE    │
└───────────────────────────────────┘  └───────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────┐
│                            BOARD (5x5)                                   │
│  Total = Round account. Each bot shows icon + their deployed amount.    │
├─────────────┬─────────────┬─────────────┬─────────────┬─────────────────┤
│  0: 1.234   │  1: 0.567   │  2: 2.100   │  3: 0.890   │  4: 1.456       │
│  📊 0.05    │             │  📐 0.10    │             │  📊 0.08        │
│             │             │             │             │  📐 0.07        │
├─────────────┼─────────────┼─────────────┼─────────────┼─────────────────┤
│  5: 0.321   │  6: 1.789   │  ...        │             │                 │
│             │  📊 0.08    │             │             │                 │
├─────────────┼─────────────┼─────────────┼─────────────┼─────────────────┤
│             │             │             │             │                 │
└─────────────┴─────────────┴─────────────┴─────────────┴─────────────────┘

┌─────────────────────────────────────────────────────────────────────────┐
│                         TRANSACTION LOG                                  │
├─────────────────────────────────────────────────────────────────────────┤
│  [12:34:56] 🤖 SENT  5xKj3...  slot=345678950                           │
│  [12:34:56] 🎯 SENT  7mNp2...  slot=345678950                           │
│  [12:34:57] 🤖 ✅    5xKj3...  CONFIRMED                                │
│  [12:34:57] 🎯 ❌    7mNp2...  EndSlotExceeded (slot was 345679001)     │
│  [12:34:58] 🤖 ❌    9qRs1...  NoDeployments (all squares -EV)          │
└─────────────────────────────────────────────────────────────────────────┘
```

### Features

**Header Section:**
- [ ] Round ID, current slot, end slot, slots remaining
- [ ] Round phase (Active, Intermission, Waiting Reset, Waiting Start)
- [ ] Session duration (how long dashboard has been running)
- [ ] RPC endpoint name

**Bot Blocks:**
- [ ] Strategy-based icons with uniqueness:
  - EV: 📊 📈 💹 🎰 🎲 (chart/gambling themed)
  - Percentage: 📐 🔢 🎯 ％ (math themed)
  - Manual: ✋ 🎮 🕹️ 👆 (hand/control themed)
  - Multiple bots same strategy: add number suffix (📊₁ 📊₂)
- [ ] Auth ID and strategy type
- [ ] Bankroll amount
- [ ] Current status with countdown (Waiting, Deploying, Deployed, Checkpointing)
- [ ] **This round: total_deployed amount**
- [ ] **Claimable rewards: SOL and ORE**
- [ ] **Session stats** (in-memory, resets on restart):
  - Time running
  - Rounds participated
  - Wins and win rate (%)
  - SOL + ORE earned this session

**Board Section:**
- [ ] 5x5 grid showing all 25 squares
- [ ] Total deployed per square (from Round account)
- [ ] Each bot's deployment shown separately: icon + amount
- [ ] Multiple bots on same square: each on own line with their amount
- [ ] Color coding (high deployment = brighter)

**Transaction Log:**
- [ ] Scrollable log of recent transactions
- [ ] Shows: timestamp, bot icon, action (SENT/CONFIRMED/FAILED)
- [ ] Signature (truncated)
- [ ] **Error details for failed txs** (fetched from RPC)

### Session Statistics (In-Memory)

Track per session without extra RPC calls. Stored in RAM, resets on restart.

```rust
struct SessionStats {
    started_at: Instant,
}

struct BotSessionStats {
    started_at: Instant,
    rounds_participated: u64,
    rounds_won: u64,           // Won = checkpoint showed rewards > 0
    sol_earned: u64,           // Cumulative SOL earned this session
    ore_earned: u64,           // Cumulative ORE earned this session
    last_rewards_sol: u64,     // rewards_sol before last checkpoint
    last_rewards_ore: u64,     // rewards_ore before last checkpoint
    // Derived:
    // - running_time = Instant::now() - started_at
    // - win_rate = rounds_won / rounds_participated * 100
}
```

**When to update:**
- `rounds_participated += 1` when bot successfully deploys
- Before checkpoint: store `last_rewards_sol` and `last_rewards_ore` from miner account
- After checkpoint: 
  - `new_rewards = miner.rewards_sol - last_rewards_sol` (delta)
  - `sol_earned += new_rewards` if positive
  - `ore_earned += (miner.rewards_ore - last_rewards_ore)` if positive
  - `rounds_won += 1` if either increased

### Transaction Error Inspection

When a transaction fails, fetch the actual error:

```rust
// After sending, queue signature for confirmation
// TxConfirmer checks status and fetches error if failed

struct TxResult {
    signature: Signature,
    status: TxStatus,  // Confirmed, Failed, Timeout
    error: Option<TransactionError>,  // Actual error from chain
    slot_landed: Option<u64>,  // What slot it landed in (if any)
}

// Common errors to display:
// - "EndSlotExceeded" - Transaction landed after round ended
// - "TooManySlotsLeft" - Transaction landed too early  
// - "NoDeployments" - EV calculation found no profitable squares
// - "InsufficientFunds" - Not enough SOL
// - "Custom(0x1)" -> "NotAuthorized"
// - etc.
```

### Implementation Tasks
- [ ] Fix and verify existing dashboard code
- [ ] Implement header section with live updates
- [ ] Implement bot blocks (dynamic based on config)
- [ ] Implement 5x5 board grid with deployment overlay
- [ ] Implement transaction log with scrolling
- [ ] Add error fetching for failed transactions
- [ ] Parse and display human-readable error messages
- [ ] Add keyboard shortcuts (q=quit, tab=switch focus, etc.)

## Phase 11: Multi-Bot Architecture
> Priority: **HIGH** - Parallel bots with optimized RPC

### Overview
Refactor to support multiple bots running in parallel with different auth_ids and strategies, while minimizing RPC calls.

### Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Shared Services                              │
├──────────────┬──────────────┬───────────────┬───────────────────────┤
│ SlotTracker  │ BoardTracker │ RoundTracker  │ BlockhashCache        │
│ (WS slot)    │ (WS account) │ (WS account)  │ (periodic RPC)        │
└──────────────┴──────────────┴───────────────┴───────────────────────┘
                                    │
              ┌─────────────────────┼─────────────────────┐
              ▼                     ▼                     ▼
        ┌──────────┐          ┌──────────┐          ┌──────────┐
        │  Bot 1   │          │  Bot 2   │          │  Bot 3   │
        │ auth_id=1│          │ auth_id=2│          │ auth_id=3│
        │ EV strat │          │ % strat  │          │ Manual   │
        │ state:   │          │ state:   │          │ state:   │
        │ Waiting  │          │ Deployed │          │ Waiting  │
        └────┬─────┘          └────┬─────┘          └────┬─────┘
             │                     │                     │
             └─────────────────────┼─────────────────────┘
                                   ▼
                          ┌─────────────────┐
                          │   TX Channel    │
                          │ (mpsc sender)   │
                          └────────┬────────┘
                                   ▼
                          ┌─────────────────┐
                          │   TX Sender     │◄─── Reads instantly, no blocking
                          │   (async task)  │
                          └────────┬────────┘
                                   ▼
                          ┌─────────────────┐
                          │  TX Confirmer   │◄─── Batch getSignatureStatuses
                          │  (async task)   │     Returns results via oneshot
                          └─────────────────┘
```

---

### Shared Services (Detail)

#### 1. SlotTracker (existing)
- Websocket subscription to slot updates
- `get_slot() -> u64`
- All bots read from same Arc<SlotTracker>

#### 2. BoardTracker (new)
- Websocket `accountSubscribe` to Board PDA
- Provides: `round_id`, `start_slot`, `end_slot`
- Detects: new round started, round ended
- Events: `BoardUpdated { round_id, start_slot, end_slot }`

#### 3. RoundTracker (new)  
- Websocket `accountSubscribe` to current Round PDA
- Provides: `deployed[25]`, `total_deployed`, `motherlode`
- Updates whenever anyone deploys
- Switches subscription when `round_id` changes

#### 4. BlockhashCache (new)
- Periodic RPC fetch (every 2 seconds normally)
- Fast refresh in deploy window (every 500ms when slots_left < 10)
- `get_blockhash() -> Hash`

---

### BotConfig Struct

```rust
struct BotConfig {
    /// Unique name for logging
    name: String,
    
    /// Auth ID for this bot's managed miner
    auth_id: u64,
    
    /// Deployment strategy
    strategy: DeployStrategy,
    
    /// When to start deploying (slots before end)
    slots_left: u64,
    
    /// Bankroll for this bot
    bankroll: u64,
    
    /// Strategy-specific params
    strategy_params: StrategyParams,
}

enum StrategyParams {
    EV { max_per_square: u64, min_bet: u64, ore_value: u64 },
    Percentage { percentage: u64, squares_count: u64 },
    Manual { amounts: [u64; 25] },
}
```

---

### Bot State Machine

Each bot maintains its own state for the current round:

```
                    ┌─────────────────┐
                    │   Idle          │◄─── Round not active (end_slot=MAX)
                    └────────┬────────┘
                             │ Round started (end_slot set)
                             ▼
                    ┌─────────────────┐
                    │   Waiting       │◄─── Waiting for deploy window
                    │                 │     (slots_left > threshold)
                    └────────┬────────┘
                             │ Deploy window reached
                             ▼
                    ┌─────────────────┐
                    │   Deploying     │◄─── Spamming transactions
                    │                 │     (slots_left <= threshold)
                    └────────┬────────┘
                             │ Round ended (slot >= end_slot)
                             ▼
                    ┌─────────────────┐
                    │   Deployed      │◄─── Waiting for next round
                    │                 │     (need to checkpoint this round)
                    └────────┬────────┘
                             │ New round started
                             ▼
                    ┌─────────────────┐
                    │  Checkpointing  │◄─── Checkpoint previous round
                    │                 │     Claim rewards if any
                    └────────┬────────┘
                             │ Done
                             ▼
                         (back to Waiting)
```

**Per-bot tracking:**
```rust
struct BotState {
    config: BotConfig,
    current_round_id: u64,
    state: BotPhase,  // Idle, Waiting, Deploying, Deployed, Checkpointing
    last_deployed_round: Option<u64>,
    last_checkpointed_round: Option<u64>,
    pending_signatures: Vec<Signature>,
}
```

---

### Round Lifecycle Coordination

**RoundCoordinator** - Orchestrates all bots based on shared state:

```rust
struct RoundCoordinator {
    bots: Vec<Bot>,
    slot_tracker: Arc<SlotTracker>,
    board_tracker: Arc<BoardTracker>,
    round_tracker: Arc<RoundTracker>,
    blockhash_cache: Arc<BlockhashCache>,
    tx_sender: mpsc::Sender<TxRequest>,
}
```

**Main loop logic:**
```
loop {
    let slot = slot_tracker.get_slot();
    let board = board_tracker.get_board();
    
    // Handle round lifecycle states
    if board.end_slot == u64::MAX {
        // All bots: Idle state
        continue;
    }
    
    if slot >= board.end_slot {
        // Round ended - all bots in Deployed state
        // Wait for new round
        continue;
    }
    
    let slots_left = board.end_slot - slot;
    
    // New round detected?
    if board.round_id > last_round_id {
        // Trigger checkpointing for bots that deployed last round
        for bot in &mut bots {
            if bot.needs_checkpoint() {
                bot.start_checkpoint();
            }
        }
    }
    
    // For each bot, check if it should deploy
    for bot in &mut bots {
        if bot.state == Waiting && slots_left <= bot.config.slots_left {
            bot.start_deploying(&round_tracker, &tx_sender);
        }
    }
}
```

---

### Transaction Pipeline (Detail)

#### TxRequest
```rust
struct TxRequest {
    transaction: Transaction,
    response_tx: oneshot::Sender<TxResult>,
}

struct TxResult {
    signature: Signature,
    confirmed: bool,
    error: Option<String>,
}
```

#### TxSender Task
```rust
async fn tx_sender_task(
    mut rx: mpsc::Receiver<TxRequest>,
    rpc: RpcClient,
    pending_tx: mpsc::Sender<PendingSig>,
) {
    while let Some(req) = rx.recv().await {
        // Send immediately, no waiting
        match rpc.send_transaction_no_wait(&req.transaction) {
            Ok(sig) => {
                // Queue for confirmation
                pending_tx.send(PendingSig { sig, response_tx: req.response_tx });
            }
            Err(e) => {
                req.response_tx.send(TxResult { error: Some(e) });
            }
        }
    }
}
```

#### TxConfirmer Task
```rust
async fn tx_confirmer_task(
    mut rx: mpsc::Receiver<PendingSig>,
    rpc: RpcClient,
) {
    let mut pending: Vec<PendingSig> = vec![];
    
    loop {
        // Collect pending signatures
        while let Ok(sig) = rx.try_recv() {
            pending.push(sig);
        }
        
        if pending.is_empty() {
            sleep(100ms).await;
            continue;
        }
        
        // Batch check status (up to 256 signatures per call)
        let sigs: Vec<Signature> = pending.iter().map(|p| p.sig).collect();
        let statuses = rpc.get_signature_statuses(&sigs);
        
        // Send results back
        for (i, status) in statuses.iter().enumerate() {
            if status.is_some() {
                let p = pending.remove(i);
                p.response_tx.send(TxResult { confirmed: true, ... });
            }
        }
        
        sleep(500ms).await;  // Check every 500ms
    }
}
```

---

### Implementation Tasks (Revised)

**Phase 11a: Shared Services**
- [ ] Create `BoardTracker` (websocket accountSubscribe to Board PDA)
- [ ] Create `RoundTracker` (websocket accountSubscribe to Round PDA, switches on round change)
- [ ] Create `BlockhashCache` (periodic RPC, fast refresh in deploy window)
- [ ] Wrap all trackers in Arc for sharing

**Phase 11b: Transaction Pipeline**
- [ ] Define `TxRequest`, `TxResult`, `PendingSig` structs
- [ ] Create `TxSender` async task
- [ ] Create `TxConfirmer` async task with batch status checking
- [ ] Create mpsc channels for pipeline

**Phase 11c: Bot Refactor**
- [ ] Define `BotConfig` struct
- [ ] Define `BotState` struct with state machine
- [ ] Refactor single bot to use shared services
- [ ] Bot receives trackers via Arc, sends txs via channel

**Phase 11d: Multi-Bot Coordination**
- [ ] Create `RoundCoordinator` struct
- [ ] Implement round lifecycle detection (new round, round end)
- [ ] Implement per-bot checkpoint/claim scheduling
- [ ] Spawn multiple bots from config file/CLI
- [ ] Coordinate deploy timing across bots

## Phase 12: Frontend UI
> Priority: **LOW** - Future

- [ ] Dashboard for round monitoring
- [ ] Manual deployment interface
- [ ] Wallet connection
- [ ] Claim interface

---

## Progress Tracking

| Phase | Status | Completion |
|-------|--------|------------|
| Phase 1: Security Fixes | ✅ Complete | 100% (4/4) |
| Phase 2: Security Hardening | ✅ Complete | 100% (4/4) |
| Phase 3: Optimization | ✅ Complete | 100% (4/4) |
| Phase 4: Code Quality | ✅ Complete | 100% (6/6) |
| Phase 5: Testing | ✅ Complete | 100% (6/6) |
| Phase 6: Documentation | 🟡 In Progress | 67% (4/6) |
| Phase 7: Strategies | ✅ Complete | 100% (7/7) |
| Phase 8: Mainnet Deployment | ✅ Complete | 100% (3/3) |
| Phase 9: Evore Bot v1 | ✅ Complete | 100% (11/11) |
| Phase 10: Dashboard TUI | 🟡 In Progress | 0% (0/6) |
| Phase 11: Multi-Bot Architecture | 🔴 Not Started | 0% (0/9) |
| Phase 12: Frontend UI | 🔴 Not Started | 0% |

---

## Notes

- Phases 1-9 complete! Program deployed to mainnet, basic bot operational.
- Program ID: `6kJMMw6psY1MjH3T3yK351uw1FL1aE7rF3xKFz4prHb`
- 27+ unit tests with comprehensive coverage
- Workspace structure: `program/` (Solana program), `bot/` (deployment bot)
- Next: Dashboard TUI, then multi-bot architecture refactor
