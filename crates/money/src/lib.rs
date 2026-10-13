//! Exact fixed-point amounts. No floating point enters pricing calculations.
#[cfg(test)]
mod m6_tests;

use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Error(pub String);
pub type Result<T> = std::result::Result<T, Error>;

/// Monetary minor units (two decimals for supported USD/EUR/GBP).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Money(pub i64);
/// Hundredths of one percent: 1250 = 12.50%.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Bps(pub i32);
/// Thousandths of the product's declared unit. 1500 = 1.500 units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Quantity(pub i64);

fn bad(s: &str) -> Error {
    Error(s.into())
}
fn fixed(input: &str, decimals: usize) -> Result<i64> {
    if input.is_empty() || input.len() > 24 || input.trim() != input {
        return Err(bad("Use a plain decimal without spaces"));
    }
    let (negative, body) = match input.strip_prefix('-') {
        Some(v) => (true, v),
        None => (false, input),
    };
    let mut pieces = body.split('.');
    let whole = pieces.next().ok_or_else(|| bad("Missing decimal"))?;
    let frac = pieces.next().unwrap_or("");
    if pieces.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|c| c.is_ascii_digit())
        || !frac.bytes().all(|c| c.is_ascii_digit())
        || frac.len() > decimals
    {
        return Err(bad("Invalid decimal or too many decimal places"));
    }
    let scale = 10_i128.pow(decimals as u32);
    let base = whole
        .parse::<i128>()
        .map_err(|_| bad("Decimal too large"))?;
    let fraction = if frac.is_empty() {
        0
    } else {
        frac.parse::<i128>().map_err(|_| bad("Invalid fraction"))?
    };
    let raw = base
        .checked_mul(scale)
        .and_then(|v| v.checked_add(fraction * 10_i128.pow((decimals - frac.len()) as u32)))
        .ok_or_else(|| bad("Decimal overflow"))?;
    i64::try_from(if negative { -raw } else { raw }).map_err(|_| bad("Decimal overflow"))
}
/// Round half away from zero, including rebates/negative values.
pub fn ratio(value: i64, numerator: i64, denominator: i64) -> Result<i64> {
    if denominator <= 0 {
        return Err(bad("Denominator must be positive"));
    }
    let value = i128::from(value) * i128::from(numerator);
    let half = i128::from(denominator) / 2;
    let rounded = if value < 0 {
        -((-value + half) / i128::from(denominator))
    } else {
        (value + half) / i128::from(denominator)
    };
    i64::try_from(rounded).map_err(|_| bad("Amount overflow"))
}
/// Positive declared rational. Components are bounded so two-leg unit/FX products
/// remain predictable; all arithmetic still checks for overflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rational {
    pub numerator: i64,
    pub denominator: i64,
}
impl Rational {
    pub const ONE: Self = Self {
        numerator: 1,
        denominator: 1,
    };
    pub fn validate(self) -> Result<()> {
        if !(1..=1_000_000_000).contains(&self.numerator)
            || !(1..=1_000_000_000).contains(&self.denominator)
        {
            return Err(bad(
                "Rational components must be positive and at most 1,000,000,000",
            ));
        }
        Ok(())
    }
    pub fn money(self, amount: Money) -> Result<Money> {
        convert_unit(amount, self, Self::ONE)
    }
    /// Quantity conversion never rounds: both quantities must be exact thousandths.
    pub fn quantity(self, amount: Quantity) -> Result<Quantity> {
        self.validate()?;
        amount.validate()?;
        let n = i128::from(amount.0)
            .checked_mul(i128::from(self.numerator))
            .ok_or_else(|| bad("Quantity conversion overflow"))?;
        let d = i128::from(self.denominator);
        if n % d != 0 {
            return Err(bad(
                "Converted quantity is not an exact thousandth of the native unit",
            ));
        }
        let result =
            Quantity(i64::try_from(n / d).map_err(|_| bad("Quantity conversion overflow"))?);
        result.validate()?;
        Ok(result)
    }
}
/// Multiply both declared legs exactly and round ONCE to cents, half away from zero.
pub fn convert_unit(amount: Money, uom: Rational, fx: Rational) -> Result<Money> {
    uom.validate()?;
    fx.validate()?;
    let n = i128::from(amount.0)
        .checked_mul(i128::from(uom.numerator))
        .and_then(|v| v.checked_mul(i128::from(fx.numerator)))
        .ok_or_else(|| bad("Conversion numerator overflow"))?;
    let d = i128::from(uom.denominator)
        .checked_mul(i128::from(fx.denominator))
        .ok_or_else(|| bad("Conversion denominator overflow"))?;
    let magnitude = n
        .checked_abs()
        .and_then(|v| v.checked_add(d / 2))
        .ok_or_else(|| bad("Conversion rounding overflow"))?
        / d;
    let rounded = if n < 0 { -magnitude } else { magnitude };
    i64::try_from(rounded)
        .map(Money)
        .map_err(|_| bad("Converted amount overflow"))
}
impl Money {
    #[allow(clippy::should_implement_trait)]
    pub fn add(self, rhs: Self) -> Result<Self> {
        self.0
            .checked_add(rhs.0)
            .map(Self)
            .ok_or_else(|| bad("Amount overflow"))
    }
    #[allow(clippy::should_implement_trait)]
    pub fn sub(self, rhs: Self) -> Result<Self> {
        self.0
            .checked_sub(rhs.0)
            .map(Self)
            .ok_or_else(|| bad("Amount overflow"))
    }
    pub fn percent(self, rate: Bps) -> Result<Self> {
        ratio(self.0, i64::from(rate.0), 10_000).map(Self)
    }
    pub fn extend(self, quantity: Quantity) -> Result<Self> {
        ratio(self.0, quantity.0, 1000).map(Self)
    }
    pub fn nonnegative(self) -> Result<Self> {
        if self.0 < 0 || self.0 > 1_000_000_000_000 {
            Err(bad("Amount must be between 0 and 10 billion"))
        } else {
            Ok(self)
        }
    }
}
impl FromStr for Money {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        fixed(s, 2).map(Self)
    }
}
impl FromStr for Quantity {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let q = Self(fixed(s, 3)?);
        q.validate()?;
        Ok(q)
    }
}
impl Quantity {
    pub fn validate(self) -> Result<()> {
        if self.0 <= 0 || self.0 > 1_000_000_000 {
            Err(bad("Quantity must be positive and at most 1,000,000 units"))
        } else {
            Ok(())
        }
    }
}
impl Bps {
    pub fn validate(self) -> Result<()> {
        if !(0..=10_000).contains(&self.0) {
            Err(bad("Percent must be between 0 and 100"))
        } else {
            Ok(())
        }
    }
}
impl FromStr for Bps {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let n = fixed(s, 2)?;
        let n = Self(i32::try_from(n).map_err(|_| bad("Percent overflow"))?);
        n.validate()?;
        Ok(n)
    }
}
fn display_fixed(f: &mut fmt::Formatter<'_>, raw: i64, scale: i64, width: usize) -> fmt::Result {
    let raw = i128::from(raw);
    write!(
        f,
        "{}{}.{:0width$}",
        if raw < 0 { "-" } else { "" },
        raw.abs() / i128::from(scale),
        raw.abs() % i128::from(scale),
        width = width
    )
}
impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_fixed(f, self.0, 100, 2)
    }
}
impl fmt::Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_fixed(f, self.0, 1000, 3)
    }
}
impl fmt::Display for Bps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_fixed(f, i64::from(self.0), 100, 2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_roundtrip_and_invariants() -> Result<()> {
        for n in -20_000..20_000 {
            let m = Money(n);
            assert_eq!(m.to_string().parse::<Money>()?, m);
            assert_eq!(m.percent(Bps(10_000))?, m);
            assert_eq!(m.extend(Quantity(1000))?, m);
            assert_eq!(m.percent(Bps(5000))?.0, -Money(-n).percent(Bps(5000))?.0);
        }
        assert_eq!(Money(1).percent(Bps(5000))?, Money(1));
        assert_eq!(Money(-1).percent(Bps(5000))?, Money(-1));
        Ok(())
    }
    #[test]
    fn hostile_input() {
        for s in [
            "",
            "NaN",
            "inf",
            "1e9",
            "1.001",
            "1,000",
            " 5",
            "+1",
            "99999999999999999999999",
        ] {
            assert!(s.parse::<Money>().is_err(), "{s}");
        }
        assert!(Money(i64::MAX).add(Money(1)).is_err());
        assert!(Money(i64::MAX).extend(Quantity(i64::MAX)).is_err());
        assert!("0".parse::<Quantity>().is_err());
        assert!("100.01".parse::<Bps>().is_err());
    }
}
