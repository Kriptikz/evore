# Evore Program Security Audit

## Overview

Evore is a Solana program built using the Steel framework that provides managed miner functionality for the ORE v3 mining/gambling protocol. It allows users to create "managed miner" accounts that can deploy SOL across a 25-square grid with an expected value (EV) based betting strategy.

**Program ID:** `6kJMMw6psY1MjH3T3yK351uw1FL1aE7rF3xKFz4prHb`

---

## Architecture

### Account Structures

#### 1. Manager Account
```rust
pub struct Manager {
    pub authority: Pubkey,  // The owner who controls this managed miner
}
```
- **Purpose:** Stores the authority (owner) of a managed miner setup
- **Discriminator:** 100
- **Size:** 8 (discriminator) + 32 (Pubkey) = 40 bytes

#### 2. Managed Miner Auth PDA
- **Seeds:** `["managed-miner-auth", manager_pubkey, auth_id (u64)]`
- **Purpose:** Acts as a signer authority for CPI calls to the ORE program
- **Note:** This is not a custom account structure - it's a system account PDA used purely for signing

### External Dependencies

| Program | Address | Purpose |
|---------|---------|---------|
| ORE v3 | `oreV3EG1i9BEgiAJ8b177Z2S2rMarzak4NMv1kULvWv` | Main mining/gambling protocol |
| Entropy | `3jSkUuYBoJzQPMEzTvkDFXCZUBksPamrVhrnHR9igu2X` | Randomness provider |
| SPL Token | System | Token transfers |
| SPL ATA | System | Associated token accounts |

### Instruction Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                         EVORE PROGRAM                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌──────────────────┐     ┌──────────────────┐                 │
│  │  CreateManager   │────►│  Manager Account │                 │
│  │  (0x00)          │     │  Created         │                 │
│  └──────────────────┘     └──────────────────┘                 │
│                                                                 │
│  ┌──────────────────┐     ┌──────────────────┐                 │
│  │    EvDeploy      │────►│  CPI to ORE      │                 │
│  │    (0x01)        │     │  deploy()        │                 │
│  └──────────────────┘     └──────────────────┘                 │
│           │                        │                            │
│           ▼                        ▼                            │
│  ┌──────────────────┐     ┌──────────────────┐                 │
│  │ EV Calculation   │     │ Fee Transfer     │                 │
│  │ (Waterfill Algo) │     │ to FEE_COLLECTOR │                 │
│  └──────────────────┘     └──────────────────┘                 │
│                                                                 │
│  ┌──────────────────┐     ┌──────────────────┐                 │
│  │  MMCheckpoint    │────►│  CPI to ORE      │                 │
│  │  (0x02)          │     │  checkpoint()    │                 │
│  └──────────────────┘     └──────────────────┘                 │
│                                                                 │
│  ┌──────────────────┐     ┌──────────────────┐                 │
│  │   MMClaimSOL     │────►│  CPI to ORE      │──► Transfer     │
│  │   (0x03)         │     │  claim_sol()     │    to Signer    │
│  └──────────────────┘     └──────────────────┘                 │
│                                                                 │
│  ┌──────────────────┐     ┌──────────────────┐                 │
│  │   MMClaimORE     │────►│  CPI to ORE      │──► Transfer     │
│  │   (0x04)         │     │  claim_ore()     │    to Signer    │
│  └──────────────────┘     └──────────────────┘                 │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## Security Issues & Vulnerabilities

### 🔴 CRITICAL

#### 1. Missing PDA Address Validation
**Location:** `process_ev_deploy.rs`, `process_checkpoint.rs`, `process_claim_sol.rs`, `process_claim_ore.rs`

**Issue:** The managed miner auth PDA is computed but **never verified** against the provided `managed_miner_auth_account_info`. An attacker could pass a different account (even one they control) and exploit the program.

```rust
// PDA is computed but the address is never checked against managed_miner_auth_account_info
let managed_miner_auth_pda = Pubkey::find_program_address(
    &[
        crate::consts::MANAGED_MINER_AUTH,
        manager_account_info.key.as_ref(),
        &auth_id.to_le_bytes(),
    ],
    &crate::id(),
);
// ❌ Missing: assert!(managed_miner_auth_pda.0 == *managed_miner_auth_account_info.key)
```

**Impact:** While `invoke_signed` would fail if seeds don't match, an attacker could potentially manipulate the program flow or cause unexpected behavior. The explicit check should be done for defense-in-depth.

**Recommendation:**
```rust
if managed_miner_auth_pda.0 != *managed_miner_auth_account_info.key {
    return Err(ProgramError::InvalidSeeds);
}
```

#### 2. Missing Manager Account Ownership Verification
**Location:** `process_ev_deploy.rs`, `process_checkpoint.rs`, `process_claim_sol.rs`, `process_claim_ore.rs`

**Issue:** While `as_account::<Manager>(&crate::id())` deserializes the account, there's no explicit check that the manager account's owner is the Evore program before deserialization in all cases.

**Impact:** Potential for type confusion attacks if a malicious account with the same discriminator byte is passed.

**Recommendation:** Add explicit owner check:
```rust
if *manager_account_info.owner != crate::id() {
    return Err(ProgramError::IllegalOwner);
}
```

### 🟠 HIGH

#### 3. Fee Transfer Before Deploy Calculation Bug
**Location:** `process_ev_deploy.rs:119-134`

**Issue:** The fee is calculated based on `total_deployed` but is transferred using `total_deployed` instead of `fee_amount`. This means the user pays 100% of their deployment amount as a "fee" instead of 1%.

```rust
// Fee calculation: 1% of total_deployed
let fee_amount = (((total_deployed as u128).saturating_mul(100).saturating_div(10_000)) as u64).max(MIN_DEPLOY_FEE);

// ❌ BUG: Transfers total_deployed instead of fee_amount!
solana_program::program::invoke(
    &solana_program::system_instruction::transfer(
        signer.key,
        fee_collector_account_info.key,
        total_deployed,  // Should be: fee_amount
    ),
    &transfer_fee_accounts,
)?;
```

**Impact:** Users lose 100% of their deployment amount to fees instead of 1%. This is a critical economic bug.

**Recommendation:**
```rust
solana_program::program::invoke(
    &solana_program::system_instruction::transfer(
        signer.key,
        fee_collector_account_info.key,
        fee_amount,  // Use the calculated fee
    ),
    &transfer_fee_accounts,
)?;
```

#### 4. ClaimSOL Drains Entire PDA Balance Including Rent
**Location:** `process_claim_sol.rs:90-103`

**Issue:** The claim_sol instruction transfers ALL lamports from the managed_miner_auth PDA to the signer, including rent reserve. This could close the account or make it unusable.

```rust
solana_program::program::invoke_signed(
    &solana_program::system_instruction::transfer(
        managed_miner_auth_account_info.key,
        signer.key,
        managed_miner_auth_account_info.lamports(),  // ALL lamports including rent
    ),
    ...
)?;
```

**Impact:** The PDA account could be garbage collected if it falls below rent-exempt threshold, making subsequent operations fail.

**Recommendation:** Calculate the amount to withdraw, leaving rent-exempt minimum:
```rust
let rent = Rent::get()?;
let rent_exempt_minimum = rent.minimum_balance(0);
let withdrawable = managed_miner_auth_account_info
    .lamports()
    .saturating_sub(rent_exempt_minimum);
```

#### 5. Missing Fee Collector Address Validation
**Location:** `process_ev_deploy.rs`

**Issue:** The `fee_collector_account_info` is not validated against the constant `FEE_COLLECTOR` address. An attacker could substitute their own address to receive fees.

```rust
// ❌ No verification that fee_collector_account_info.key == FEE_COLLECTOR
solana_program::program::invoke(
    &solana_program::system_instruction::transfer(
        signer.key,
        fee_collector_account_info.key,  // Could be any address!
        total_deployed,
    ),
    ...
)?;
```

**Recommendation:**
```rust
if *fee_collector_account_info.key != crate::consts::FEE_COLLECTOR {
    return Err(ProgramError::InvalidAccountData);
}
```

### 🟡 MEDIUM

#### 6. Missing Writable Check for Key Accounts
**Location:** Multiple processors

**Issue:** Several accounts that need to be mutable are not checked for the `is_writable` flag:
- `process_claim_sol.rs`: `managed_miner_auth_account_info` not checked for writable
- `process_claim_ore.rs`: Multiple accounts not checked
- `process_checkpoint.rs`: `managed_miner_auth_account_info` not checked

**Impact:** Transaction might fail silently or behave unexpectedly if accounts aren't marked writable.

**Recommendation:** Add writable checks for all accounts that will be modified.

#### 7. Missing Entropy Program Verification
**Location:** `process_ev_deploy.rs`

**Issue:** The `entropy_program` account key is never verified against `entropy_api::id()`.

```rust
// ❌ Missing check
if *entropy_program.key != entropy_api::id() {
    return Err(ProgramError::IncorrectProgramId);
}
```

#### 8. Unused `managed_miner_auth_pda` Variable
**Location:** `process_ev_deploy.rs:84`

**Issue:** The `managed_miner_auth_pda` is computed but the `.0` (address) component is never used for validation. Only the bump (`.1`) is used later.

#### 9. No Reentrancy Protection
**Issue:** While Solana's programming model is generally resistant to traditional reentrancy, the CPIs to ORE and back could theoretically allow state manipulation if ORE has callbacks.

**Recommendation:** Consider using a reentrancy guard pattern if ORE program has any callback mechanisms.

### 🟢 LOW

#### 10. Missing SPL Token Program Verification
**Location:** `process_claim_ore.rs`

**Issue:** `spl_program` and `spl_ata_program` are not verified against expected addresses.

```rust
// Should add:
if *spl_program.key != spl_token::id() {
    return Err(ProgramError::IncorrectProgramId);
}
if *spl_ata_program.key != spl_associated_token_account::id() {
    return Err(ProgramError::IncorrectProgramId);
}
```

#### 11. Hardcoded Magic Numbers
**Location:** `process_ev_deploy.rs`

**Issue:** Mathematical constants are hardcoded without clear documentation:
```rust
const NUM: u128 = 891;       // 0.891 = 891/1000
const DEN24: u128 = 24_010;  // 24.01
const C_LAM: u128 = 25_000;  // 25 * 1000
```

**Recommendation:** Add comprehensive documentation explaining the mathematical model.

#### 12. Potential Integer Truncation
**Location:** `process_ev_deploy.rs:193-200`

**Issue:** Values are cast from u128 to u64 without overflow checks:
```rust
let bankroll_u64: u64 = bankroll as u64;  // Could truncate
```

**Recommendation:** Use `try_into()` with proper error handling.

#### 13. Unused Import
**Location:** `process_claim_ore.rs:7`

**Issue:** `EvDeploy` and `MMClaimSOL` are imported but never used.

---

## Missing Security Checks Summary

| Check | CreateManager | EvDeploy | Checkpoint | ClaimSOL | ClaimORE |
|-------|---------------|----------|------------|----------|----------|
| Signer verification | ✅ | ✅ | ✅ | ✅ | ✅ |
| Manager owner check | N/A | ⚠️ | ⚠️ | ⚠️ | ⚠️ |
| PDA address validation | N/A | ❌ | ❌ | ❌ | ❌ |
| System program check | ✅ | ✅ | ✅ | ✅ | ✅ |
| ORE program check | N/A | ✅ | ✅ | ✅ | ✅ |
| Entropy program check | N/A | ❌ | N/A | N/A | N/A |
| SPL program check | N/A | N/A | N/A | N/A | ❌ |
| Fee collector check | N/A | ❌ | N/A | N/A | N/A |
| Writable checks | ✅ | ⚠️ | ⚠️ | ❌ | ❌ |
| Authority check | N/A | ✅ | ✅ | ✅ | ✅ |

Legend: ✅ Present | ❌ Missing | ⚠️ Partial/Implicit

---

## Recommendations

### Immediate Fixes (Before Deployment)

1. **Fix fee transfer bug** - Change `total_deployed` to `fee_amount` in the transfer
2. **Add PDA address validation** - Verify computed PDAs match provided accounts
3. **Add fee collector verification** - Check against hardcoded constant
4. **Fix rent drain issue** - Leave rent-exempt minimum in PDA accounts

### Security Improvements

1. **Add comprehensive account validation** - Check all account owners, writability, and addresses
2. **Add program ID checks** - Verify all external programs (entropy, SPL token, SPL ATA)
3. **Use checked arithmetic** - Replace casts with `try_into()` and handle errors
4. **Add reentrancy guard** - If ORE program has callbacks

### Code Quality

1. **Remove unused imports** - Clean up `EvDeploy`, `MMClaimSOL` imports
2. **Document magic numbers** - Explain EV calculation constants
3. **Add comprehensive error types** - Create specific errors for each failure mode
4. **Add unit tests** - Test edge cases and attack scenarios

---

## Gas/Compute Optimization Notes

1. **PDA computation** - `find_program_address` is expensive. Consider passing bump as instruction data for verification instead of recomputation.

2. **Clone vs reference** - Multiple `.clone()` calls on AccountInfo could be avoided by restructuring.

3. **Loop in deploy** - The 25-iteration loop with conditional CPIs is compute-intensive. Consider batching or off-chain calculation.

---

## Conclusion

The Evore program has several security issues that need to be addressed before mainnet deployment. The most critical are:

1. **Fee transfer bug** that charges 100% instead of 1%
2. **Missing PDA validation** that could lead to exploits
3. **Missing fee collector check** that could redirect fees
4. **Rent drain issue** that could break PDA accounts

The codebase shows good practices in some areas (authority checks, basic signer verification) but lacks the comprehensive validation expected for a production Solana program handling user funds.

