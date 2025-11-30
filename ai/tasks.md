# Current Tasks

> Last Updated: 2025-11-30

## Active

_None currently active_

---

## Up Next

### Task 2: Add Fee Collector Address Verification
**File:** `src/processor/process_ev_deploy.rs`  
**Priority:** 🔴 Critical  
**Issue:** Fee collector account is not validated, allowing fee theft

Add verification after line ~72 (after system_program check):
```rust
if *fee_collector_account_info.key != crate::consts::FEE_COLLECTOR {
    return Err(ProgramError::InvalidAccountData);
}
```

---

### Task 3: Add PDA Address Validation (process_ev_deploy.rs)
**File:** `src/processor/process_ev_deploy.rs`  
**Priority:** 🔴 Critical  

Add after PDA computation (~line 92):
```rust
if managed_miner_auth_pda.0 != *managed_miner_auth_account_info.key {
    return Err(ProgramError::InvalidSeeds);
}
```

---

## Backlog

- [ ] Task 4: Add PDA validation in `process_checkpoint.rs`
- [ ] Task 5: Add PDA validation in `process_claim_sol.rs`
- [ ] Task 6: Add PDA validation in `process_claim_ore.rs`
- [ ] Task 7: Fix rent drain in `process_claim_sol.rs`
- [ ] Task 8: Add entropy program check
- [ ] Task 9: Add SPL program checks
- [ ] Task 10: Add writable account checks

---

## Completed

### ✅ Task 1: Fix Critical Fee Transfer Bug
**File:** `src/processor/process_ev_deploy.rs`  
**Line:** 131  
**Completed:** 2025-11-30

Changed `total_deployed` to `fee_amount` in the fee transfer:
```rust
// Before (BUG):
total_deployed,

// After (FIXED):
fee_amount,
```
