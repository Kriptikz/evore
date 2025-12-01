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
- [x] Replace unsafe casts with safe conversions (`From`, `.min()` + cast, `.clamp()`)
- [x] Simplify `calculate_deployments` function signature (u64 instead of u128 params)
- [x] Simplify fee calculation (avoid unnecessary widening/narrowing)

## Phase 5: Testing (High)
> Priority: **HIGH** - Validate fixes and prevent regressions

- [x] Refactor test infrastructure for unit testing
- [x] Add unit tests for CreateManager instruction
- [x] Add unit tests for EvDeploy instruction
- [x] Add security-focused tests (wrong authority)
- [x] Add tests for all error types
- [x] Add edge case tests

## Phase 6: Documentation (Medium)
> Priority: **MEDIUM** - For maintainability

- [x] Create security audit document
- [x] Create program architecture documentation
- [x] Document EV calculation constants
- [ ] Add inline documentation for all public functions
- [ ] Create client SDK documentation
- [ ] Add deployment guide

## Phase 7: Deployment Strategies
> Priority: **HIGH** - Multiple strategy options for deploy instruction

### Strategy Types

1. **EV Strategy** (Current)
   - Takes bankroll, min_bet, max_per_square, ore_value, slots_left
   - Calculates optimal deployment using waterfill algorithm
   - Deploys based on +EV squares

2. **Percentage Strategy** (New)
   - Args: `percentage`, `squares_count`, `bankroll`
   - For each square 0 to (squares_count - 1):
     - Calculate amount to own `percentage` of that square
     - Formula: `amount = P * T / (1 - P)` where T = current square total
     - Example: Square has 1 SOL, want 10% → deploy 0.111 SOL (0.111/1.111 = 10%)
   - Continues until bankroll exhausted
   - Same amounts can be batched in single CPI call
   - No randomization - deploys in order

3. **Manual Strategy** (New)
   - User specifies exact squares and amounts
   - Squares with same amount can be batched in single CPI call
   - Full control over deployment

### Implementation Tasks

- [x] Create `DeployStrategy` enum (EV, Percentage, Manual)
- [x] Implement percentage-based deployment processor
- [x] Implement manual deployment processor
- [x] Refactor current EV logic into strategy pattern
- [x] Add strategy selection to instruction
- [ ] Add tests for each strategy
- [x] Update instruction builders

## Phase 8: Deployment & Products
> Priority: **FINAL** - Production deployment and tooling

- [ ] Mainnet deployment
- [ ] Create bot for automated deployments
- [ ] Create frontend UI

---

## Progress Tracking

| Phase | Status | Completion |
|-------|--------|------------|
| Phase 1: Security Fixes | ✅ Complete | 100% (4/4) |
| Phase 2: Security Hardening | ✅ Complete | 100% (4/4) |
| Phase 3: Optimization | ✅ Complete | 100% (4/4) |
| Phase 4: Code Quality | ✅ Complete | 100% (6/6) |
| Phase 5: Testing | ✅ Complete | 100% (6/6) |
| Phase 6: Documentation | 🟡 In Progress | 50% (3/6) |
| Phase 7: Strategies | 🟡 In Progress | 86% (6/7) |
| Phase 8: Deployment | 🔴 Not Started | 0% |

---

## Notes

- Phases 1-5 complete! All critical fixes, hardening, optimizations, code quality, and testing done.
- 21+ unit tests with comprehensive coverage
- Phase 7 introduces flexible deployment strategies for different use cases
- Phase 8 is final production deployment with bot and frontend
