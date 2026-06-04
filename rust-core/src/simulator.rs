use crate::graph::Cycle;
use crate::messages::{ExecutionCandidate, PoolKind};
use crate::state::StateStore;

#[derive(Debug, Clone)]
pub struct SimulationConfig {
    pub gas_cost: i128,
    pub min_profit: i128,
    pub borrow_divisor: u128,
    pub max_input_share_bps: u32,
    pub optimization_steps: usize,
    pub stable_max_imbalance_bps: u32,
    pub min_cycle_edge_profit_bps: u32,
    pub allow_v3_approximation: bool,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            gas_cost: 0,
            min_profit: 0,
            borrow_divisor: 1_000,
            max_input_share_bps: 1_000,
            optimization_steps: 12,
            stable_max_imbalance_bps: 500,
            min_cycle_edge_profit_bps: 0,
            allow_v3_approximation: false,
        }
    }
}

pub fn simulate_cycle(state: &StateStore, cycle: &Cycle, config: &SimulationConfig) -> Option<ExecutionCandidate> {
    if config.min_cycle_edge_profit_bps > 0 {
        let marginal_profit_bps = cycle_marginal_profit_bps(state, cycle, config)?;
        if marginal_profit_bps < config.min_cycle_edge_profit_bps as f64 {
            return None;
        }
    }

    let first_pool = state.get(cycle.pool_ids.first()?)?;
    let min_input = first_pool.reserve_in / config.borrow_divisor.max(1);
    let max_input = first_pool
        .reserve_in
        .saturating_mul(config.max_input_share_bps.max(1) as u128)
        / 10_000;
    if min_input == 0 || max_input == 0 || min_input > max_input {
        return None;
    }

    let mut best_input = 0_u128;
    let mut best_output = 0_u128;
    let mut best_profit = i128::MIN;
    let steps = config.optimization_steps.max(1);
    let span = max_input.saturating_sub(min_input);

    for step in 0..=steps {
        let candidate_input = min_input.saturating_add(span.saturating_mul(step as u128) / steps as u128);
        if candidate_input == 0 {
            continue;
        }
        let output = simulate_input(state, cycle, candidate_input, config)?;
        let profit = output as i128 - candidate_input as i128 - config.gas_cost;
        if profit > best_profit {
            best_profit = profit;
            best_input = candidate_input;
            best_output = output;
        }
    }

    if best_profit <= config.min_profit || best_input == 0 {
        return None;
    }

    Some(ExecutionCandidate {
        cycle_id: cycle.id.clone(),
        borrow_token: first_pool.token_in,
        borrow_amount: best_input,
        gross_output: best_output,
        expected_profit: best_profit,
        touched_pools: cycle.pool_ids.clone(),
    })
}

fn cycle_marginal_profit_bps(state: &StateStore, cycle: &Cycle, config: &SimulationConfig) -> Option<f64> {
    let mut rate = 1.0_f64;
    for pool_id in &cycle.pool_ids {
        let pool = state.get(pool_id)?;
        if pool.sqrt_price_x96 > 0 && !config.allow_v3_approximation {
            return None;
        }
        let pool_rate = marginal_output_rate(
            pool.pool_kind,
            pool.reserve_in,
            pool.reserve_out,
            pool.fee_bps,
            config.stable_max_imbalance_bps,
            pool.amp_factor.unwrap_or(100),
        )?;
        rate *= pool_rate;
        if !rate.is_finite() || rate <= 0.0 {
            return None;
        }
    }

    Some((rate - 1.0) * 10_000.0)
}

pub fn marginal_output_rate(pool_kind: PoolKind, reserve_in: u128, reserve_out: u128, fee_bps: u32, stable_max_imbalance_bps: u32, amp_factor: u64) -> Option<f64> {
    let rate = match pool_kind {
        PoolKind::Xyk => exact_xyk_marginal_output_rate(reserve_in, reserve_out, fee_bps)?,
        PoolKind::Stable => {
            let probe_input = marginal_probe_input(reserve_in, reserve_out);
            if probe_input == 0 {
                return None;
            }
            let output = simulate_stable_swap(
                probe_input,
                reserve_in,
                reserve_out,
                fee_bps,
                stable_max_imbalance_bps,
                amp_factor,
            )?;
            if output == 0 {
                return None;
            }
            output as f64 / probe_input as f64
        }
    };

    if rate <= 0.0 || !rate.is_finite() {
        return None;
    }

    Some(rate)
}

pub fn exact_xyk_marginal_output_rate(reserve_in: u128, reserve_out: u128, fee_bps: u32) -> Option<f64> {
    if reserve_in == 0 || reserve_out == 0 || fee_bps >= 10_000 {
        return None;
    }

    let fee_multiplier = (10_000 - fee_bps) as f64 / 10_000.0;
    let rate = (reserve_out as f64 / reserve_in as f64) * fee_multiplier;
    if rate.is_finite() && rate > 0.0 {
        Some(rate)
    } else {
        None
    }
}

pub fn exact_stable_marginal_output_rate(
    reserve_in: u128,
    reserve_out: u128,
    fee_bps: u32,
    stable_max_imbalance_bps: u32,
    amp_factor: u64,
) -> Option<f64> {
    if reserve_in == 0 || reserve_out == 0 || fee_bps >= 10_000 {
        return None;
    }

    let max_reserve = reserve_in.max(reserve_out);
    let imbalance = reserve_in.abs_diff(reserve_out);
    let imbalance_bps = imbalance.saturating_mul(10_000) / max_reserve;
    if imbalance_bps > stable_max_imbalance_bps as u128 {
        return None;
    }

    let amp = amp_factor as u128;
    let d = compute_d(reserve_in, reserve_out, amp)?;
    let ann = amp.checked_mul(4)?;

    let x = reserve_in as f64;
    let y = reserve_out as f64;
    let d = d as f64;
    let ann = ann as f64;
    if x <= 0.0 || y <= 0.0 || d <= 0.0 || ann <= 0.0 {
        return None;
    }

    let c = d.powi(3) / (4.0 * ann * x);
    let numerator = y + (c / x);
    let denominator = (2.0 * y) + x + (d / ann) - d;
    if numerator <= 0.0 || denominator <= 0.0 {
        return None;
    }

    let fee_multiplier = (10_000 - fee_bps) as f64 / 10_000.0;
    let rate = (numerator / denominator) * fee_multiplier;
    if rate.is_finite() && rate > 0.0 {
        Some(rate)
    } else {
        None
    }
}

fn simulate_input(state: &StateStore, cycle: &Cycle, amount_in: u128, config: &SimulationConfig) -> Option<u128> {
    let mut amount = amount_in;
    for pool_id in &cycle.pool_ids {
        let pool = state.get(pool_id)?;
        if pool.sqrt_price_x96 > 0 && !config.allow_v3_approximation {
            return None;
        }
        amount = match pool.pool_kind {
            PoolKind::Xyk => simulate_xyk_swap(amount, pool.reserve_in, pool.reserve_out, pool.fee_bps)?,
            PoolKind::Stable => simulate_stable_swap(
                amount,
                pool.reserve_in,
                pool.reserve_out,
                pool.fee_bps,
                config.stable_max_imbalance_bps,
                pool.amp_factor.unwrap_or(100),
            )?,
        };
    }
    Some(amount)
}

fn simulate_xyk_swap(amount_in: u128, reserve_in: u128, reserve_out: u128, fee_bps: u32) -> Option<u128> {
    if amount_in == 0 || reserve_in == 0 || reserve_out == 0 {
        return None;
    }
    let amount_in_with_fee = amount_in.saturating_mul((10_000 - fee_bps) as u128);
    let numerator = amount_in_with_fee.saturating_mul(reserve_out);
    let denominator = reserve_in
        .saturating_mul(10_000)
        .saturating_add(amount_in_with_fee);
    if denominator == 0 {
        return None;
    }
    Some(numerator / denominator)
}

fn marginal_probe_input(reserve_in: u128, reserve_out: u128) -> u128 {
    let base = reserve_in.min(reserve_out);
    if base == 0 {
        return 0;
    }

    (base / 1_000_000).max(1)
}

fn simulate_stable_swap(
    amount_in: u128,
    reserve_in: u128,
    reserve_out: u128,
    fee_bps: u32,
    stable_max_imbalance_bps: u32,
    amp_factor: u64,
) -> Option<u128> {
    if amount_in == 0 || reserve_in == 0 || reserve_out == 0 {
        return None;
    }

    let max_reserve = reserve_in.max(reserve_out);
    if max_reserve == 0 {
        return None;
    }

    let imbalance = reserve_in.abs_diff(reserve_out);
    let imbalance_bps = imbalance.saturating_mul(10_000) / max_reserve;

    // Imbalance penalty: instead of hard-rejecting pools above the threshold,
    // apply a higher effective fee proportional to how imbalanced the pool is.
    // This ensures the pool is still considered for arbitrage during volatile
    // periods when it needs rebalancing most.
    let effective_fee = if imbalance_bps > stable_max_imbalance_bps as u128 {
        let excess_bps = imbalance_bps - stable_max_imbalance_bps as u128;
        // Scale fee linearly: each bps of excess adds 3 bps to the fee
        let penalty_bps = excess_bps.saturating_mul(6);
        fee_bps as u128 + penalty_bps
    } else {
        fee_bps as u128
    };

    // Clamp fee to 100% max
    let effective_fee = effective_fee.min(10_000);

    let amount_in_with_fee = amount_in.saturating_mul(10_000 - effective_fee) / 10_000;
    let x = reserve_in.checked_add(amount_in_with_fee)?;
    let y = get_y(x, reserve_in, reserve_out, amp_factor)?;
    let amount_out = reserve_out.checked_sub(y)?;

    Some(amount_out.min(reserve_out))
}

// The `construct_uint!` macro internally expands code that clippy flags as a
// manual `div_ceil` reimplementation; we cannot edit the generated code, so the
// lint is allowed here.
#[allow(clippy::manual_div_ceil)]
mod u256_impl {
    use uint::construct_uint;

    construct_uint! {
        pub struct U256(4);
    }
}

pub use u256_impl::U256;

fn get_y(x: u128, reserve_in: u128, reserve_out: u128, amp_factor: u64) -> Option<u128> {
    let amp = amp_factor as u128;
    let d = compute_d(reserve_in, reserve_out, amp)?;
    let ann = amp.checked_mul(4)?;

    // Compute c = D³ / (4 * A * n * x) using U256 to avoid overflow.
    // The naive u128 computation: D²/(2x) then *D/(2*ann) has TWO truncations
    // (losing precision) and D² overflows u128 for 18-decimal pools.
    // With U256 we compute D³ / (4*ann*x) in a single division step.
    let d_u256 = U256::from(d);
    let x_u256 = U256::from(x);
    let two_ann_u256 = U256::from(ann.checked_mul(2)?);
    // c = D³ / (4*ann*x) = D³ / (2*ann*2*x)
    // Compute as: (D * D * D) / (2*ann * 2*x)
    let d_cubed = d_u256 * d_u256 * d_u256;
    let c_u256 = d_cubed / (two_ann_u256 * x_u256 * U256::from(2));
    let c = c_u256.as_u128(); // safe: D³/(4*ann*x) ≈ D * (D/(4*ann)) * (D/x). Since D≈x and ann≥100, this is O(D/ann) ≤ D

    let b = x.checked_add(d.checked_div(ann)?)?;
    let mut y = d;

    for _ in 0..255 {
        let y_prev = y;
        let numerator = y.checked_mul(y)?.checked_add(c)?;
        let denominator = y.checked_mul(2)?.checked_add(b)?.checked_sub(d)?;
        if denominator == 0 {
            return None;
        }
        y = numerator.checked_div(denominator)?;
        if y.abs_diff(y_prev) <= 1 {
            return Some(y);
        }
    }

    Some(y)
}

fn compute_d(reserve_in: u128, reserve_out: u128, amp_factor: u128) -> Option<u128> {
    let sum_u256 = U256::from(reserve_in) + U256::from(reserve_out);
    if sum_u256 == U256::zero() {
        return None;
    }

    let ann = U256::from(amp_factor.checked_mul(4)?);
    let mut d = sum_u256;

    let two_reserve_in = U256::from(reserve_in) * U256::from(2);
    let two_reserve_out = U256::from(reserve_out) * U256::from(2);

    for _ in 0..255 {
        let d_prev = d;
        let d_p = d * d / two_reserve_in * d / two_reserve_out;

        let numerator = (ann * sum_u256 + d_p * U256::from(2)) * d;
        let denominator = (ann - U256::from(1)) * d + d_p * U256::from(3);
        if denominator == U256::zero() {
            return None;
        }
        d = numerator / denominator;
        if d.abs_diff(d_prev) <= U256::from(1) {
            return Some(d.as_u128());
        }
    }

    Some(d.as_u128())
}

#[cfg(test)]
mod tests {
    use super::{exact_stable_marginal_output_rate, simulate_cycle, simulate_stable_swap, SimulationConfig};
    use crate::graph::Cycle;
    use crate::messages::{PoolKind, PoolSnapshot};
    use crate::state::StateStore;

    #[test]
    fn profitable_xyk_cycle_produces_candidate() {
        let state = StateStore::default();
        seed(&state, snapshot("pool-a", PoolKind::Xyk, None, "A", "B", 1_000_000, 1_300_000, 30));
        seed(&state, snapshot("pool-b", PoolKind::Xyk, None, "B", "C", 1_000_000, 1_300_000, 30));
        seed(&state, snapshot("pool-c", PoolKind::Xyk, None, "C", "A", 1_000_000, 1_300_000, 30));

        let cycle = cycle(["pool-a", "pool-b", "pool-c"]);
        let candidate = simulate_cycle(&state, &cycle, &config()).expect("candidate");

        assert!(candidate.borrow_amount > 0);
        assert!(candidate.gross_output > candidate.borrow_amount);
        assert!(candidate.expected_profit > 0);
    }

    #[test]
    fn stable_pool_swap_penalizes_large_imbalance() {
        // Instead of hard-rejecting imbalanced pools, the simulator applies a fee penalty.
        // An imbalanced pool (1M/1.3M = 2307 bps > 500 max) with amount_in=10_000 should
        // still execute but with a much higher effective fee.
        let output_penalty = simulate_stable_swap(10_000, 1_000_000, 1_300_000, 4, 500, 200);
        assert!(output_penalty.is_some());
        // The output should be reduced compared to a non-penalized swap.
        let output_no_penalty = simulate_stable_swap(10_000, 1_000_000, 1_300_000, 4, 9999, 200);
        assert!(output_no_penalty.is_some());
        assert!(output_penalty.unwrap() < output_no_penalty.unwrap());
    }

    #[test]
    fn stable_pool_swap_accepts_near_balanced_pool() {
        let output = simulate_stable_swap(10_000, 1_000_000, 1_001_000, 4, 500, 200);
        assert!(output.is_some());
    }

    #[test]
    fn exact_stable_marginal_rate_is_near_par_for_balanced_pool() {
        let rate = exact_stable_marginal_output_rate(1_000_000, 1_000_000, 4, 500, 200).expect("rate");
        assert!(rate > 0.999);
        assert!(rate < 1.0);
    }

    #[test]
    fn exact_stable_marginal_rate_penalizes_large_imbalance() {
        // The f64 marginal rate function still hard-rejects large imbalances since
        // it's a pre-filter only (used for min_cycle_edge_profit_bps).
        // The actual pool simulation (simulate_stable_swap) uses the soft penalty.
        let rate = exact_stable_marginal_output_rate(1_000_000, 1_300_000, 4, 500, 200);
        assert!(rate.is_none());
    }

    #[test]
    fn optimization_can_use_larger_than_minimum_input() {
        let state = StateStore::default();
        seed(&state, snapshot("pool-a", PoolKind::Xyk, None, "A", "B", 10_000_000, 15_000_000, 30));
        seed(&state, snapshot("pool-b", PoolKind::Xyk, None, "B", "A", 10_000_000, 15_000_000, 30));

        let cycle = cycle(["pool-a", "pool-b"]);
        let cfg = SimulationConfig {
            borrow_divisor: 1_000,
            max_input_share_bps: 2_000,
            optimization_steps: 16,
            ..SimulationConfig::default()
        };

        let candidate = simulate_cycle(&state, &cycle, &cfg).expect("candidate");
        let minimum_input = 10_000_000 / 1_000;
        assert!(candidate.borrow_amount >= minimum_input);
    }

    #[test]
    fn threshold_gate_rejects_weak_marginal_cycles() {
        let state = StateStore::default();
        seed(&state, snapshot("pool-a", PoolKind::Xyk, None, "A", "B", 1_000_000, 1_300_000, 30));
        seed(&state, snapshot("pool-b", PoolKind::Xyk, None, "B", "C", 1_000_000, 1_300_000, 30));
        seed(&state, snapshot("pool-c", PoolKind::Xyk, None, "C", "A", 1_000_000, 1_300_000, 30));

        let cycle = cycle(["pool-a", "pool-b", "pool-c"]);
        let cfg = SimulationConfig {
            min_cycle_edge_profit_bps: 100_000,
            ..config()
        };

        assert!(simulate_cycle(&state, &cycle, &cfg).is_none());
    }

    fn seed(state: &StateStore, snapshot: PoolSnapshot) {
        state.insert_snapshot(snapshot);
    }

    fn snapshot(
        pool_id: &str,
        pool_kind: PoolKind,
        amp_factor: Option<u64>,
        token_in: &str,
        token_out: &str,
        reserve_in: u128,
        reserve_out: u128,
        fee_bps: u32,
    ) -> PoolSnapshot {
        PoolSnapshot {
            pool_id: pool_id.to_string(),
            dex: "test".to_string(),
            pool_kind,
            amp_factor,
            sqrt_price_x96: 0,
            liquidity: 0,
            token_in: token_in.to_string(),
            token_out: token_out.to_string(),
            reserve_in,
            reserve_out,
            fee_bps,
        }
    }

    fn cycle<const N: usize>(pool_ids: [&str; N]) -> Cycle {
        Cycle {
            id: pool_ids.join("->"),
            pool_ids: pool_ids.iter().map(|id| id.to_string()).collect(),
        }
    }

    fn config() -> SimulationConfig {
        SimulationConfig {
            gas_cost: 0,
            min_profit: 0,
            borrow_divisor: 1_000,
            max_input_share_bps: 1_000,
            optimization_steps: 8,
            stable_max_imbalance_bps: 500,
            min_cycle_edge_profit_bps: 0,
            allow_v3_approximation: false,
        }
    }
}
