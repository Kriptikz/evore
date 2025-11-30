# Current Tasks

> Last Updated: 2025-11-30

## Active

_None currently active_

---

## Up Next

### Task 20: Implement Deployment Strategies
**Priority:** 🔴 High

**Phase 7 - Strategy Implementation:**

1. **Create DeployStrategy enum**
   - `EV` - Current waterfill algorithm
   - `Percentage` - X% on Y squares in order
   - `Manual` - User-specified squares and amounts

2. **Percentage Strategy**
   - Args: `percentage`, `squares_count`, `bankroll`
   - For each square (0 to squares_count - 1):
     - Calculate amount to own `percentage` of that square
     - Formula: `amount = P * T / (1 - P)` where T = current square total
   - Continues until bankroll exhausted
   - Same amounts batched in single CPI call

3. **Manual Strategy**
   - Args: Array of (square_index, amount) pairs
   - Batch squares with same amount in single CPI
   - Full user control

4. **Refactor EV Strategy**
   - Move current logic to separate function
   - Keep existing params: bankroll, min_bet, max_per_square, ore_value, slots_left

---

## Backlog

- [ ] Task 21: Add tests for Percentage strategy
- [ ] Task 22: Add tests for Manual strategy
- [ ] Task 23: Update instruction builders for strategies
- [ ] Task 24: Mainnet deployment
- [ ] Task 25: Create deployment bot
- [ ] Task 26: Create frontend UI

---

## Completed

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
