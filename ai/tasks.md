# Current Tasks

> Last Updated: 2025-11-30

## Active

_None currently active_

---

## Up Next

### Task 10: Add More Unit Tests
**Priority:** 🟠 High

Potential test cases to add:
- `create_manager`: Wrong system program
- `ev_deploy`: End slot exceeded, invalid PDA/bump
- `checkpoint`: Invalid PDA, round not found
- `claim_sol`: Invalid PDA, no rewards
- `claim_ore`: Invalid PDA, no rewards, ATA creation

---

## Backlog

- [ ] Task 11: Remove unused imports in processors
- [ ] Task 12: Add comprehensive error types

---

## Completed

### ✅ Task 9: Refactor Test Setup for Unit Testing (Improved)
**Completed:** 2025-11-30

**Configurable Account Helpers:**
- `add_manager_account(program_test, address, authority)` - Evore Manager
- `add_board_account(program_test, round_id, start_slot, end_slot)` - ORE Board
- `add_round_account(program_test, round_id, deployed, total_deployed, expires_at)` - ORE Round
- `add_ore_miner_account(program_test, authority, deployed, sol, ore, checkpoint_id, round_id)` - ORE Miner
- `add_entropy_var_account(program_test, board_address, end_at)` - Entropy Var

**Snapshot Helpers (for complex external state):**
- `add_treasury_account()`, `add_mint_account()`, `add_treasury_ata_account()`, `add_config_account()`

**Convenience:**
- `setup_deploy_test_accounts()` - Sets up common accounts for deploy tests

**Test Modules Created:**
- `create_manager::test_success`, `create_manager::test_already_initialized`
- `ev_deploy::test_success`, `ev_deploy::test_too_many_slots_left`, `ev_deploy::test_wrong_authority`
- `checkpoint::test_wrong_authority`
- `claim_sol::test_wrong_authority`
- `claim_ore::test_wrong_authority`

---

### ✅ Task 8: Add Writable Account Checks
**Completed:** 2025-11-30

---

### ✅ Task 7: Add Bump Parameter for Deterministic CU Usage
**Completed:** 2025-11-30

---

### ✅ Task 6: Add Program Verifications
**Completed:** 2025-11-30

---

### ✅ Task 5: Fix Rent Drain (CANCELLED)

---

### ✅ Task 4: Add PDA Address Validation (All Remaining Processors)
**Completed:** 2025-11-30

---

### ✅ Task 3: Add PDA Address Validation (process_ev_deploy.rs)
**Completed:** 2025-11-30

---

### ✅ Task 2: Add Fee Collector Address Verification
**Completed:** 2025-11-30

---

### ✅ Task 1: Fix Critical Fee Transfer Bug
**Completed:** 2025-11-30
