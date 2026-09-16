//! Units, the unbounded sentinel, and the one float-to-integer conversion.

use std::fmt;

/// No bound is known in this direction.
///
/// Every width is `u64` nanoseconds, and this value is the only one that does
/// not mean a width. Width addition saturates to it: `UNBOUNDED` plus anything
/// is `UNBOUNDED`, and two finite widths whose sum passes `u64::MAX` are
/// `UNBOUNDED` too, because a bound that wide is not one a caller can act on.
pub const UNBOUNDED: u64 = u64::MAX;

/// Adds two widths, saturating to [`UNBOUNDED`].
///
/// `saturating_add` already maps both `UNBOUNDED + x` and a finite overflow to
/// `u64::MAX`, which is the sentinel, so no separate check is needed.
pub(crate) fn widths(a: u64, b: u64) -> u64 {
    a.saturating_add(b)
}

/// Why [`ns_from_ms`] refused a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MsError {
    /// NaN, or positive or negative infinity.
    NotFinite,
    /// Less than zero. `-0.0` is not negative.
    Negative,
    /// The rounded nanosecond count does not fit `u64`.
    OutOfRange,
}

impl fmt::Display for MsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            MsError::NotFinite => "the millisecond value is not finite",
            MsError::Negative => "the millisecond value is negative",
            MsError::OutOfRange => "the millisecond value does not fit u64 nanoseconds",
        })
    }
}

impl std::error::Error for MsError {}

/// `2^64` as an `f64`, which is exact. Every rounded value below it fits `u64`.
const TWO_POW_64: f64 = 18_446_744_073_709_551_616.0;

/// `f64` milliseconds to `u64` nanoseconds, rounded to nearest.
///
/// The result is `(ms * 1e6).round()`: one multiply, then round half away from
/// zero. Rounding rather than truncating, because `4.1 * 1e6` is
/// `4099999.9999999995` in `f64`, and truncation would read 4.1 ms as
/// 4 099 999 ns.
///
/// ```
/// use extendedresearch_clock::{ns_from_ms, MsError};
/// assert_eq!(ns_from_ms(4.1), Ok(4_100_000));
/// assert_eq!(ns_from_ms(-0.0), Ok(0));
/// assert_eq!(ns_from_ms(-1.0), Err(MsError::Negative));
/// assert_eq!(ns_from_ms(f64::NAN), Err(MsError::NotFinite));
/// ```
///
/// # Errors
///
/// [`MsError::NotFinite`] for NaN and either infinity, [`MsError::Negative`]
/// for a value below zero, and [`MsError::OutOfRange`] when the rounded value
/// is `2^64` or more.
pub fn ns_from_ms(ms: f64) -> Result<u64, MsError> {
    if !ms.is_finite() {
        return Err(MsError::NotFinite);
    }
    if ms < 0.0 {
        return Err(MsError::Negative);
    }
    let rounded = (ms * 1e6).round();
    if rounded >= TWO_POW_64 {
        return Err(MsError::OutOfRange);
    }
    // `rounded` is a non-negative integer-valued float below 2^64, so the cast
    // is exact.
    Ok(rounded as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_saturate_to_unbounded() {
        assert_eq!(widths(1, 2), 3);
        assert_eq!(widths(UNBOUNDED, 0), UNBOUNDED);
        assert_eq!(widths(0, UNBOUNDED), UNBOUNDED);
        assert_eq!(widths(1 << 63, 1 << 63), UNBOUNDED);
        assert_eq!(widths(u64::MAX - 1, 1), UNBOUNDED);
        assert_eq!(widths(u64::MAX - 2, 1), u64::MAX - 1);
    }

    #[test]
    fn rounds_where_truncation_is_wrong() {
        assert_eq!(ns_from_ms(4.1), Ok(4_100_000));
        assert_eq!(ns_from_ms(1.005), Ok(1_005_000));
        assert_eq!(ns_from_ms(16.666666666666668), Ok(16_666_667));
    }

    #[test]
    fn refuses_what_is_not_a_time() {
        assert_eq!(ns_from_ms(f64::NAN), Err(MsError::NotFinite));
        assert_eq!(ns_from_ms(f64::INFINITY), Err(MsError::NotFinite));
        assert_eq!(ns_from_ms(f64::NEG_INFINITY), Err(MsError::NotFinite));
        assert_eq!(ns_from_ms(-1e-300), Err(MsError::Negative));
        assert_eq!(ns_from_ms(-0.0), Ok(0));
        assert_eq!(ns_from_ms(0.0), Ok(0));
    }

    #[test]
    fn the_top_of_the_range() {
        // The double nearest 2^64 ns, in ms: its product is exactly 2^64.
        assert_eq!(
            ns_from_ms(f64::from_bits(0x42b0_c6f7_a0b5_ed8d)),
            Err(MsError::OutOfRange)
        );
        assert_eq!(ns_from_ms(f64::MAX), Err(MsError::OutOfRange));
        // The next double down is the largest millisecond value that fits.
        assert_eq!(
            ns_from_ms(f64::from_bits(0x42b0_c6f7_a0b5_ed8c)),
            Ok(18_446_744_073_709_547_520)
        );
    }

    #[test]
    fn half_rounds_away_from_zero() {
        // `0.0000025 * 1e6` is exactly 2.5 in f64; `0.0000004 * 1e6` is just
        // under 0.4.
        assert_eq!(ns_from_ms(0.0000025), Ok(3));
        assert_eq!(ns_from_ms(0.0000004), Ok(0));
    }

    #[test]
    fn errors_display() {
        assert!(MsError::NotFinite.to_string().contains("finite"));
        assert!(MsError::Negative.to_string().contains("negative"));
        assert!(MsError::OutOfRange.to_string().contains("fit"));
    }
}
