use solana_program::{
    account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey, system_program
};
use steel::*;

use crate::{
    consts::{DEPLOY_FEE, FEE_COLLECTOR}, entropy_api, error::EvoreError, instruction::{DeployStrategy, MMDeploy}, ore_api::{self, Board, Round}, state::Manager
};

/// Maximum number of CPI calls allowed per transaction
const MAX_CPI_CALLS: usize = 18;

/// A batch of deployments to execute in a single CPI call
/// Multiple squares can receive the same amount in one CPI
#[derive(Clone, Debug)]
pub struct DeploymentBatch {
    pub amount: u64,
    pub squares: [bool; 25],
}

impl DeploymentBatch {
    pub fn new(amount: u64, squares: [bool; 25]) -> Self {
        Self { amount, squares }
    }
    
    /// Create a batch for a single square
    pub fn single(amount: u64, square_idx: usize) -> Self {
        let mut squares = [false; 25];
        if square_idx < 25 {
            squares[square_idx] = true;
        }
        Self { amount, squares }
    }
    
    /// Create a batch for all 25 squares
    pub fn all_squares(amount: u64) -> Self {
        Self { amount, squares: [true; 25] }
    }
}

pub fn process_mm_deploy(
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> Result<(), ProgramError> {
    let args = MMDeploy::try_from_bytes(instruction_data)?;
    let auth_id = u64::from_le_bytes(args.auth_id);
    
    // Parse strategy enum with its data
    let strategy = args.get_strategy()
        .map_err(|_| ProgramError::InvalidInstructionData)?;

    let [
            signer,
            manager_account_info,
            managed_miner_auth_account_info,
            ore_miner_account_info,
            fee_collector_account_info,
            automation_account_info,
            config_account_info,
            board_account_info,
            round_account_info,
            entropy_var_account_info,
            ore_program,
            entropy_program,
            system_program
    ] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    let clock = Clock::get()?;
    let board = board_account_info
        .as_account::<Board>(&ore_api::id())?;

    if clock.slot >= board.end_slot {
        return Err(EvoreError::EndSlotReached.into());
    }

    // EV strategy has slots_left check
    if let DeployStrategy::EV { slots_left, .. } = strategy {
        let current_slots_left = board.end_slot - clock.slot;
        if current_slots_left > slots_left {
            return Err(EvoreError::TooManySlotsLeft.into());
        }
    }

    if !signer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    if !manager_account_info.is_writable {
        return Err(ProgramError::InvalidAccountData);
    }

    if manager_account_info.data_is_empty() {
        return Err(EvoreError::ManagerNotInitialized.into());
    }

    if *ore_program.key != ore_api::id() {
        return Err(ProgramError::IncorrectProgramId);
    }

    if *system_program.key != system_program::id() {
        return Err(ProgramError::IncorrectProgramId);
    }

    if *entropy_program.key != entropy_api::id() {
        return Err(ProgramError::IncorrectProgramId);
    }

    if *fee_collector_account_info.key != FEE_COLLECTOR {
        return Err(EvoreError::InvalidFeeCollector.into());
    }

    let manager = manager_account_info
        .as_account::<Manager>(&crate::id())?;

    if manager.authority != *signer.key {
        return Err(EvoreError::NotAuthorized.into());
    }

    let round = round_account_info
        .as_account::<Round>(&ore_api::id())?;

    // Use create_program_address with bump from instruction data for deterministic CU usage
    let managed_miner_auth_pda = Pubkey::create_program_address(
        &[
            crate::consts::MANAGED_MINER_AUTH,
            manager_account_info.key.as_ref(),
            &auth_id.to_le_bytes(),
            &[args.bump],
        ],
        &crate::id(),
    ).map_err(|_| EvoreError::InvalidPDA)?;

    if managed_miner_auth_pda != *managed_miner_auth_account_info.key {
        return Err(EvoreError::InvalidPDA.into());
    }

    // Check if already deployed this round (only if miner exists)
    let is_already_deployed = if !ore_miner_account_info.data_is_empty() {
        let miner = ore_miner_account_info.as_account::<ore_api::Miner>(&ore_api::id())?;
        miner.round_id == board.round_id
    } else {
        false // First ever deploy, miner doesn't exist yet
    };

    // This applies to all strategies, not just EV
    let allow_multi_deploy = args.get_allow_multi_deploy();
    if !allow_multi_deploy && is_already_deployed {
          return Err(EvoreError::AlreadyDeployedThisRound.into());
    }

    // Calculate deployments based on strategy - returns batched deployments
    let (batches, total_deployed) = match strategy {
        DeployStrategy::EV { bankroll, max_per_square, min_bet, ore_value, .. } => {
            calculate_ev_deployments(round, bankroll, min_bet, max_per_square, ore_value)
        },
        DeployStrategy::Percentage { bankroll, percentage, squares_count } => {
            calculate_percentage_deployments(round, bankroll, percentage, squares_count)
        },
        DeployStrategy::Manual { amounts } => {
            calculate_manual_deployments(amounts)
        },
        DeployStrategy::Split { amount } => {
            calculate_split_deployments(round, amount)
        },
    };

    if total_deployed == 0 {
        return Err(EvoreError::NoDeployments.into());
    }

    let deploy_accounts = 
        vec![
            managed_miner_auth_account_info.clone(),
            managed_miner_auth_account_info.clone(),
            automation_account_info.clone(),
            board_account_info.clone(),
            config_account_info.clone(),
            ore_miner_account_info.clone(),
            round_account_info.clone(),
            system_program.clone(),
            ore_program.clone(),
            entropy_var_account_info.clone(),
            entropy_program.clone(),
            ore_program.clone(),
        ];

    // transfer fee to fee_collector for deployments 1_000 lamports flat fee
    // only transfer on first deploymnet of a round
    if !is_already_deployed {
      let fee_amount = DEPLOY_FEE;
      let transfer_fee_accounts = 
          vec![
              signer.clone(),
              fee_collector_account_info.clone(),
              system_program.clone(),
          ];
      solana_program::program::invoke(
          &solana_program::system_instruction::transfer(
              signer.key,
              fee_collector_account_info.key,
              fee_amount,
          ),
          &transfer_fee_accounts,
      )?;
    }

    // Transfer funds to managed_miner_auth PDA for CPI deployments
    // The PDA must always retain: rent_exempt_minimum + CHECKPOINT_FEE
    // Plus we need enough for: total_deployed + miner_rent (if first deploy)
    let transfer_accounts = 
        vec![
            signer.clone(),
            managed_miner_auth_account_info.clone(),
            system_program.clone(),
        ];
    
    // Minimum rent for 0-byte account (PDA has no data)
    const AUTH_PDA_RENT: u64 = 890_880;
    
    // Miner account rent: ORE creates miner account on first deploy
    let miner_rent = if ore_miner_account_info.data_is_empty() {
        let size = 8 + std::mem::size_of::<ore_api::Miner>();
        solana_program::rent::Rent::default().minimum_balance(size)
    } else {
        0
    };
    
    // Required balance after transaction:
    // - AUTH_PDA_RENT: keep PDA rent-exempt
    // - CHECKPOINT_FEE: ORE checkpoint requires this
    // - total_deployed: funds for deployments
    // - miner_rent: if miner account needs creation
    let required_balance = AUTH_PDA_RENT
        .saturating_add(ore_api::CHECKPOINT_FEE)
        .saturating_add(total_deployed)
        .saturating_add(miner_rent);
    
    let current_balance = managed_miner_auth_account_info.lamports();
    let transfer_amount = required_balance.saturating_sub(current_balance);
    
    if transfer_amount > 0 {
        solana_program::program::invoke(
            &solana_program::system_instruction::transfer(
                signer.key,
                managed_miner_auth_account_info.key,
                transfer_amount,
            ),
            &transfer_accounts,
        )?;
    }

    // Execute batched deployments - each batch is a single CPI call
    let managed_miner_auth_key = *deploy_accounts[0].key;
    for batch in batches {
        if batch.amount == 0 {
            continue;
        }

        solana_program::program::invoke_signed(
            &ore_api::deploy(
                managed_miner_auth_key,
                managed_miner_auth_key,
                batch.amount,
                round.id,
                batch.squares,
            ),
            &deploy_accounts,
            &[&[
                crate::consts::MANAGED_MINER_AUTH,
                manager_account_info.key.as_ref(),
                &auth_id.to_le_bytes(),
                &[args.bump],
            ]],
        )?;
    }

    Ok(())
}

/// Calculate deployments using percentage strategy with bucketing for CPI optimization.
/// Deploys to own `percentage` (in basis points) of each square across `squares_count` squares.
/// 
/// Key behavior: 
/// - If bankroll is insufficient, percentage is automatically reduced.
/// - If more than 18 squares need deployment, squares are grouped into exactly 18 buckets
///   based on similar deployment amounts. Squares in each bucket receive the average amount.
/// - If 18 or fewer squares, each square gets its own deployment (no bucketing).
/// 
/// Formula to own P% of square: amount = P * T / (10000 - P)
/// Max affordable percentage: P_max = 10000 * B / (Total + B)
fn calculate_percentage_deployments(
    round: &Round,
    bankroll: u64,
    percentage: u64,      // In basis points (1000 = 10%)
    squares_count: u64,   // Number of squares to deploy to (1-25)
) -> (Vec<DeploymentBatch>, u64) {
    // Validate inputs
    if percentage == 0 || percentage >= 10000 || squares_count == 0 || squares_count > 25 || bankroll == 0 {
        return (Vec::new(), 0);
    }
    
    let count = (squares_count as usize).min(25);
    
    // Step 1: Calculate total deployed across target squares and identify deployable squares
    let mut total_on_target_squares: u128 = 0;
    let mut deployable_indices: Vec<usize> = Vec::new();
    
    for i in 0..count {
        if round.deployed[i] > 0 {
            total_on_target_squares += round.deployed[i] as u128;
            deployable_indices.push(i);
        }
    }
    
    // If no squares have existing deployments, we can't deploy
    if deployable_indices.is_empty() || total_on_target_squares == 0 {
        return (Vec::new(), 0);
    }
    
    // Step 2: Determine actual percentage (may reduce if bankroll insufficient)
    let p = percentage as u128;
    let b = bankroll as u128;
    let total_cost_at_requested = (p * total_on_target_squares) / (10000 - p);
    
    let actual_percentage = if total_cost_at_requested > b {
        let p_max = (10000u128 * b) / (total_on_target_squares + b);
        p_max.min(percentage as u128) as u64
    } else {
        percentage
    };
    
    if actual_percentage == 0 {
        return (Vec::new(), 0);
    }
    
    // Step 3: Calculate ideal amount for each deployable square
    let p_actual = actual_percentage as u128;
    let mut square_amounts: Vec<(usize, u64)> = Vec::new();
    
    for &i in &deployable_indices {
        let t = round.deployed[i] as u128;
        let amount_u128 = (p_actual * t) / (10000 - p_actual);
        let amount = amount_u128.min(u64::MAX as u128) as u64;
        
        if amount > 0 {
            square_amounts.push((i, amount));
        }
    }
    
    if square_amounts.is_empty() {
        return (Vec::new(), 0);
    }
    
    // Step 4: Create batches - bucket if >18 squares, otherwise individual
    let num_squares = square_amounts.len();
    
    if num_squares <= MAX_CPI_CALLS {
        // No bucketing needed - each square gets its own batch
        let mut batches = Vec::with_capacity(num_squares);
        let mut total_spent: u64 = 0;
        
        for (square_idx, amount) in square_amounts {
            batches.push(DeploymentBatch::single(amount, square_idx));
            total_spent = total_spent.saturating_add(amount);
        }
        
        (batches, total_spent)
    } else {
        // Bucketing required - sort by amount and group into exactly 18 buckets
        bucket_deployments(square_amounts)
    }
}

/// Group squares into exactly 18 buckets based on similar deployment amounts.
/// Squares are sorted by amount and then divided into 18 groups.
/// Each bucket deploys the average amount to all squares in that bucket.
fn bucket_deployments(mut square_amounts: Vec<(usize, u64)>) -> (Vec<DeploymentBatch>, u64) {
    let num_squares = square_amounts.len();
    
    // Sort by amount ascending so adjacent squares have similar amounts
    square_amounts.sort_by_key(|&(_, amt)| amt);
    
    // Distribute squares into exactly 18 buckets
    // With N squares and 18 buckets:
    // - First (N % 18) buckets get (N / 18 + 1) squares each
    // - Remaining buckets get (N / 18) squares each
    let base_size = num_squares / MAX_CPI_CALLS;
    let extra_squares = num_squares % MAX_CPI_CALLS;
    
    let mut batches = Vec::with_capacity(MAX_CPI_CALLS);
    let mut total_spent: u64 = 0;
    let mut idx = 0;
    
    for bucket_idx in 0..MAX_CPI_CALLS {
        // Determine how many squares go in this bucket
        let bucket_size = if bucket_idx < extra_squares {
            base_size + 1
        } else {
            base_size
        };
        
        if bucket_size == 0 {
            continue;
        }
        
        // Collect squares for this bucket and calculate average amount
        let mut squares_mask = [false; 25];
        let mut sum_amounts: u64 = 0;
        
        for _ in 0..bucket_size {
            if idx < num_squares {
                let (square_idx, amount) = square_amounts[idx];
                squares_mask[square_idx] = true;
                sum_amounts = sum_amounts.saturating_add(amount);
                idx += 1;
            }
        }
        
        // Use average amount for this bucket
        let avg_amount = sum_amounts / (bucket_size as u64);
        
        if avg_amount > 0 {
            // Total for this batch is avg_amount * number of squares in batch
            let batch_total = avg_amount.saturating_mul(bucket_size as u64);
            total_spent = total_spent.saturating_add(batch_total);
            batches.push(DeploymentBatch::new(avg_amount, squares_mask));
        }
    }
    
    (batches, total_spent)
}

/// Calculate deployments using split strategy
/// Splits the total amount equally across all 25 squares in a single CPI call
fn calculate_split_deployments(
    round: &Round,
    total_amount: u64,
) -> (Vec<DeploymentBatch>, u64) {
    if total_amount == 0 {
        return (Vec::new(), 0);
    }
    
    // Check that at least one square has existing deployments
    // (can't deploy to completely empty board)
    let has_deployments = round.deployed.iter().any(|&d| d > 0);
    if !has_deployments {
        return (Vec::new(), 0);
    }
    
    // Split equally across all 25 squares
    let per_square = total_amount / 25;
    
    if per_square == 0 {
        return (Vec::new(), 0);
    }
    
    // Total actually deployed (may be slightly less due to integer division)
    let actual_total = per_square * 25;
    
    // Single batch with all 25 squares
    let batch = DeploymentBatch::all_squares(per_square);
    
    (vec![batch], actual_total)
}

/// Calculate deployments using manual strategy
/// Simply uses the provided amounts directly, one batch per square
fn calculate_manual_deployments(
    amounts: [u64; 25],
) -> (Vec<DeploymentBatch>, u64) {
    let mut batches = Vec::new();
    let mut total: u64 = 0;
    
    for i in 0..25 {
        let amount = amounts[i];
        if amount > 0 {
            batches.push(DeploymentBatch::single(amount, i));
            total = total.saturating_add(amount);
        }
    }
    
    (batches, total)
}

/// Calculate deployments using EV waterfill strategy
fn calculate_ev_deployments(
    round: &Round,
    bankroll: u64,
    min_bet: u64,
    max_per_square: u64,
    ore_value_lamports: u64,
) -> (Vec<DeploymentBatch>, u64) {
    // Round.deployed is already [u64; 25], no conversion needed
    let r_deploys = round.deployed;

    let tick: u64 = 100;

    // EV safety per lamport (in ppm of value). 10 ~= 0.001% edge per lamport.
    let margin_ppm: u32 = 10;

    let plan = plan_max_profit_waterfill(
        r_deploys,
        bankroll,
        min_bet,
        tick,
        margin_ppm,
        ore_value_lamports,
        max_per_square,
    );

    // Convert per-square amounts to batches (one batch per non-zero square)
    let mut batches = Vec::new();
    let mut actual_total: u64 = 0;
    for i in 0..25 {
        let amount = plan.per_square[i];
        if amount > 0 {
            batches.push(DeploymentBatch::single(amount, i));
            actual_total = actual_total.saturating_add(amount);
        }
    }
    (batches, actual_total)
}

// ========================== EV Calculation Constants ==========================
//
// These constants model the ORE v3 game economics:
//
// The game has 25 squares. When a round ends:
// - One square is randomly selected as the winner
// - Winners split 89.1% of the total pool from losing squares
// - Plus each winner gets a share of the ORE motherlode
//
// Mathematical model:
// - P(win) = 1/25 for each square
// - EV_sol = stake * (0.891 * L / (T + stake) - 1) where L = losers' pool, T = current square total
// - EV_ore = ore_value * stake / (25 * (T + stake))
//
// Fixed-point arithmetic (multiplied by 1000 to avoid decimals):

/// 89.1% = 891/1000 - fraction of losers' pool distributed to winners
const NUM: u128 = 891;

/// 24.01 = 24010/1000 - derived from 1/P(win) adjusted for the 89.1% factor
/// Formula: 25 / 0.891 ≈ 28.06, but game mechanics adjust this to 24.01
const DEN24: u128 = 24_010;

/// 25 * 1000 - number of squares times the fixed-point multiplier
const C_LAM: u128 = 25_000;


// ============================ Utilities ===============================

#[inline]
fn sum25_u64(v: &[u64; 25]) -> u64 {
    v.iter().copied().sum()
}

// Integer floor sqrt for u128 (Newton)
fn isqrt_u128(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut x0 = n;
    let mut x1 = (n >> 1) + 1;
    while x1 < x0 {
        x0 = x1;
        x1 = (x1 + n / x1) >> 1;
    }
    x0
}

// Snap strictly DOWN to tick & min_bet (never up), u64 flavor.
fn snap_down_u64(amount: u64, min_bet: u64, tick: u64) -> u64 {
    if amount == 0 {
        return 0;
    }
    let a = if tick > 0 { (amount / tick) * tick } else { amount };
    if a < min_bet { 0 } else { a }
}

// ======================= EV / Profit (lamports) =======================

/// EV numerator/denominator with fixed total_sum (S0) and base T_i.
/// This matches the old profit_fraction but uses explicit S0 instead of
/// recomputing the sum.
fn profit_fraction_fixed_s(
    total_sum: u128,       // S0
    ti: u128,              // T_i
    x: u128,               // stake on this square
    ore_value_lamports: u128,
) -> (i128, u128) {
    if x == 0 {
        return (0, 1);
    }

    let tx = ti.saturating_add(x);
    let l  = total_sum.saturating_sub(ti);

    // SOL part: N_sol = x * ( 891*L - 24010*(T + x) )
    let inner_pos = NUM.saturating_mul(l);
    let inner_neg = DEN24.saturating_mul(tx);

    let inner_i: i128 = if inner_pos >= inner_neg {
        (inner_pos - inner_neg) as i128
    } else {
        -((inner_neg - inner_pos) as i128)
    };

    // Widening cast u128 → i128 is safe when x is bounded by lamport values
    let x_i128 = x.min(i128::MAX as u128) as i128;
    let n_sol: i128 = if inner_i >= 0 {
        x_i128.saturating_mul(inner_i)
    } else {
        -(x_i128.saturating_mul(inner_i.saturating_abs()))
    };

    // D = 25*1000*(T + x)
    let d: u128 = C_LAM.saturating_mul(tx);

    // Ore part:
    // EV_ore = ore_value * x / (25 * tx)
    // In terms of the same denominator d:
    // n_ore = EV_ore * d = ore_value * x * 1000
    let ore_num_u = ore_value_lamports
        .saturating_mul(x)
        .saturating_mul(1000);
    // Safe conversion: clamp to i128::MAX if overflow (practically impossible for lamport values)
    let ore_num = ore_num_u.min(i128::MAX as u128) as i128;

    let n_total = n_sol.saturating_add(ore_num);
    (n_total, d)
}

/// EV≥0 ceiling at current state on square i with fixed S0:
///
/// Condition EV_total(x) >= 0 reduces to:
///
///   x <= floor( (NUM*L + 1000*ore_value) / DEN24 ) - T_i
///
/// If this cap is <= 0, there is no non-negative-EV stake you can add
/// on this square at all. We use this as a cheap filter.
fn dmax_for_square_fixed_s(
    total_sum: u128,
    ti: u128,
    ore_value_lamports: u128,
) -> u64 {
    if total_sum <= ti {
        return 0;
    }
    let l = total_sum.saturating_sub(ti);

    let cap = NUM
        .saturating_mul(l)
        .saturating_add(ore_value_lamports.saturating_mul(1_000))
        .saturating_div(DEN24);

    if cap <= ti {
        0
    } else {
        let dmax = cap.saturating_sub(ti);
        if dmax > u64::MAX as u128 {
            u64::MAX
        } else {
            dmax as u64
        }
    }
}

/// Closed-form optimal x_i(λ) with S and L treated as fixed for this square:
///
/// Let:
///   L_i = S0 - T_i
///   A_i = NUM * L_i + 1000 * ore_value
///   B(λ) = DEN24 + C_LAM * λ
///
/// Then the maximizer of EV_i(x) - λ x (continuous relaxation) is:
///
///   x* = max(0, sqrt( T_i * A_i / B(λ) ) - T_i )
///
/// (before applying discrete constraints / snaps).
fn optimal_x_for_lambda(
    total_sum: u128,       // S0
    ti_u64: u64,           // T_i
    ore_value_lamports: u64,
    lambda: u64,           // dimensionless Lagrange multiplier
) -> u64 {
    let ti = u128::from(ti_u64);
    if ti == 0 {
        return 0;
    }

    let s = total_sum;
    if s <= ti {
        // no losers pool, no edge
        return 0;
    }

    let l = s.saturating_sub(ti); // L_i
    let ore = u128::from(ore_value_lamports);

    // A_i = NUM*L_i + 1000*ore_value
    let a = NUM
        .saturating_mul(l)
        .saturating_add(ore.saturating_mul(1_000));

    // B(λ) = DEN24 + C_LAM * λ
    let b_lambda = DEN24.saturating_add(
        C_LAM.saturating_mul(u128::from(lambda))
    );

    if b_lambda == 0 || a == 0 {
        return 0;
    }

    // q = T_i * A_i / B(λ)
    let q = ti
        .saturating_mul(a)
        .saturating_div(b_lambda);

    if q == 0 {
        return 0;
    }

    let root = isqrt_u128(q);
    if root <= ti {
        return 0;
    }

    let x = root.saturating_sub(ti);
    if x == 0 {
        0
    } else {
        // Safe narrowing: clamp to u64::MAX to prevent truncation
        x.min(u64::MAX as u128) as u64
    }
}



// ========================= Water-filling + filter =====================

#[derive(Clone, Debug)]
pub struct Allocation {
    pub per_square: [u64; 25],      // totals per square (for summary)
    pub spent: u64,
    /// Integer estimate of total EV (SOL + ore), in lamports.
    pub exp_profit_est_lamports: i64,
}

/// Compute allocation for a *fixed* λ:
/// - Skip squares that cannot have non-negative EV at λ=0 (active[i] = false).
/// - For each active square, compute x_i(λ) from closed form.
/// - Snap to tick/min_bet, respect bankroll and max_per_square.
/// - Check EV>0 and EV/x >= margin_ppm / 1e6.
/// - Return per-square allocations + total spent + EV estimate.
///
/// NOTE: This assumes total_sum S0 is the round's pre-our-bets total.
/// It does *not* update S as x’s change; this is the approximation we
/// discussed. EV is still computed exactly for the chosen x.
fn allocation_for_lambda(
    t: [u64; 25],
    active: &[bool; 25],
    total_sum_u64: u64,
    bankroll: u64,
    min_bet: u64,
    tick_size: u64,
    margin_ppm: u32,
    ore_value_lamports: u64,
    max_per_square: u64,
    lambda: u64,
) -> Allocation {
    let mut per_square = [0u64; 25];
    let mut spent: u64 = 0;
    let mut ev_sum: i64 = 0;

    // Widening casts (u64 → u128) are always safe
    let total_sum: u128 = u128::from(total_sum_u64);
    let ore_u128: u128 = u128::from(ore_value_lamports);

    if bankroll < min_bet {
        return Allocation {
            per_square,
            spent,
            exp_profit_est_lamports: ev_sum,
        };
    }

    for i in 0..25 {
        if !active[i] {
            continue;
        }

        if spent >= bankroll {
            break;
        }

        let ti_u64 = t[i];
        if ti_u64 == 0 {
            // Original math never bet on empty squares; keep behavior.
            continue;
        }

        // Per-square cap
        let cap_left_for_square = if max_per_square > 0 {
            let already = per_square[i];
            if already >= max_per_square {
                continue;
            }
            max_per_square.saturating_sub(already)
        } else {
            u64::MAX
        };

        if cap_left_for_square < min_bet {
            continue;
        }

        // Continuous optimum for this λ
        let mut x = optimal_x_for_lambda(
            total_sum,
            ti_u64,
            ore_value_lamports,
            lambda,
        );
        if x == 0 {
            continue;
        }

        // Respect global bankroll + per-square cap
        let remaining_bankroll = bankroll.saturating_sub(spent);
        x = x.min(remaining_bankroll).min(cap_left_for_square);
        if x < min_bet {
            continue;
        }

        x = snap_down_u64(x, min_bet, tick_size);
        if x == 0 {
            continue;
        }

        // EV check for this x (widening casts are always safe)
        let ti_u128 = u128::from(ti_u64);
        let x_u128  = u128::from(x);
        let (n, d)  = profit_fraction_fixed_s(
            total_sum,
            ti_u128,
            x_u128,
            ore_u128,
        );

        if n <= 0 {
            continue;
        }

        // Margin check: EV/x >= margin_ppm / 1e6
        if margin_ppm > 0 {
            // n / d / x >= m / 1e6 ⇒ n * 1e6 >= m * x * d
            let lhs = match n.checked_mul(1_000_000) {
                Some(v) => v,
                None => {
                    // Extremely large; skip as safety.
                    continue;
                }
            };
            // Safe widening: u32 → i128, u64 → i128
            let rhs = i128::from(margin_ppm)
                .saturating_mul(i128::from(x))
                .saturating_mul(d.min(i128::MAX as u128) as i128);

            if lhs < rhs {
                continue;
            }
        }

        // Accept allocation
        per_square[i] = per_square[i].saturating_add(x);
        spent = spent.saturating_add(x);

        // Approximate EV contribution: floor(n/d), clamped to i64 range
        // Safe narrowing: d is u128, clamp to i128::MAX before conversion
        let d_i128 = d.min(i128::MAX as u128) as i128;
        let ev_contrib = n / d_i128;
        let ev_contrib_i64 = ev_contrib.clamp(i64::MIN as i128, i64::MAX as i128) as i64;
        ev_sum = ev_sum.saturating_add(ev_contrib_i64);

        if spent >= bankroll {
            break;
        }
    }

    Allocation {
        per_square,
        spent,
        exp_profit_est_lamports: ev_sum,
    }
}

/// Lagrange-multiplier "water-filling" planner:
/// - Compute S0 = sum T and prefilter squares with dmax_i(S0, T_i, ore) < min_bet.
///   Those are EV-neutral-or-negative even at λ=0.
/// - For a given λ, compute per-square x_i(λ) using analytic formula
///   only on active squares.
/// - Binary-search λ so that Σ x_i(λ) is as close as possible to bankroll
///   without exceeding it.
/// - Still enforces EV>0, margin_ppm, min_bet, tick_size, and max_per_square.
pub fn plan_max_profit_waterfill(
    t: [u64; 25],      // current round deployments (lamports)
    bankroll: u64,
    min_bet: u64,
    tick_size: u64,
    margin_ppm: u32,
    ore_value_lamports: u64,
    max_per_square: u64,
) -> Allocation {
    let total_sum_u64 = sum25_u64(&t);
    let total_sum_u128 = u128::from(total_sum_u64);
    let ore_u128 = u128::from(ore_value_lamports);

    // If we can't even place a min bet, bail.
    if bankroll < min_bet {
        return Allocation {
            per_square: [0u64; 25],
            spent: 0,
            exp_profit_est_lamports: 0,
        };
    }

    // ---------- Idea 3: cheap negative-EV prefilter ----------
    let mut active = [true; 25];

    for i in 0..25 {
        let ti_u64 = t[i];
        if ti_u64 == 0 {
            // Keep behavior consistent with original code: never bet on empty squares.
            active[i] = false;
            continue;
        }

        let ti_u128 = u128::from(ti_u64);
        // dmax at λ=0 with fixed S0:
        let dmax0 = dmax_for_square_fixed_s(total_sum_u128, ti_u128, ore_u128);

        // If you can't even place min_bet with EV>=0 on this square,
        // it's EV-neutral-or-negative for any additional stake.
        if dmax0 < min_bet {
            active[i] = false;
        }
    }

    // First check λ = 0 (no “penalty” for budget).
    let alloc_zero = allocation_for_lambda(
        t,
        &active,
        total_sum_u64,
        bankroll,
        min_bet,
        tick_size,
        margin_ppm,
        ore_value_lamports,
        max_per_square,
        0,
    );

    if alloc_zero.spent <= bankroll {
        // We don't saturate bankroll; λ=0 is fine.
        return alloc_zero;
    }

    // Need to increase λ until total spent <= bankroll.
    // Start with λ in [lambda_lo, lambda_hi], doubling lambda_hi until we
    // undershoot or hit a safe upper bound.
    let mut lambda_lo: u64 = 0;
    let mut lambda_hi: u64 = 1;

    const MAX_LAMBDA: u64 = 1 << 40;      // arbitrary large ceiling
    const MAX_LAMBDA_SEARCH_STEPS: usize = 40;
    const MAX_BISECT_STEPS: usize = 40;

    let mut alloc_hi = alloc_zero;

    // Exponential search for an upper bound where spent <= bankroll
    for _ in 0..MAX_LAMBDA_SEARCH_STEPS {
        let alloc = allocation_for_lambda(
            t,
            &active,
            total_sum_u64,
            bankroll,
            min_bet,
            tick_size,
            margin_ppm,
            ore_value_lamports,
            max_per_square,
            lambda_hi,
        );

        if alloc.spent <= bankroll {
            alloc_hi = alloc;
            break;
        }

        lambda_lo = lambda_hi;
        lambda_hi = lambda_hi.saturating_mul(2);
        if lambda_hi >= MAX_LAMBDA {
            // Just clamp here.
            lambda_hi = MAX_LAMBDA;
            alloc_hi = alloc;
            break;
        }
    }

    // If even at MAX_LAMBDA we still overspend, clamp to that.
    if alloc_hi.spent > bankroll {
        return alloc_hi;
    }

    // Binary search between lambda_lo and lambda_hi for a tight λ.
    let mut best_alloc = alloc_hi;
    let mut lo = lambda_lo;
    let mut hi = lambda_hi;

    for _ in 0..MAX_BISECT_STEPS {
        if hi <= lo + 1 {
            break;
        }
        let mid = lo + (hi - lo) / 2;

        let alloc_mid = allocation_for_lambda(
            t,
            &active,
            total_sum_u64,
            bankroll,
            min_bet,
            tick_size,
            margin_ppm,
            ore_value_lamports,
            max_per_square,
            mid,
        );

        if alloc_mid.spent > bankroll {
            // λ still too low ⇒ spend too much ⇒ move low up
            lo = mid;
        } else {
            // Valid (spent <= bankroll). Keep as best and move high down.
            hi = mid;
            best_alloc = alloc_mid;
        }
    }

    best_alloc
}

