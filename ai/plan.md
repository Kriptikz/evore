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
- [ ] Update tests with deterministic keypairs

## Phase 4: Code Quality (Medium)
> Priority: **MEDIUM** - Good practice improvements

- [ ] Remove unused imports (`EvDeploy`, `MMClaimSOL` in `process_claim_ore.rs`)
- [ ] Replace unsafe casts with `try_into()` and proper error handling
- [ ] Document magic numbers in EV calculation (NUM, DEN24, C_LAM)
- [ ] Add comprehensive error types for each failure mode

## Phase 5: Testing (High)
> Priority: **HIGH** - Validate fixes and prevent regressions

- [ ] Add unit tests for EV calculation edge cases
- [ ] Add integration tests for each instruction
- [ ] Add security-focused tests (invalid authority, wrong accounts, etc.)
- [ ] Add tests for fee calculation verification

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
| Phase 1: Security Fixes | ✅ Complete | 100% (4/4) |
| Phase 2: Security Hardening | ✅ Complete | 100% (4/4) |
| Phase 3: Optimization | 🟡 In Progress | 75% (3/4) |
| Phase 4: Code Quality | 🔴 Not Started | 0% |
| Phase 5: Testing | 🔴 Not Started | 0% |
| Phase 6: Documentation | 🟡 In Progress | 33% (2/6) |
| Phase 7: Deployment | 🔴 Not Started | 0% |

---

## Notes

- Phase 1 & 2 complete! All critical security fixes and hardening done.
- Phase 3 (CU optimization) mostly complete - bump parameters added
- Remaining CU variance is from external ORE program PDA lookups (can't be fixed)
- Consider external audit after Phase 4 completion
