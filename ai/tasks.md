# Current Tasks

> Last Updated: 2025-11-30

## Active

_None currently active_

---

## Up Next

### Task 7: Add Bump Parameter for Deterministic CU Usage
**Priority:** 🟠 High  
**Goal:** Make CU consumption deterministic by avoiding `find_program_address` on-chain

**Changes Required:**

#### 1. Update Instruction Structs (`src/instruction.rs`)
Add `bump: u8` field to each instruction that uses PDAs:
- `EvDeploy` - add `bump: u8`
- `MMCheckpoint` - add `bump: u8`
- `MMClaimSOL` - add `bump: u8`
- `MMClaimORE` - add `bump: u8`

#### 2. Update Processors
Replace in each processor:
```rust
// Before (variable CU):
let managed_miner_auth_pda = Pubkey::find_program_address(&seeds, &crate::id());

// After (fixed CU):
let managed_miner_auth_pda = Pubkey::create_program_address(
    &[...seeds, &[args.bump]],
    &crate::id()
)?;
// Then verify it matches the provided account
```

#### 3. Update Instruction Builders (`src/instruction.rs`)
Compute bump client-side using `find_program_address` and pass it to instruction.

#### 4. Update Tests
Use deterministic keypairs so bumps are consistent.

---

## Backlog

- [ ] Task 8: Add writable account checks (all processors)
- [ ] Task 9: Remove unused imports
- [ ] Task 10: Add comprehensive error types

---

## Completed

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
