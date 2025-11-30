# Evore Development Plan

> Last Updated: 2025-11-30

## Phase 1: Security Fixes (Critical)
> Priority: **IMMEDIATE** - Must complete before any deployment

- [x] Fix fee transfer bug in `process_ev_deploy.rs` (transfers `total_deployed` instead of `fee_amount`)
- [x] Add fee collector address verification in `process_ev_deploy.rs`
- [x] Add PDA address validation in `process_ev_deploy.rs`
- [x] Add PDA address validation in remaining processors (checkpoint, claim_sol, claim_ore)

## Phase 2: Security Hardening (High)
> Priority: **HIGH** - Complete before mainnet

- [x] Add program verifications (entropy, SPL token, SPL ATA)
- [x] Add writable checks for mutable accounts in `process_claim_sol.rs`
- [x] Add writable checks for mutable accounts in `process_claim_ore.rs`
- [x] Add writable checks for mutable accounts in `process_checkpoint.rs`

## Phase 3: Optimization (High - CU Determinism)
> Priority: **HIGH** - Required for predictable CU usage

- [x] Add `bump` parameter to all instruction structs
- [x] Replace `find_program_address` with `create_program_address` + bump verification
- [x] Update instruction builders to accept/compute bump client-side
- [x] Refactor tests for modular unit testing

## Phase 4: Code Quality (Medium)
> Priority: **MEDIUM** - Good practice improvements

- [x] Remove unused imports (`EvDeploy`, `MMClaimSOL`, `size_of`)
- [x] Document magic numbers in EV calculation (NUM, DEN24, C_LAM)
- [x] Add comprehensive error types for each failure mode
- [ ] Replace unsafe casts with `try_into()` (deferred - mostly safe widening casts)

## Phase 5: Testing (High)
> Priority: **HIGH** - Validate fixes and prevent regressions

- [x] Refactor test infrastructure for unit testing
- [x] Add unit tests for CreateManager instruction
- [x] Add unit tests for EvDeploy instruction
- [x] Add security-focused tests (wrong authority)
- [ ] Add more edge case tests
- [ ] Add tests for new error types

## Phase 6: Documentation (Medium)
> Priority: **MEDIUM** - For maintainability

- [x] Create security audit document
- [x] Create program architecture documentation
- [x] Document EV calculation constants
- [ ] Add inline documentation for all public functions
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
| Phase 1: Security Fixes | ✅ Complete | 100% (4/4) |
| Phase 2: Security Hardening | ✅ Complete | 100% (4/4) |
| Phase 3: Optimization | ✅ Complete | 100% (4/4) |
| Phase 4: Code Quality | ✅ Complete | 100% (3/3) |
| Phase 5: Testing | 🟡 In Progress | 67% (4/6) |
| Phase 6: Documentation | 🟡 In Progress | 50% (3/6) |
| Phase 7: Deployment | 🔴 Not Started | 0% |

---

## Notes

- Phases 1-4 complete! All critical fixes, hardening, optimizations, and code quality done.
- Test infrastructure in place with modular unit testing
- Error types expanded with descriptive messages
- Consider external audit after Phase 5 completion
