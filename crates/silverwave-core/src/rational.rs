//! Exact rational cycle time (`Rational`, `Rational64`, `Time`) with `i128` intermediate
//! arithmetic and Stern-Brocot continued-fraction overflow protection (`MAX_DENOMINATOR`).

use alloc::string::{String, ToString};
use core::cmp::Ordering;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::ops::{Add, Div, Mul, Neg, Rem, Sub};
use core::str::FromStr;

/// Exact rational number `num / den` maintained in canonical reduced form (`den > 0`, `gcd(|num|, den) == 1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rational {
    num: i64,
    den: i64,
}

/// Canonical 64-bit rational alias shared across all SilverWave crates.
pub type Rational64 = Rational;

/// Exact rational cycle time. `0` is the origin of cycle 0; `[n, n + 1)` is cycle `n`.
pub type Time = Rational;

impl Rational {
    /// Prime ceiling for denominators before Stern-Brocot convergent rationalization engages (`PP-03`).
    pub const MAX_DENOMINATOR: i64 = 1_000_000_007;

    /// Constructs a canonical reduced `Rational`.
    /// Returns `None` if `den == 0`.
    #[inline]
    pub fn new(num: i64, den: i64) -> Option<Self> {
        if den == 0 {
            return None;
        }
        Some(Self::checked_op_stern_brocot(num as i128, den as i128))
    }

    /// Constructs a canonical reduced `Rational`, clamping `den == 0` to `1` for infallible contexts.
    #[inline]
    pub const fn new_const(num: i64, den: i64) -> Self {
        if den == 0 {
            return Self { num: 0, den: 1 };
        }
        let (n, d) = if den < 0 {
            (-(num as i128), -(den as i128))
        } else {
            (num as i128, den as i128)
        };
        let g = Self::gcd_u128(n.unsigned_abs(), d.unsigned_abs());
        let rn = (n / (g as i128)) as i64;
        let rd = (d / (g as i128)) as i64;
        Self { num: rn, den: rd }
    }

    /// Constructs an integer `Rational` (`n / 1`).
    #[inline]
    pub const fn from_integer(n: i64) -> Self {
        Self { num: n, den: 1 }
    }

    /// Additive identity `0 / 1`.
    #[inline]
    pub const fn zero() -> Self {
        Self { num: 0, den: 1 }
    }

    /// Multiplicative identity `1 / 1`.
    #[inline]
    pub const fn one() -> Self {
        Self { num: 1, den: 1 }
    }

    /// Numerator accessor.
    #[inline]
    pub const fn numer(self) -> i64 {
        self.num
    }

    /// Denominator accessor (always `> 0`).
    #[inline]
    pub const fn denom(self) -> i64 {
        self.den
    }

    /// Returns `true` iff `self.num == 0`.
    #[inline]
    pub const fn is_zero(self) -> bool {
        self.num == 0
    }

    /// Returns `true` iff `self.num > 0`.
    #[inline]
    pub const fn is_positive(self) -> bool {
        self.num > 0
    }

    /// Returns `true` iff `self.num < 0`.
    #[inline]
    pub const fn is_negative(self) -> bool {
        self.num < 0
    }

    /// Returns `|self|`.
    #[inline]
    pub fn abs(self) -> Self {
        Self::checked_op_stern_brocot((self.num as i128).abs(), self.den as i128)
    }

    /// Returns `-1`, `0`, or `1` as a `Rational`.
    #[inline]
    pub const fn signum(self) -> Self {
        if self.num > 0 {
            Self::one()
        } else if self.num < 0 {
            Self { num: -1, den: 1 }
        } else {
            Self::zero()
        }
    }

    /// Reciprocal `den / num`, or `None` if `self == 0`.
    #[inline]
    pub fn recip(self) -> Option<Self> {
        if self.num == 0 {
            None
        } else {
            Some(Self::checked_op_stern_brocot(self.den as i128, self.num as i128))
        }
    }

    /// Largest integer `<= self`.
    #[inline]
    pub const fn floor_i64(self) -> i64 {
        self.num.div_euclid(self.den)
    }

    /// Largest integer `<= self` as a `Rational`.
    #[inline]
    pub const fn floor(self) -> Self {
        Self::from_integer(self.floor_i64())
    }

    /// Smallest integer `>= self` as `i64`.
    #[inline]
    pub const fn ceil_i64(self) -> i64 {
        let q = self.num.div_euclid(self.den);
        let r = self.num.rem_euclid(self.den);
        if r == 0 { q } else { q + 1 }
    }

    /// Smallest integer `>= self` as a `Rational`.
    #[inline]
    pub const fn ceil(self) -> Self {
        Self::from_integer(self.ceil_i64())
    }

    /// Truncates toward zero.
    #[inline]
    pub const fn trunc(self) -> Self {
        Self::from_integer(self.num / self.den)
    }

    /// Fractional part `self - self.trunc()`.
    #[inline]
    pub fn fract(self) -> Self {
        self - self.trunc()
    }

    /// Converts `self` to `f64`.
    #[inline]
    pub fn to_f64(self) -> f64 {
        (self.num as f64) / (self.den as f64)
    }

    /// Approximates a finite `f64` as a `Rational` with denominator `<= max_den`.
    pub fn from_f64_approx(val: f64, max_den: i64) -> Option<Self> {
        if !val.is_finite() || max_den <= 0 {
            return None;
        }
        let scale = max_den.min(Self::MAX_DENOMINATOR) as f64;
        let scaled = libm_round(val * scale) as i128;
        Some(Self::checked_op_stern_brocot(scaled, scale as i128))
    }

    /// Computes exact arithmetic in `i128` space before GCD reduction; if a deeply nested
    /// polymetric denominator exceeds `MAX_DENOMINATOR = 1_000_000_007`, deterministically
    /// rationalizes via Stern-Brocot continued-fraction approximation without panicking.
    pub fn checked_op_stern_brocot(num: i128, den: i128) -> Self {
        if den == 0 {
            return Self::zero();
        }
        let (mut n, mut d) = if den < 0 { (-num, -den) } else { (num, den) };
        if n == 0 {
            return Self::zero();
        }
        let g = Self::gcd_u128(n.unsigned_abs(), d.unsigned_abs()) as i128;
        n /= g;
        d /= g;

        let max_d = Self::MAX_DENOMINATOR as i128;
        if d <= max_d && n >= (i64::MIN as i128) && n <= (i64::MAX as i128) {
            return Self {
                num: n as i64,
                den: d as i64,
            };
        }

        // Continued-fraction / Stern-Brocot convergent rationalization bounded by MAX_DENOMINATOR.
        let sign: i128 = if n < 0 { -1 } else { 1 };
        let mut a = n.abs();
        let mut b = d;

        let mut p0: i128 = 0;
        let mut q0: i128 = 1;
        let mut p1: i128 = 1;
        let mut q1: i128 = 0;

        while b != 0 {
            let q = a / b;
            let r = a % b;

            let next_q = q.saturating_mul(q1).saturating_add(q0);
            let next_p = q.saturating_mul(p1).saturating_add(p0);

            if next_q > max_d || next_p > (i64::MAX as i128) {
                let k_q = if q1 > 0 { (max_d - q0) / q1 } else { 0 };
                let k_p = if p1 > 0 {
                    ((i64::MAX as i128) - p0) / p1
                } else {
                    k_q
                };
                let k = k_q.min(k_p);
                if k > 0 {
                    p1 = k * p1 + p0;
                    q1 = k * q1 + q0;
                }
                break;
            }

            p0 = p1;
            q0 = q1;
            p1 = next_p;
            q1 = next_q;

            a = b;
            b = r;
        }

        let final_den = q1.clamp(1, max_d) as i64;
        let final_num = (sign * p1).clamp(i64::MIN as i128 + 1, i64::MAX as i128) as i64;
        let g2 = Self::gcd_u128(final_num.unsigned_abs() as u128, final_den.unsigned_abs() as u128) as i64;
        Self {
            num: final_num / g2,
            den: final_den / g2,
        }
    }

    #[inline]
    const fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
        while b != 0 {
            let t = b;
            b = a % b;
            a = t;
        }
        if a == 0 { 1 } else { a }
    }
}

#[inline]
fn libm_round(x: f64) -> f64 {
    if x >= 0.0 {
        (x + 0.5) as i64 as f64
    } else {
        (x - 0.5) as i64 as f64
    }
}

impl Default for Rational {
    fn default() -> Self {
        Self::zero()
    }
}

impl Hash for Rational {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.num.hash(state);
        self.den.hash(state);
    }
}

impl PartialOrd for Rational {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Rational {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        let lhs = (self.num as i128) * (other.den as i128);
        let rhs = (other.num as i128) * (self.den as i128);
        lhs.cmp(&rhs)
    }
}

impl Add for Rational {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        let n = (self.num as i128) * (rhs.den as i128) + (rhs.num as i128) * (self.den as i128);
        let d = (self.den as i128) * (rhs.den as i128);
        Self::checked_op_stern_brocot(n, d)
    }
}

impl Sub for Rational {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        let n = (self.num as i128) * (rhs.den as i128) - (rhs.num as i128) * (self.den as i128);
        let d = (self.den as i128) * (rhs.den as i128);
        Self::checked_op_stern_brocot(n, d)
    }
}

impl Mul for Rational {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self::Output {
        let n = (self.num as i128) * (rhs.num as i128);
        let d = (self.den as i128) * (rhs.den as i128);
        Self::checked_op_stern_brocot(n, d)
    }
}

impl Div for Rational {
    type Output = Self;
    #[inline]
    fn div(self, rhs: Self) -> Self::Output {
        if rhs.num == 0 {
            return Self::zero();
        }
        let n = (self.num as i128) * (rhs.den as i128);
        let d = (self.den as i128) * (rhs.num as i128);
        Self::checked_op_stern_brocot(n, d)
    }
}

impl Rem for Rational {
    type Output = Self;
    #[inline]
    fn rem(self, rhs: Self) -> Self::Output {
        if rhs.num == 0 {
            return Self::zero();
        }
        let q = (self / rhs).trunc();
        self - q * rhs
    }
}

impl Neg for Rational {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self::Output {
        Self::checked_op_stern_brocot(-(self.num as i128), self.den as i128)
    }
}

impl From<i64> for Rational {
    #[inline]
    fn from(v: i64) -> Self {
        Self::from_integer(v)
    }
}

impl From<(i64, i64)> for Rational {
    #[inline]
    fn from((n, d): (i64, i64)) -> Self {
        Self::new_const(n, d)
    }
}

impl fmt::Display for Rational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 {
            write!(f, "{}", self.num)
        } else {
            write!(f, "{}/{}", self.num, self.den)
        }
    }
}

impl FromStr for Rational {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        if let Some((n_str, d_str)) = trimmed.split_once('/') {
            let n = n_str
                .trim()
                .parse::<i64>()
                .map_err(|_| "invalid numerator".to_string())?;
            let d = d_str
                .trim()
                .parse::<i64>()
                .map_err(|_| "invalid denominator".to_string())?;
            Self::new(n, d).ok_or_else(|| "zero denominator".to_string())
        } else if trimmed.contains('.') {
            let v = trimmed
                .parse::<f64>()
                .map_err(|_| "invalid float".to_string())?;
            Self::from_f64_approx(v, 1_000_000).ok_or_else(|| "non-finite float".to_string())
        } else {
            let n = trimmed
                .parse::<i64>()
                .map_err(|_| "invalid integer".to_string())?;
            Ok(Self::from_integer(n))
        }
    }
}

/// Conversion and cycle-boundary helpers for `Time`.
pub trait TimeExt {
    /// Returns the floor integer cycle index (`sam(t)` in Tidal / `wholeCycle` start in Strudel).
    fn sam(self) -> Time;

    /// Returns `sam(t) + 1`, the start of the next cycle (`nextSam(t)`).
    fn next_sam(self) -> Time;

    /// Returns the fractional position within the current cycle: `t - t.sam()`, in `[0, 1)`.
    fn cycle_pos(self) -> Time;

    /// Converts cycle time to wall-clock seconds given cycles-per-second (`cps`).
    fn to_seconds(self, cps: f64) -> f64;
}

impl TimeExt for Time {
    #[inline]
    fn sam(self) -> Time {
        self.floor()
    }

    #[inline]
    fn next_sam(self) -> Time {
        self.sam() + Time::one()
    }

    #[inline]
    fn cycle_pos(self) -> Time {
        self - self.sam()
    }

    #[inline]
    fn to_seconds(self, cps: f64) -> f64 {
        if cps <= 0.0 {
            0.0
        } else {
            self.to_f64() / cps
        }
    }
}
