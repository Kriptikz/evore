# Current Tasks

> Last Updated: 2025-12-01

## Active

### Task 24: Bot - Testing on Mainnet
**Priority:** 🔴 High

**Test checklist:**
- [ ] `status` - View current round
- [ ] `info` - Display managed_miner_auth PDA
- [ ] `deploy` - Single EV deployment with spam
- [ ] `run` - Continuous loop with auto checkpoint/claim
- [ ] `checkpoint` - Manual checkpoint
- [ ] `claim-sol` - Manual SOL claim

**Setup:**
```bash
# .env in bot/ directory
RPC_URL=https://your-rpc.com
WS_URL=wss://your-rpc.com  # Optional
KEYPAIR_PATH=/path/to/signer.json
MANAGER_PATH=/path/to/manager.json
```

---

## Up Next

- Task 25: Frontend UI

---

## Backlog

_None_

---

## Completed

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
