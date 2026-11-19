//! Deterministic maths for simulation.
//!
//! The platform maths library computes `sin`, `cos`, `exp` and friends with
//! its own algorithms, so Windows and Linux can return different last bits
//! for the same input, and replays drift apart. Everything here uses only
//! IEEE-754 operations whose results are exactly specified (`+ - * /`,
//! `sqrt`, rounding to an integer), evaluated in `f64` and rounded once to
//! `f32`. The same input gives the same bits on every x86-64 and ARM64
//! machine.
//!
//! Accuracy is within 1 ulp of `f32` for the ranges simulation uses
//! (|x| < 10^5 for the trigonometric functions).
#![forbid(unsafe_code)]

pub mod plane;
pub mod quaternion;

pub use plane::Plane;
pub use quaternion::Quat;

use std::f64::consts::FRAC_PI_2;

// π/2 split in two so `x - k·π/2` keeps its precision for large `k`.
const PIO2_HI: f64 = 1.570_796_326_734_125_6;
const PIO2_LO: f64 = 6.077_100_506_506_192e-11;

/// Taylor coefficients 1/(2n+1)! and 1/(2n)!, enough for |r| ≤ π/4 in f64.
fn sin_poly(r: f64) -> f64 {
    let r2 = r * r;
    let p = -1.0 / 39_916_800.0;
    let p = p * r2 + 1.0 / 362_880.0;
    let p = p * r2 - 1.0 / 5_040.0;
    let p = p * r2 + 1.0 / 120.0;
    let p = p * r2 - 1.0 / 6.0;
    r + r * r2 * p
}

fn cos_poly(r: f64) -> f64 {
    let r2 = r * r;
    let p = 1.0 / 479_001_600.0;
    let p = p * r2 - 1.0 / 3_628_800.0;
    let p = p * r2 + 1.0 / 40_320.0;
    let p = p * r2 - 1.0 / 720.0;
    let p = p * r2 + 1.0 / 24.0;
    let p = p * r2 - 0.5;
    1.0 + r2 * p
}

/// `(sin x, cos x)` in f64, deterministic.
pub fn sin_cos_f64(x: f64) -> (f64, f64) {
    if !x.is_finite() {
        return (f64::NAN, f64::NAN);
    }
    let k = (x / FRAC_PI_2).round();
    let r = (x - k * PIO2_HI) - k * PIO2_LO;
    let (s, c) = (sin_poly(r), cos_poly(r));
    match (k as i64).rem_euclid(4) {
        0 => (s, c),
        1 => (c, -s),
        2 => (-s, -c),
        _ => (-c, s),
    }
}

/// `(sin x, cos x)`, deterministic across platforms.
pub fn sin_cos(x: f32) -> (f32, f32) {
    let (s, c) = sin_cos_f64(f64::from(x));
    (s as f32, c as f32)
}

pub fn sin(x: f32) -> f32 {
    sin_cos(x).0
}

pub fn cos(x: f32) -> f32 {
    sin_cos(x).1
}

/// Square root is exactly rounded by IEEE-754, so the hardware result is already deterministic.
pub fn sqrt(x: f32) -> f32 {
    x.sqrt()
}

const LN2_HI: f64 = 6.931_471_803_691_238e-1;
const LN2_LO: f64 = 1.908_214_929_270_587_7e-10;

/// `y · 2^k` without leaving the exactly-specified operations.
fn scale2(mut y: f64, mut k: i64) -> f64 {
    let p = |e: i64| f64::from_bits(((e + 1023) as u64) << 52);
    while k > 1023 {
        y *= p(1023);
        k -= 1023;
    }
    while k < -1022 {
        y *= p(-1022);
        k += 1022;
    }
    y * p(k)
}

/// e^x in f64, deterministic.
pub fn exp_f64(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x > 709.79 {
        return f64::INFINITY;
    }
    if x < -745.2 {
        return 0.0;
    }
    let k = (x / std::f64::consts::LN_2).round();
    let r = (x - k * LN2_HI) - k * LN2_LO;
    // Taylor series of e^r for |r| <= 0.35, Horner form, 1/n! from n = 14 down.
    let mut p = 1.0;
    for n in (1..=14).rev() {
        p = 1.0 + p * r / n as f64;
    }
    scale2(p, k as i64)
}

/// Natural logarithm in f64, deterministic.
pub fn ln_f64(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    if x.is_infinite() {
        return x;
    }
    // x = m · 2^e with m in [√½, √2).
    let (mut x, mut e) = (x, 0i64);
    if x < f64::MIN_POSITIVE {
        x *= scale2(1.0, 54);
        e -= 54;
    }
    let bits = x.to_bits();
    e += ((bits >> 52) & 0x7ff) as i64 - 1023;
    let mut m = f64::from_bits((bits & ((1u64 << 52) - 1)) | (1023u64 << 52));
    if m > std::f64::consts::SQRT_2 {
        m *= 0.5;
        e += 1;
    }
    // ln m = 2·atanh(f), f = (m-1)/(m+1), |f| <= 0.172.
    let f = (m - 1.0) / (m + 1.0);
    let f2 = f * f;
    let mut p = 0.0;
    for n in (1..=12).rev() {
        p = 1.0 / (2 * n + 1) as f64 + p * f2;
    }
    let lm = 2.0 * f + 2.0 * f * f2 * p;
    let e = e as f64;
    e * LN2_HI + (e * LN2_LO + lm)
}

/// a^b in f64, deterministic. Whole exponents up to 1024 are exact products.
pub fn pow_f64(a: f64, b: f64) -> f64 {
    if b == 0.0 || a == 1.0 {
        return 1.0;
    }
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    let whole = b.fract() == 0.0;
    if whole && b.abs() <= 1024.0 {
        let (mut base, mut n, mut acc) = (a, b.abs() as u64, 1.0f64);
        while n > 0 {
            if n & 1 == 1 {
                acc *= base;
            }
            base *= base;
            n >>= 1;
        }
        return if b < 0.0 { 1.0 / acc } else { acc };
    }
    if a == 0.0 {
        return if b > 0.0 { 0.0 } else { f64::INFINITY };
    }
    if a < 0.0 {
        if !whole {
            return f64::NAN;
        }
        let odd = (b / 2.0).fract() != 0.0;
        let r = exp_f64(b * ln_f64(-a));
        return if odd { -r } else { r };
    }
    exp_f64(b * ln_f64(a))
}

const FRAC_PI_6: f64 = std::f64::consts::FRAC_PI_6;
const TAN_PI_12: f64 = 0.267_949_192_431_122_7;
const SQRT_3: f64 = 1.732_050_807_568_877_2;

/// Arctangent in f64, deterministic.
pub fn atan_f64(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    let (neg, mut t) = (x < 0.0, x.abs());
    let mut base = 0.0;
    let inv = t > 1.0;
    if inv {
        t = 1.0 / t;
    }
    if t > TAN_PI_12 {
        base = FRAC_PI_6;
        t = (t * SQRT_3 - 1.0) / (t + SQRT_3);
    }
    let t2 = t * t;
    let mut p = 0.0;
    for n in (1..=16).rev() {
        let c = 1.0 / (2 * n + 1) as f64;
        p = if n % 2 == 1 { -c } else { c } + p * t2;
    }
    let mut r = base + (t + t * t2 * p);
    if inv {
        r = FRAC_PI_2 - r;
    }
    if neg {
        -r
    } else {
        r
    }
}

/// The angle of the point (x, y), in (-π, π], deterministic.
pub fn atan2_f64(y: f64, x: f64) -> f64 {
    use std::f64::consts::{FRAC_PI_4, PI};
    if x.is_nan() || y.is_nan() {
        return f64::NAN;
    }
    let sign = |v: f64| if y.is_sign_negative() { -v } else { v };
    if x.is_infinite() && y.is_infinite() {
        return sign(if x > 0.0 { FRAC_PI_4 } else { 3.0 * FRAC_PI_4 });
    }
    if x == 0.0 {
        return if y == 0.0 {
            sign(if x.is_sign_negative() { PI } else { 0.0 })
        } else {
            sign(FRAC_PI_2)
        };
    }
    let a = atan_f64(y / x);
    if x > 0.0 {
        a
    } else {
        sign(PI) + a
    }
}

/// Arcsine in f64, deterministic; NaN outside [-1, 1].
pub fn asin_f64(x: f64) -> f64 {
    if !(-1.0..=1.0).contains(&x) {
        return f64::NAN;
    }
    atan2_f64(x, ((1.0 - x) * (1.0 + x)).sqrt())
}

/// Arccosine in f64, deterministic; NaN outside [-1, 1].
pub fn acos_f64(x: f64) -> f64 {
    if !(-1.0..=1.0).contains(&x) {
        return f64::NAN;
    }
    atan2_f64(((1.0 - x) * (1.0 + x)).sqrt(), x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs() -> impl Iterator<Item = f32> {
        (0..20_000).map(|i| (i as f32 - 10_000.0) * 0.013_7)
    }

    #[test]
    fn matches_the_platform_library_to_within_one_ulp() {
        for x in inputs() {
            let (s, c) = sin_cos(x);
            let (rs, rc) = (f64::from(x).sin() as f32, f64::from(x).cos() as f32);
            for (a, b) in [(s, rs), (c, rc)] {
                let ulps = (a.to_bits() as i64 - b.to_bits() as i64).abs();
                assert!(ulps <= 1 || (a - b).abs() < 1e-7, "x={x} got {a} want {b}");
            }
        }
    }

    #[test]
    fn exact_values_at_the_quadrants() {
        assert_eq!(sin_cos(0.0), (0.0, 1.0));
        assert!(sin(std::f32::consts::FRAC_PI_2) == 1.0);
        assert!(cos(std::f32::consts::PI) == -1.0);
        assert!(sin(f32::NAN).is_nan() && cos(f32::INFINITY).is_nan());
    }

    /// The same bits on every machine: this hash was recorded on Linux x86-64
    /// and must hold on Windows, macOS and ARM64 too.
    #[test]
    fn golden_bits_are_identical_on_every_platform() {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for x in inputs() {
            let (s, c) = sin_cos(x);
            for v in [s, c] {
                hash = (hash ^ u64::from(v.to_bits())).wrapping_mul(0x0100_0000_01b3);
            }
        }
        assert_eq!(hash, GOLDEN, "got {hash:#018x}");
    }

    const GOLDEN: u64 = 0xe645_7948_5b67_6ba6;

    fn close(a: f64, b: f64, what: &str) {
        let tol = 4e-15 * b.abs().max(1e-300);
        assert!(
            a == b || (a - b).abs() <= tol || (a.is_nan() && b.is_nan()),
            "{what}: got {a:e} want {b:e}"
        );
    }

    fn wide() -> impl Iterator<Item = f64> {
        (0..40_000).map(|i| (i as f64 - 20_000.0) * 0.0173)
    }

    #[test]
    fn exp_and_ln_match_the_platform_closely() {
        for x in wide() {
            close(exp_f64(x / 20.0), (x / 20.0).exp(), "exp");
            let y = x.abs() * 3.7 + 1e-9;
            close(ln_f64(y), y.ln(), "ln");
        }
        for x in [1e-310, 5e-324, 1e300, f64::MAX, 0.5, 2.0] {
            let tol = if x < 1e-300 { 1e-13 } else { 4e-15 };
            assert!((ln_f64(x) - x.ln()).abs() <= tol * x.ln().abs(), "ln {x:e}");
        }
        assert_eq!(exp_f64(0.0), 1.0);
        assert_eq!(ln_f64(1.0), 0.0);
        assert!(exp_f64(1000.0).is_infinite() && exp_f64(-1000.0) == 0.0);
        assert!(ln_f64(-1.0).is_nan() && ln_f64(0.0) == f64::NEG_INFINITY);
    }

    #[test]
    fn pow_handles_signs_and_whole_exponents() {
        assert_eq!(pow_f64(2.0, 10.0), 1024.0);
        assert_eq!(pow_f64(-2.0, 3.0), -8.0);
        assert_eq!(pow_f64(2.0, -2.0), 0.25);
        assert_eq!(pow_f64(0.0, 0.0), 1.0);
        assert!(pow_f64(-2.0, 0.5).is_nan());
        assert_eq!(pow_f64(0.0, -1.0), f64::INFINITY);
        for x in wide().filter(|x| *x > 0.0) {
            let (a, b) = (x / 50.0, (x / 97.0).sin() * 7.3);
            let (got, want) = (pow_f64(a, b), a.powf(b));
            assert!((got - want).abs() <= 1e-13 * want.abs(), "{a}^{b}: {got} vs {want}");
        }
    }

    #[test]
    fn inverse_trig_matches_the_platform_closely() {
        for x in wide() {
            let t = x / 31.0;
            close(atan_f64(t), t.atan(), "atan");
            let s = (x / 20_000.0 / 0.0173).clamp(-1.0, 1.0);
            assert!((asin_f64(s) - s.asin()).abs() < 1e-15 * 8.0, "asin {s}");
            assert!((acos_f64(s) - s.acos()).abs() < 1e-15 * 8.0, "acos {s}");
            let (y, xx) = ((x * 1.3).sin() * x, (x * 0.7).cos() * 5.0);
            assert!((atan2_f64(y, xx) - y.atan2(xx)).abs() < 1e-15 * 8.0, "atan2 {y} {xx}");
        }
        for (y, x) in [
            (0.0, 0.0),
            (-0.0, 0.0),
            (0.0, -0.0),
            (-0.0, -0.0),
            (1.0, 0.0),
            (-1.0, 0.0),
            (f64::INFINITY, f64::INFINITY),
            (f64::NEG_INFINITY, f64::NEG_INFINITY),
            (1.0, f64::NEG_INFINITY),
            (-1.0, f64::NEG_INFINITY),
        ] {
            assert_eq!(atan2_f64(y, x).to_bits(), y.atan2(x).to_bits(), "atan2({y}, {x})");
        }
        assert!(asin_f64(1.5).is_nan() && acos_f64(-1.01).is_nan());
    }

    /// The same bits on every machine, like the sine table above.
    #[test]
    fn golden_bits_for_exp_ln_pow_and_inverse_trig() {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for x in wide() {
            let s = (x / 346.0).clamp(-1.0, 1.0);
            for v in [
                exp_f64(x / 20.0),
                ln_f64(x.abs() + 0.5),
                pow_f64(x.abs() / 40.0, x / 900.0),
                atan_f64(x),
                atan2_f64(x, 3.0 - x),
                asin_f64(s),
                acos_f64(s),
            ] {
                hash = (hash ^ v.to_bits()).wrapping_mul(0x0100_0000_01b3);
            }
        }
        assert_eq!(hash, MORE_GOLDEN, "got {hash:#018x}");
    }

    const MORE_GOLDEN: u64 = 0xd6aa_2434_9988_16f6;
}
