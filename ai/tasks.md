# Current Tasks

> Last Updated: 2025-11-30

## Active

_None currently active_

---

## Up Next

### Task 5: Fix Rent Drain in process_claim_sol.rs
**File:** `src/processor/process_claim_sol.rs`  
**Priority:** 🔴 Critical  
**Issue:** Transfers ALL lamports including rent, could close the PDA

Fix: Leave rent-exempt minimum in account or consider if draining is intentional behavior.

---

## Backlog

- [ ] Task 6: Add entropy program check in `process_ev_deploy.rs`
- [ ] Task 7: Add SPL program checks in `process_claim_ore.rs`
- [ ] Task 8: Add writable account checks (all processors)

---

## Completed

### ✅ Task 4: Add PDA Address Validation (All Remaining Processors)
**Files:** 
- `src/processor/process_checkpoint.rs`
- `src/processor/process_claim_sol.rs`
- `src/processor/process_claim_ore.rs`

**Completed:** 2025-11-30

Added to each file after PDA computation:
```rust
if managed_miner_auth_pda.0 != *managed_miner_auth_account_info.key {
    return Err(ProgramError::InvalidSeeds);
}
```

---

### ✅ Task 3: Add PDA Address Validation (process_ev_deploy.rs)
**File:** `src/processor/process_ev_deploy.rs`  
**Completed:** 2025-11-30

---

### ✅ Task 2: Add Fee Collector Address Verification
**File:** `src/processor/process_ev_deploy.rs`  
**Completed:** 2025-11-30

---

### ✅ Task 1: Fix Critical Fee Transfer Bug
**File:** `src/processor/process_ev_deploy.rs`  
**Completed:** 2025-11-30
