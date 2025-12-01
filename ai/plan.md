# Evore Development Plan

> Last Updated: 2025-12-01

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
- [x] Create bot README with commands
- [ ] Add inline documentation for all public functions
- [ ] Create client SDK documentation

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
- [x] Add tests for each strategy
- [x] Update instruction builders

## Phase 8: Mainnet Deployment
> Priority: **HIGH** - Production deployment

- [x] Mainnet deployment
- [x] Convert to Cargo workspace
- [x] Create bot crate structure

## Phase 9: Evore Bot ✅
> Priority: **HIGH** - Automated deployment bot

### Overview

Bot for automated EV deployments with spam strategy to land transactions in final slots.

### Configuration

**.env file:**
```
RPC_URL=https://your-rpc.com
WS_URL=wss://your-rpc.com
KEYPAIR_PATH=/path/to/signer.json      # Signer keypair (pays fees, signs txs)
MANAGER_PATH=/path/to/manager.json     # Manager keypair (separate account)
```

**Key distinction:**
- **Signer** - Pays transaction fees, must have SOL balance
- **Manager** - Separate keypair, owns the Manager account and controls managed miner auths

### Deployment Strategy (EV + Spam)

1. **Timing**: Deploy at configurable slots_left, starts sending 50ms before target slot
2. **Spam Mode**: Send transactions every 100ms until end_slot reached
3. **Fire-and-forget**: Skip preflight, 0 retries - we handle manually
4. **Confirm later**: Check which transactions landed after spam window

### Commands

| Command | Description |
|---------|-------------|
| `status` | Show current round, slots remaining, deployments |
| `info` | Display managed_miner_auth PDA for website lookup |
| `deploy` | Single EV deployment (spam mode at round end) |
| `run` | Continuous loop: checkpoint → claim SOL → deploy → repeat |
| `checkpoint` | Manual checkpoint (auto-detects round from miner) |
| `claim-sol` | Manual SOL claim |
| `create-manager` | Create Manager account |
| `dashboard` | Live TUI dashboard |

### Implementation Tasks

- [x] Project setup (Cargo workspace, .env support)
- [x] RPC client (skip preflight, 0 retries)
- [x] Websocket slot tracking (real-time slot updates)
- [x] Round state fetching (get_board, get_round, get_miner)
- [x] Transaction building (deploy, checkpoint, claim_sol)
- [x] Single deploy with spam mode + countdown display
- [x] Continuous deploy loop with auto checkpoint & claim SOL
- [x] CLI with subcommands
- [x] Manager keypair loading (separate from signer)
- [x] Balance display (signer, managed_miner_auth, miner rewards)
- [x] Round lifecycle handling (intermission, reset waiting, MAX end_slot)
- [x] Auto-detect checkpoint round from miner account
- [x] Claim SOL only if rewards_sol > 0
- [x] Priority fee code ready (disabled for now)

## Phase 10: Frontend UI
> Priority: **LOW** - Future

- [ ] Dashboard for round monitoring
- [ ] Manual deployment interface
- [ ] Wallet connection
- [ ] Claim interface

---

## Progress Tracking

| Phase | Status | Completion |
|-------|--------|------------|
| Phase 1: Security Fixes | ✅ Complete | 100% (4/4) |
| Phase 2: Security Hardening | ✅ Complete | 100% (4/4) |
| Phase 3: Optimization | ✅ Complete | 100% (4/4) |
| Phase 4: Code Quality | ✅ Complete | 100% (6/6) |
| Phase 5: Testing | ✅ Complete | 100% (6/6) |
| Phase 6: Documentation | 🟡 In Progress | 67% (4/6) |
| Phase 7: Strategies | ✅ Complete | 100% (7/7) |
| Phase 8: Mainnet Deployment | ✅ Complete | 100% (3/3) |
| Phase 9: Evore Bot | ✅ Complete | 100% (13/13) |
| Phase 10: Frontend UI | 🔴 Not Started | 0% |

---

## Notes

- Phases 1-9 complete! Program deployed to mainnet, bot operational.
- Program ID: `6kJMMw6psY1MjH3T3yK351uw1FL1aE7rF3xKFz4prHb`
- 27+ unit tests with comprehensive coverage
- Workspace structure: `program/` (Solana program), `bot/` (deployment bot)
- Bot tested on mainnet with successful deployments
