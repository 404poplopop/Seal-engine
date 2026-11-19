//! When speculation pays (Part 4d.7 and 4d.8 of the design).

/// Expected critical-path time saved per guess:
/// `G = p * t_saved - (1 - p) * t_pen - t_chk`.
pub fn expected_gain(p: f64, t_saved: f64, t_pen: f64, t_chk: f64) -> f64 {
    p * t_saved - (1.0 - p) * t_pen - t_chk
}

/// The accuracy above which a guess pays: `(t_pen + t_chk) / (t_saved + t_pen)`.
/// Above 1 means it never pays.
pub fn breakeven(t_saved: f64, t_pen: f64, t_chk: f64) -> f64 {
    let denom = t_saved + t_pen;
    if denom <= 0.0 {
        return f64::INFINITY;
    }
    (t_pen + t_chk) / denom
}

/// Spare budget for next frame, in milliseconds:
/// `S = eta * (sum_i (1 - u_i) * T + (1 - u_g) * T)`. Utilizations are
/// clamped to 0..1; pass `None` for the GPU when there is no measurement.
pub fn spare_budget_ms(worker_busy: &[f64], gpu_busy: Option<f64>, frame_ms: f64, eta: f64) -> f64 {
    let idle = |u: f64| 1.0 - u.clamp(0.0, 1.0);
    let cpu: f64 = worker_busy.iter().map(|&u| idle(u) * frame_ms).sum();
    let gpu = gpu_busy.map_or(0.0, |u| idle(u) * frame_ms);
    (eta.clamp(0.0, 1.0) * (cpu + gpu)).max(0.0)
}

/// Predicted share of an optimistic batch that must re-execute when each of
/// `k` transactions touches `w` of `accounts`:
/// `rho = 1 - A / (2wk) * (1 - exp(-2wk / A))`.
pub fn predicted_reexecution(accounts: f64, w: f64, k: f64) -> f64 {
    if accounts <= 0.0 || w <= 0.0 || k <= 0.0 {
        return 0.0;
    }
    let x = 2.0 * w * k / accounts;
    1.0 - (1.0 - (-x).exp()) / x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breakeven_is_where_gain_crosses_zero() {
        let (s, pen, chk) = (2.0, 3.0, 0.1);
        let p = breakeven(s, pen, chk);
        assert!(expected_gain(p, s, pen, chk).abs() < 1e-12);
        assert!(expected_gain(p + 0.01, s, pen, chk) > 0.0);
        assert!(expected_gain(p - 0.01, s, pen, chk) < 0.0);
    }

    #[test]
    fn spare_capacity_makes_cheap_checks_pay_at_low_accuracy() {
        // Competing for cores: a wrong guess costs as much as it would save.
        assert!(breakeven(1.0, 1.0, 0.02) > 0.5);
        // Spare capacity only: almost no penalty.
        assert!(breakeven(1.0, 0.0, 0.02) < 0.03);
    }

    #[test]
    fn spare_budget_counts_partly_busy_cores_and_the_gpu() {
        let s = spare_budget_ms(&[1.0, 0.5, 0.0, 2.0], Some(0.75), 16.0, 0.8);
        // idle: 0 + 8 + 16 + 0 (clamped) + gpu 4 = 28, times 0.8.
        assert!((s - 22.4).abs() < 1e-9);
        assert_eq!(spare_budget_ms(&[], None, 16.0, 0.8), 0.0);
    }

    #[test]
    fn reexecution_model_matches_the_measured_table() {
        let k = 8192.0;
        let close = |a: f64, want: f64| (predicted_reexecution(a, 2.0, k) - want).abs() < 0.01;
        assert!(close(1_000_000.0, 0.016));
        assert!(close(10_000.0, 0.707));
        assert!(close(1_000.0, 0.970));
    }
}
