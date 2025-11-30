# Current Tasks

> Last Updated: 2025-11-30

## Active

_None currently active_

---

## Up Next

### Task 17: Add Inline Documentation
**Priority:** 🟡 Medium

- Document public functions in `instruction.rs`
- Document processor functions
- Document state structs and fields

---

## Backlog

- [ ] Task 18: Create client SDK documentation
- [ ] Task 19: Deployment guide

---

## Completed

### ✅ Task 16b: Use Custom Errors + Add Error Tests
**Completed:** 2025-11-30

**Changes:**

Updated code to use all custom `EvoreError` variants:
- `ManagerNotInitialized` - used in all 4 processors
- `InvalidPDA` - used in all 4 processors
- `InvalidFeeCollector` - used in ev_deploy
- `NoDeployments` - used in ev_deploy
- `ArithmeticOverflow` - used in ev_deploy

**New tests:**
- `test_no_profitable_deployments` - EV calc finds no profitable squares

---

### ✅ Task 16: Edge Case Tests
**Completed:** 2025-11-30

**Tests added:**

**EvDeploy:**
- `test_zero_bankroll` - Deploy with 0 bankroll (NoDeployments error)
- `test_no_profitable_deployments` - EV is negative
- `test_invalid_round_id` - Deploy with non-existent round

**ClaimSOL:**
- `test_no_rewards` - Claim with zero SOL rewards

**ClaimORE:**
- `test_no_rewards` - Claim with zero ORE rewards

---

### ✅ Task 11-15: Error Tests
**Completed:** 2025-11-30

---

### ✅ Task 10: Phase 4 - Code Quality (Complete)
**Completed:** 2025-11-30

---

### ✅ Task 9: Refactor Test Setup for Unit Testing
**Completed:** 2025-11-30

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
