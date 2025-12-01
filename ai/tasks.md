# Current Tasks

> Last Updated: 2025-12-01

## Active

### Task 22: Mainnet Deployment
**Priority:** 🔴 High

**Steps:**
1. Build program for mainnet: `cargo build-sbf`
2. Deploy to Solana mainnet
3. Verify deployment
4. Test basic functionality on mainnet

---

## Up Next

- Task 23: Create deployment bot
- Task 24: Create frontend UI

---

## Backlog

_None_

---

## Completed

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
