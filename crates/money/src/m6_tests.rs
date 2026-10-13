//! Independent finite-domain checks for the exact conversion arithmetic.
use super::*;

fn fraction(numerator: i64, denominator: i64) -> Rational {
    Rational {
        numerator,
        denominator,
    }
}

// Quotient/remainder oracle deliberately does not use convert_unit's add-half path.
fn rounded_fraction(numerator: i128, denominator: i128) -> Result<i64> {
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    let adjustment = if remainder.abs() * 2 >= denominator {
        numerator.signum()
    } else {
        0
    };
    i64::try_from(quotient + adjustment).map_err(|_| Error("Oracle overflow".into()))
}

#[test]
fn combined_conversion_matches_exact_rational_oracle_and_sign_symmetry() -> Result<()> {
    for amount in -101..=101 {
        for (un, ud) in [(1, 1), (1, 2), (2, 3), (7, 4), (19, 13)] {
            for (fn_, fd) in [(1, 1), (2, 1), (3, 2), (11, 9), (17, 23)] {
                let actual = convert_unit(Money(amount), fraction(un, ud), fraction(fn_, fd))?;
                let exact = rounded_fraction(
                    i128::from(amount) * i128::from(un) * i128::from(fn_),
                    i128::from(ud) * i128::from(fd),
                )?;
                assert_eq!(actual, Money(exact), "{amount} * {un}/{ud} * {fn_}/{fd}");
                assert_eq!(
                    actual.0,
                    -convert_unit(Money(-amount), fraction(un, ud), fraction(fn_, fd))?.0
                );
                assert_eq!(
                    actual,
                    convert_unit(Money(amount), fraction(fn_, fd), fraction(un, ud))?,
                    "Exact legs commute before rounding"
                );
                assert_eq!(
                    actual,
                    convert_unit(Money(amount), fraction(un * 7, ud * 7), fraction(fn_, fd))?,
                    "Equivalent declared ratios produce the same money"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn conversion_rounds_once_and_does_not_promise_reversible_money() -> Result<()> {
    let half = fraction(1, 2);
    let double = fraction(2, 1);
    assert_eq!(convert_unit(Money(1), half, double)?, Money(1));
    assert_eq!(
        double.money(half.money(Money(1))?)?,
        Money(2),
        "Sequential rounding is a different convention"
    );
    assert_eq!(half.money(Money(1))?, Money(1));
    assert_eq!(half.money(Money(-1))?, Money(-1));
    assert_eq!(
        fraction(3, 2).money(fraction(2, 3).money(Money(1))?)?,
        Money(2),
        "Even exact reciprocal factors cannot undo cent rounding"
    );
    assert_eq!(
        fraction(1, 12)
            .money(Money(1_000))?
            .extend(Quantity(12_000))?,
        Money(996)
    );
    assert_eq!(
        Money(1_000).extend(Quantity(1_000))?,
        Money(1_000),
        "Unit-first prices need not preserve extended native totals"
    );
    Ok(())
}

#[test]
fn quantity_conversion_is_exact_and_validates_both_sides() -> Result<()> {
    for amount in 1..=400 {
        for (n, d) in [(1, 3), (2, 3), (3, 2), (12, 1), (7, 11)] {
            let result = fraction(n, d).quantity(Quantity(amount));
            if amount * n % d == 0 {
                let converted = result?;
                assert_eq!(converted, Quantity(amount * n / d));
                assert_eq!(fraction(d, n).quantity(converted)?, Quantity(amount));
            } else {
                assert!(result.is_err(), "No rounding may admit {amount} * {n}/{d}");
            }
        }
    }
    assert!(fraction(1, 1_000_000_000).quantity(Quantity(1)).is_err());
    assert!(fraction(12, 1).quantity(Quantity(1_000_000_000)).is_err());
    assert!(
        fraction(1, 2).quantity(Quantity(1_000_000_002)).is_err(),
        "An invalid request quantity cannot become valid by shrinking"
    );
    assert!(Rational::ONE.quantity(Quantity(0)).is_err());
    assert!(Rational::ONE.quantity(Quantity(-1)).is_err());
    Ok(())
}

#[test]
fn checked_extremes_allow_exact_cancellation_and_reject_out_of_range_ratios() -> Result<()> {
    let max = fraction(1_000_000_000, 1);
    let min = fraction(1, 1_000_000_000);
    for amount in [i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX] {
        assert_eq!(Rational::ONE.money(Money(amount))?, Money(amount));
        assert_eq!(convert_unit(Money(amount), max, min)?, Money(amount));
        assert_eq!(convert_unit(Money(amount), min, max)?, Money(amount));
    }
    assert!(convert_unit(Money(i64::MAX), max, max).is_err());
    assert!(fraction(2, 1).money(Money(i64::MIN)).is_err());
    for invalid in [
        fraction(0, 1),
        fraction(-1, 1),
        fraction(1, 0),
        fraction(1, -1),
        fraction(1_000_000_001, 1),
        fraction(1, 1_000_000_001),
    ] {
        assert!(invalid.validate().is_err());
        assert!(
            invalid.money(Money(0)).is_err(),
            "A zero amount must not hide an invalid factor"
        );
        assert!(invalid.quantity(Quantity(1_000)).is_err());
    }
    Ok(())
}
