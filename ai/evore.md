# Evore Program Documentation

## Overview

Evore is a Solana program that provides **managed miner** functionality for the ORE v3 mining/gambling protocol. It enables automated, EV-positive (Expected Value) betting strategies on ORE's 25-square grid game.

**Program ID:** `6kJMMw6psY1MjH3T3yK351uw1FL1aE7rF3xKFz4prHb`

## Purpose

The ORE v3 protocol features a gambling mechanism where miners deploy SOL across a 25-square grid. After a round ends, a random square is selected, and winners share the pool. Evore automates this process by:

1. Calculating optimal bet distributions using a waterfill algorithm
2. Managing miner accounts via PDAs for secure CPI signing
3. Automating checkpoint and claim operations
4. Collecting fees for the service

---

## Technology Stack

| Component | Technology |
|-----------|------------|
| Language | Rust |
| Framework | Steel 4.0.3 |
| Solana Version | 2.1.x |
| Token Standard | SPL Token |

### Dependencies

```toml
solana-program = "^2.1"
steel = { version = "4.0.3", features = ["spl"] }
spl-token = { version = "^4", features = ["no-entrypoint"] }
spl-associated-token-account = { version = "^6", features = ["no-entrypoint"] }
```

---

## Account Architecture

### Manager Account

The primary account type that stores ownership information for a managed miner setup.

```rust
#[repr(C)]
pub struct Manager {
    pub authority: Pubkey,  // 32 bytes - The owner who controls this manager
}
```

| Field | Size | Description |
|-------|------|-------------|
| Discriminator | 8 bytes | Steel account discriminator (100) |
| authority | 32 bytes | Pubkey of the account owner |
| **Total** | **40 bytes** | |

**Key Properties:**
- Created by the `CreateManager` instruction
- Authority can call all managed miner operations
- One Manager can have multiple auth_ids (sub-accounts)

### Managed Miner Auth PDA

A Program Derived Address used for signing CPI calls to the ORE program.

**Seeds:** `["managed-miner-auth", manager_pubkey, auth_id (u64 LE bytes)]`

**Purpose:**
- Acts as the "authority" for an ORE Miner account
- Signs deploy, checkpoint, and claim transactions via CPI
- Holds SOL temporarily during operations

---

## External Program Integration

### ORE v3 Program

**Address:** `oreV3EG1i9BEgiAJ8b177Z2S2rMarzak4NMv1kULvWv`

Evore interacts with the following ORE accounts:

| Account | PDA Seeds | Description |
|---------|-----------|-------------|
| Board | `["board"]` | Current round info, start/end slots |
| Round | `["round", round_id]` | Per-round deployment and winner data |
| Miner | `["miner", authority]` | Per-user mining state and rewards |
| Automation | `["automation", authority]` | Automation settings |
| Treasury | `["treasury"]` | Protocol treasury |
| Config | `["config"]` | Protocol configuration |

**ORE Data Structures Used:**

```rust
pub struct Board {
    pub round_id: u64,      // Current round number
    pub start_slot: u64,    // Round start slot
    pub end_slot: u64,      // Round end slot
}

pub struct Round {
    pub id: u64,
    pub deployed: [u64; 25],     // SOL per square
    pub slot_hash: [u8; 32],     // Randomness source
    pub count: [u64; 25],        // Miners per square
    pub expires_at: u64,
    pub motherlode: u64,         // ORE rewards
    pub total_deployed: u64,
    pub total_winnings: u64,
    // ... more fields
}

pub struct Miner {
    pub authority: Pubkey,
    pub deployed: [u64; 25],
    pub rewards_sol: u64,
    pub rewards_ore: u64,
    // ... more fields
}
```

### Entropy Program

**Address:** `3jSkUuYBoJzQPMEzTvkDFXCZUBksPamrVhrnHR9igu2X`

Provides verifiable randomness for the ORE protocol.

---

## Instructions

### 1. CreateManager (0x00)

Creates a new Manager account.

**Accounts:**
| # | Account | Signer | Writable | Description |
|---|---------|--------|----------|-------------|
| 0 | signer | ✅ | ✅ | Payer and authority |
| 1 | manager | ✅ | ✅ | New manager account (keypair) |
| 2 | system_program | ❌ | ❌ | System program |

**Data:** None

**Flow:**
1. Validate signer
2. Create account with space for Manager struct
3. Set discriminator
4. Set authority to signer

---

### 2. EvDeploy (0x01)

Deploys SOL across the ORE grid using an EV-optimized strategy.

**Accounts:**
| # | Account | Signer | Writable | Description |
|---|---------|--------|----------|-------------|
| 0 | signer | ✅ | ✅ | Must be manager authority |
| 1 | manager | ❌ | ✅ | Manager account |
| 2 | managed_miner_auth | ❌ | ✅ | PDA for signing |
| 3 | ore_miner | ❌ | ✅ | ORE Miner account |
| 4 | fee_collector | ❌ | ✅ | Fee recipient |
| 5 | automation | ❌ | ✅ | ORE Automation account |
| 6 | board | ❌ | ✅ | ORE Board account |
| 7 | round | ❌ | ✅ | ORE Round account |
| 8 | entropy_var | ❌ | ✅ | Entropy Var account |
| 9 | ore_program | ❌ | ❌ | ORE program |
| 10 | entropy_program | ❌ | ❌ | Entropy program |
| 11 | system_program | ❌ | ❌ | System program |

**Data:**
```rust
pub struct EvDeploy {
    pub auth_id: [u8; 8],        // Sub-account identifier
    pub bankroll: [u8; 8],       // Max SOL to deploy
    pub max_per_square: [u8; 8], // Cap per square
    pub min_bet: [u8; 8],        // Minimum bet size
    pub ore_value: [u8; 8],      // ORE price in lamports
    pub slots_left: [u8; 8],     // Max slots remaining to deploy
}
```

**Flow:**
1. Parse instruction data
2. Validate round hasn't ended
3. Validate slots_left threshold
4. Verify authority owns manager
5. Calculate optimal deployments using waterfill algorithm
6. Transfer fee to fee_collector
7. Transfer deployment funds to PDA
8. CPI deploy for each square with non-zero allocation

---

### 3. MMCheckpoint (0x02)

Triggers a checkpoint for the managed miner to finalize round results.

**Accounts:**
| # | Account | Signer | Writable | Description |
|---|---------|--------|----------|-------------|
| 0 | signer | ✅ | ❌ | Must be manager authority |
| 1 | manager | ❌ | ✅ | Manager account |
| 2 | managed_miner_auth | ❌ | ✅ | PDA for signing |
| 3 | ore_miner | ❌ | ✅ | ORE Miner account |
| 4 | treasury | ❌ | ✅ | ORE Treasury |
| 5 | board | ❌ | ✅ | ORE Board account |
| 6 | round | ❌ | ✅ | ORE Round account |
| 7 | system_program | ❌ | ❌ | System program |
| 8 | ore_program | ❌ | ❌ | ORE program |

**Data:**
```rust
pub struct MMCheckpoint {
    pub auth_id: [u8; 8],
}
```

---

### 4. MMClaimSOL (0x03)

Claims SOL rewards from the ORE miner and transfers to signer.

**Accounts:**
| # | Account | Signer | Writable | Description |
|---|---------|--------|----------|-------------|
| 0 | signer | ✅ | ✅ | Must be manager authority |
| 1 | manager | ❌ | ✅ | Manager account |
| 2 | managed_miner_auth | ❌ | ✅ | PDA for signing |
| 3 | ore_miner | ❌ | ✅ | ORE Miner account |
| 4 | system_program | ❌ | ❌ | System program |
| 5 | ore_program | ❌ | ❌ | ORE program |

**Data:**
```rust
pub struct MMClaimSOL {
    pub auth_id: [u8; 8],
}
```

**Flow:**
1. Verify authority
2. CPI claim_sol to ORE
3. Transfer all PDA lamports to signer

---

### 5. MMClaimORE (0x04)

Claims ORE token rewards and transfers to signer.

**Accounts:**
| # | Account | Signer | Writable | Description |
|---|---------|--------|----------|-------------|
| 0 | signer | ✅ | ✅ | Must be manager authority |
| 1 | manager | ❌ | ✅ | Manager account |
| 2 | managed_miner_auth | ❌ | ✅ | PDA for signing |
| 3 | ore_miner | ❌ | ✅ | ORE Miner account |
| 4 | mint | ❌ | ✅ | ORE token mint |
| 5 | recipient | ❌ | ✅ | PDA's token account |
| 6 | signer_recipient | ❌ | ✅ | Signer's token account |
| 7 | treasury | ❌ | ✅ | ORE Treasury |
| 8 | treasury_tokens | ❌ | ✅ | Treasury token account |
| 9 | system_program | ❌ | ❌ | System program |
| 10 | spl_program | ❌ | ❌ | SPL Token program |
| 11 | spl_ata_program | ❌ | ❌ | SPL ATA program |
| 12 | ore_program | ❌ | ❌ | ORE program |

**Data:**
```rust
pub struct MMClaimORE {
    pub auth_id: [u8; 8],
}
```

**Flow:**
1. Verify authority
2. Create recipient ATA if needed
3. CPI claim_ore to ORE
4. Create signer's ATA if needed
5. Transfer ORE tokens from PDA to signer

---

## EV Calculation Algorithm

The waterfill algorithm optimizes bet placement to maximize expected value.

### Constants

```rust
const NUM: u128 = 891;      // 0.891 (89.1% of losers' pool to winners)
const DEN24: u128 = 24_010; // 24.01 (inverse win probability factor)
const C_LAM: u128 = 25_000; // 25 * 1000 (grid size factor)
```

### Algorithm Overview

1. **Prefilter squares** - Skip squares where EV is inherently negative
2. **Compute optimal stake** per square using closed-form solution
3. **Binary search λ (Lagrange multiplier)** to fit within bankroll
4. **Snap to constraints** - Enforce min_bet, tick_size, max_per_square
5. **Verify positive EV** for each allocation

### Key Functions

- `plan_max_profit_waterfill()` - Main entry point
- `allocation_for_lambda()` - Compute allocation for given λ
- `optimal_x_for_lambda()` - Closed-form optimal stake
- `profit_fraction_fixed_s()` - EV calculation
- `dmax_for_square_fixed_s()` - Maximum EV-positive stake

---

## Constants

```rust
// PDA seed
pub const MANAGED_MINER_AUTH: &[u8] = b"managed-miner-auth";

// Fee recipient
pub const FEE_COLLECTOR: Pubkey = pubkey!("56qSi79jWdM1zie17NKFvdsh213wPb15HHUqGUjmJ2Lr");

// Minimum fee
pub const MIN_DEPLOY_FEE: u64 = 5_000; // 0.000005 SOL
```

---

## Error Codes

```rust
pub enum EvoreError {
    NotAuthorized = 1,      // Signer is not the manager authority
    TooManySlotsLeft = 2,   // Round has too many slots remaining
    EndSlotExceeded = 3,    // Round has already ended
}
```

---

## File Structure

```
src/
├── lib.rs              # Entry point, instruction dispatch
├── state.rs            # Manager account, PDA functions
├── instruction.rs      # Instruction definitions and builders
├── consts.rs           # Constants (seeds, addresses, fees)
├── error.rs            # Custom error types
├── ore_api.rs          # ORE program interface
├── entropy_api.rs      # Entropy program interface
└── processor/
    ├── mod.rs
    ├── process_create_manager.rs
    ├── process_ev_deploy.rs
    ├── process_checkpoint.rs
    ├── process_claim_sol.rs
    └── process_claim_ore.rs
```

---

## Usage Example

```rust
// 1. Create a manager
let manager_keypair = Keypair::new();
let ix1 = evore::instruction::create_manager(
    signer.pubkey(),
    manager_keypair.pubkey()
);

// 2. Deploy with EV strategy
let ix2 = evore::instruction::ev_deploy(
    signer.pubkey(),
    manager_keypair.pubkey(),
    auth_id: 1,
    round_id: current_round,
    bankroll: 300_000_000,      // 0.3 SOL
    max_per_square: 100_000_000, // 0.1 SOL cap
    min_bet: 10_000,            // 0.00001 SOL minimum
    ore_value: 800_000_000,     // 1 ORE = 0.8 SOL
    slots_left: 2,              // Deploy only in last 2 slots
);

// 3. After round ends, checkpoint
let ix3 = evore::instruction::mm_checkpoint(
    signer.pubkey(),
    manager_keypair.pubkey(),
    round_id,
    auth_id: 1,
);

// 4. Claim rewards
let ix4 = evore::instruction::mm_claim_sol(signer, manager, auth_id);
let ix5 = evore::instruction::mm_claim_ore(signer, manager, auth_id);
```

---

## Testing

Run tests with:
```bash
cargo test-sbf
```

Tests use `solana-program-test` with mock accounts loaded from `tests/buffers/`.

