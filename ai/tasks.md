# Current Tasks

## Active

### Fix Critical Fee Transfer Bug
**File:** `src/processor/process_ev_deploy.rs`  
**Line:** ~131  
**Issue:** Transfers `total_deployed` instead of `fee_amount`  
**Priority:** 🔴 Critical

```rust
// Current (WRONG):
solana_program::system_instruction::transfer(
    signer.key,
    fee_collector_account_info.key,
    total_deployed,  // ❌ Should be fee_amount
)

// Fixed:
solana_program::system_instruction::transfer(
    signer.key,
    fee_collector_account_info.key,
    fee_amount,  // ✅ Correct
)
```

---

### Add PDA Address Validation
**Files:** All processors  
**Priority:** 🔴 Critical

Add validation after computing PDA:
```rust
if managed_miner_auth_pda.0 != *managed_miner_auth_account_info.key {
    return Err(ProgramError::InvalidSeeds);
}
```

---

### Add Fee Collector Verification
**File:** `src/processor/process_ev_deploy.rs`  
**Priority:** 🔴 Critical

```rust
if *fee_collector_account_info.key != crate::consts::FEE_COLLECTOR {
    return Err(ProgramError::InvalidAccountData);
}
```

---

## Backlog

- [ ] Fix rent drain in `process_claim_sol.rs`
- [ ] Add entropy program check
- [ ] Add SPL program checks
- [ ] Add writable account checks
- [ ] Write security tests

---

## Completed

_No completed tasks yet_

