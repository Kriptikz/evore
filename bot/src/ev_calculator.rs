//! EV Calculator - Pure SOL EV calculations for board display
//!
//! Uses the same formula as the on-chain program (process_mm_deploy.rs)
//! with ore_value = 0 for pure SOL EV calculation.
//!
//! Formula:
//! EV_sol = x * (891 * L - 24010 * (T + x)) / (25000 * (T + x))
//!
//! Optimal stake (no limits, maximizes EV):
//! x* = sqrt(T * 891 * L / 24010) - T

/// Constants from the on-chain EV calculation
const NUM: u128 = 891;       // 89.1% - fraction of losers' pool to winners
const DEN24: u128 = 24_010;  // derived from 1/P(win) adjusted for 89.1%
const C_LAM: u128 = 25_000;  // 25 squares * 1000 fixed-point multiplier

/// EV calculation result for a single square
#[derive(Clone, Debug, Default)]
pub struct SquareEV {
    /// Square index (0-24)
    pub index: usize,
    /// Current total deployed on this square (from Round)
    pub total_deployed: u64,
    /// Optimal stake to maximize EV (lamports)
    pub optimal_stake: u64,
    /// Expected profit at optimal stake (lamports, signed)
    pub expected_profit: i64,
    /// Is this square +EV?
    pub is_positive: bool,
}

/// EV calculation results for all 25 squares plus totals
#[derive(Clone, Debug, Default)]
pub struct BoardEV {
    /// Per-square EV data
    pub squares: [SquareEV; 25],
    /// Total optimal deployment across all +EV squares
    pub total_optimal_stake: u64,
    /// Total expected profit if all +EV squares deployed optimally
    pub total_expected_profit: i64,
    /// Number of +EV squares
    pub positive_ev_count: usize,
}

/// Calculate optimal stake for a single square (pure SOL EV, no ore value)
///
/// x* = sqrt(T * NUM * L / DEN24) - T
///
/// Returns 0 if the square is -EV at any stake
fn calculate_optimal_stake(total_sum: u64, ti: u64) -> u64 {
    if ti == 0 {
        return 0;
    }

    let s = total_sum as u128;
    let t = ti as u128;

    if s <= t {
        // No losers pool - no edge possible
        return 0;
    }

    let l = s - t; // Losers pool

    // A = NUM * L (no ore component since ore_value = 0)
    let a = NUM.saturating_mul(l);

    // q = T * A / DEN24
    let q = t.saturating_mul(a).saturating_div(DEN24);

    if q == 0 {
        return 0;
    }

    // x* = sqrt(q) - T
    let root = isqrt_u128(q);
    if root <= t {
        return 0;
    }

    let x = root - t;
    
    // Safe narrowing to u64
    x.min(u64::MAX as u128) as u64
}

/// Calculate EV for a given stake on a square (pure SOL, no ore)
///
/// EV_sol = x * (NUM * L - DEN24 * (T + x)) / (C_LAM * (T + x))
fn calculate_ev(total_sum: u64, ti: u64, stake: u64) -> i64 {
    if stake == 0 || ti == 0 {
        return 0;
    }

    let s = total_sum as u128;
    let t = ti as u128;
    let x = stake as u128;

    if s <= t {
        return 0;
    }

    let l = s - t; // Losers pool
    let tx = t + x;

    // inner_pos = NUM * L
    let inner_pos = NUM.saturating_mul(l);
    // inner_neg = DEN24 * (T + x)
    let inner_neg = DEN24.saturating_mul(tx);

    // Denominator: C_LAM * (T + x)
    let d = C_LAM.saturating_mul(tx);

    if d == 0 {
        return 0;
    }

    // Numerator: x * (inner_pos - inner_neg)
    let (n, is_negative) = if inner_pos >= inner_neg {
        (x.saturating_mul(inner_pos - inner_neg), false)
    } else {
        (x.saturating_mul(inner_neg - inner_pos), true)
    };

    // EV = N / D
    let ev_abs = (n / d) as i64;
    
    if is_negative {
        -ev_abs
    } else {
        ev_abs
    }
}

/// Calculate EV for all squares on the board
pub fn calculate_board_ev(deployed: &[u64; 25]) -> BoardEV {
    let total_sum: u64 = deployed.iter().sum();
    
    let mut result = BoardEV::default();
    
    for i in 0..25 {
        let ti = deployed[i];
        let optimal_stake = calculate_optimal_stake(total_sum, ti);
        let expected_profit = calculate_ev(total_sum, ti, optimal_stake);
        let is_positive = expected_profit > 0;
        
        result.squares[i] = SquareEV {
            index: i,
            total_deployed: ti,
            optimal_stake,
            expected_profit,
            is_positive,
        };
        
        if is_positive {
            result.positive_ev_count += 1;
            result.total_optimal_stake += optimal_stake;
            result.total_expected_profit += expected_profit;
        }
    }
    
    result
}

/// Integer floor sqrt for u128 (Newton's method)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_isqrt() {
        assert_eq!(isqrt_u128(0), 0);
        assert_eq!(isqrt_u128(1), 1);
        assert_eq!(isqrt_u128(4), 2);
        assert_eq!(isqrt_u128(9), 3);
        assert_eq!(isqrt_u128(100), 10);
        assert_eq!(isqrt_u128(101), 10); // floor
    }

    #[test]
    fn test_empty_board() {
        let deployed = [0u64; 25];
        let result = calculate_board_ev(&deployed);
        assert_eq!(result.positive_ev_count, 0);
        assert_eq!(result.total_optimal_stake, 0);
        assert_eq!(result.total_expected_profit, 0);
    }

    #[test]
    fn test_single_square_deployed() {
        // If only one square has deployment, all other squares have 0 EV
        let mut deployed = [0u64; 25];
        deployed[0] = 1_000_000_000; // 1 SOL
        
        let result = calculate_board_ev(&deployed);
        
        // Square 0 has no losers pool (only one square has deployment)
        // So it should be 0 EV
        assert_eq!(result.squares[0].is_positive, false);
    }

    #[test]
    fn test_multiple_squares() {
        // Realistic scenario: multiple squares have deployments
        let mut deployed = [0u64; 25];
        deployed[0] = 1_000_000_000;  // 1 SOL
        deployed[1] = 500_000_000;    // 0.5 SOL
        deployed[2] = 200_000_000;    // 0.2 SOL
        
        let result = calculate_board_ev(&deployed);
        
        // All squares should have +EV opportunity (unless the math says otherwise)
        // The exact values depend on the formula
        println!("Board EV results:");
        for sq in &result.squares[0..5] {
            println!("  Square {}: deployed={}, optimal={}, ev={}, +ev={}",
                sq.index, sq.total_deployed, sq.optimal_stake, sq.expected_profit, sq.is_positive);
        }
        println!("Total: optimal_stake={}, expected_profit={}, +ev_count={}",
            result.total_optimal_stake, result.total_expected_profit, result.positive_ev_count);
    }
}

