# Evore Development Plan

> Last Updated: 2025-11-30

## Phase 1: Security Fixes (Critical)
> Priority: **IMMEDIATE** - Must complete before any deployment

- [x] Fix fee transfer bug in `process_ev_deploy.rs` (transfers `total_deployed` instead of `fee_amount`)
- [x] Add fee collector address verification in `process_ev_deploy.rs`
- [x] Add PDA address validation in `process_ev_deploy.rs`
- [x] Add PDA address validation in remaining processors (checkpoint, claim_sol, claim_ore)
- [ ] Fix rent drain issue in `process_claim_sol.rs`

## Phase 2: Security Hardening (High)
> Priority: **HIGH** - Complete before mainnet

- [ ] Add Entropy program verification in `process_ev_deploy.rs`
- [ ] Add SPL Token program verification in `process_claim_ore.rs`
- [ ] Add SPL ATA program verification in `process_claim_ore.rs`
- [ ] Add writable checks for mutable accounts in `process_claim_sol.rs`
- [ ] Add writable checks for mutable accounts in `process_claim_ore.rs`
- [ ] Add writable checks for mutable accounts in `process_checkpoint.rs`

## Phase 3: Code Quality (Medium)
> Priority: **MEDIUM** - Good practice improvements

- [ ] Remove unused imports (`EvDeploy`, `MMClaimSOL` in `process_claim_ore.rs`)
- [ ] Replace unsafe casts with `try_into()` and proper error handling
- [ ] Document magic numbers in EV calculation (NUM, DEN24, C_LAM)
- [ ] Add comprehensive error types for each failure mode

## Phase 4: Testing (High)
> Priority: **HIGH** - Validate fixes and prevent regressions

- [ ] Add unit tests for EV calculation edge cases
- [ ] Add integration tests for each instruction
- [ ] Add security-focused tests (invalid authority, wrong accounts, etc.)
- [ ] Add tests for fee calculation verification
- [ ] Test rent-exempt handling in claim operations

## Phase 5: Optimization (Low)
> Priority: **LOW** - Performance improvements

- [ ] Reduce `.clone()` calls on AccountInfo
- [ ] Consider batching deploy CPIs
- [ ] Optimize loop in `process_ev_deploy.rs` (25 iterations)
- [ ] Cache PDA bumps where possible

## Phase 6: Documentation (Medium)
> Priority: **MEDIUM** - For maintainability

- [x] Create security audit document
- [x] Create program architecture documentation
- [ ] Add inline documentation for all public functions
- [ ] Document the EV waterfill algorithm mathematically
- [ ] Create client SDK documentation
- [ ] Add deployment guide

## Phase 7: Deployment Preparation
> Priority: **FINAL** - Pre-deployment checklist

- [ ] Security audit by external party
- [ ] Testnet deployment and testing
- [ ] Verify program on Solana Explorer
- [ ] Set up monitoring and alerts
- [ ] Create incident response plan
- [ ] Mainnet deployment

---

## Progress Tracking

| Phase | Status | Completion |
|-------|--------|------------|
| Phase 1: Security Fixes | 🟡 In Progress | 80% (4/5) |
| Phase 2: Security Hardening | 🔴 Not Started | 0% |
| Phase 3: Code Quality | 🔴 Not Started | 0% |
| Phase 4: Testing | 🔴 Not Started | 0% |
| Phase 5: Optimization | 🔴 Not Started | 0% |
| Phase 6: Documentation | 🟡 In Progress | 33% (2/6) |
| Phase 7: Deployment | 🔴 Not Started | 0% |

---

## Notes

- All security fixes (Phase 1) must be completed before any public deployment
- Testing (Phase 4) should run in parallel with fixes
- Consider external audit after Phase 2 completion
