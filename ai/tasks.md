# Current Tasks

> Last Updated: 2025-11-30

## Active

_None currently active_

---

## Up Next

### Task 6: Add Program Verifications
**Priority:** 🟠 High

**Files & Changes:**

1. **`src/processor/process_ev_deploy.rs`** - Add entropy program check:
```rust
if *entropy_program.key != entropy_api::id() {
    return Err(ProgramError::IncorrectProgramId);
}
```

2. **`src/processor/process_claim_ore.rs`** - Add SPL program checks:
```rust
if *spl_program.key != spl_token::id() {
    return Err(ProgramError::IncorrectProgramId);
}

if *spl_ata_program.key != spl_associated_token_account::id() {
    return Err(ProgramError::IncorrectProgramId);
}
```

---

## Backlog

- [ ] Task 7: Add writable account checks (all processors)
- [ ] Task 8: Remove unused imports
- [ ] Task 9: Add comprehensive error types

---

## Completed

### ✅ Task 5: Fix Rent Drain (CANCELLED)
**Reason:** Not applicable - managed_miner_auth PDA is only used as signing authority, not a persistent account.

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
