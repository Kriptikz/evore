# Current Tasks

> Last Updated: 2025-11-30

## Active

_None currently active_

---

## Up Next

### Task 8: Add Writable Account Checks
**Priority:** 🟠 High

**Files & Changes:**

1. **`src/processor/process_claim_sol.rs`** - Add writable check for `managed_miner_auth_account_info`
2. **`src/processor/process_claim_ore.rs`** - Add writable checks for mutable accounts
3. **`src/processor/process_checkpoint.rs`** - Add writable check for `managed_miner_auth_account_info`

---

## Backlog

- [ ] Task 9: Remove unused imports
- [ ] Task 10: Add comprehensive error types
- [ ] Task 11: Update tests with deterministic keypairs

---

## Completed

### ✅ Task 7: Add Bump Parameter for Deterministic CU Usage
**Completed:** 2025-11-30

**Files modified:**
- `src/instruction.rs` - Added `bump: u8` to EvDeploy, MMCheckpoint, MMClaimSOL, MMClaimORE
- `src/processor/process_ev_deploy.rs` - Use `create_program_address` with args.bump
- `src/processor/process_checkpoint.rs` - Use `create_program_address` with args.bump
- `src/processor/process_claim_sol.rs` - Use `create_program_address` with args.bump
- `src/processor/process_claim_ore.rs` - Use `create_program_address` with args.bump

**Pattern:** Client computes bump via `find_program_address`, passes it in instruction data. On-chain uses `create_program_address` (O(1) CU) instead of `find_program_address` (O(n) CU).

---

### ✅ Task 6: Add Program Verifications
**Completed:** 2025-11-30

---

### ✅ Task 5: Fix Rent Drain (CANCELLED)
**Reason:** Not applicable - PDA only used as signing authority.

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
