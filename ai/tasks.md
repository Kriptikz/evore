# Current Tasks

> Last Updated: 2025-11-30

## Active

_None currently active_

---

## Up Next

### Task 11: Add More Edge Case Tests
**Priority:** 🟡 Medium

- Test with zero bankroll
- Test with invalid round_id
- Test checkpoint before round ends
- Test claim with no rewards

---

## Backlog

- [ ] Task 12: Add inline documentation for public functions
- [ ] Task 13: Create client SDK documentation
- [ ] Task 14: Deployment guide

---

## Completed

### ✅ Task 10: Phase 4 - Code Quality
**Completed:** 2025-11-30

**Changes:**
1. **Removed unused imports:**
   - `process_claim_ore.rs`: Removed `EvDeploy`, `MMClaimSOL`, `Board`, `Round`
   - `process_create_manager.rs`: Removed `std::mem::size_of`

2. **Documented EV calculation constants:**
   - Added comprehensive comments explaining NUM, DEN24, C_LAM
   - Documented the mathematical model for the ORE game

3. **Enhanced error types:**
   - Added `InvalidPDA`, `ManagerNotInitialized`, `InvalidFeeCollector`
   - Added `NoDeployments`, `ArithmeticOverflow`
   - Added descriptive error messages for debugging

---

### ✅ Task 9: Refactor Test Setup for Unit Testing (Improved)
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
