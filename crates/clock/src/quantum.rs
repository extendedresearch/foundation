//! Measuring a clock's quantum from the deltas between consecutive readings.

/// Minimum count of non-zero deltas [`quantum_from_deltas`] requires.
pub const QUANTUM_MIN_DELTAS: usize = 16;
/// Largest divisor `k` tried: candidate quanta go down to `d_min / 64`.
pub const QUANTUM_MAX_DIVISOR: u64 = 64;
/// Relative per-delta tolerance: 1/16 of the quantum.
pub const QUANTUM_TOL_DEN: u128 = 16;
/// Absolute per-delta slack, capped at `q/64`, and the span slack for the
/// whole-microsecond snap: two readings' worth of `ns_from_ms` error at
/// `Date.now()` magnitudes.
pub const QUANTUM_SLACK_NS: u128 = 512;
/// No candidate quantum below 1 µs.
pub const QUANTUM_FLOOR_NS: u64 = 1_000;

/// The largest quantum every non-zero delta fits, as span over step count.
///
/// The deltas are the differences between consecutive readings of one clock.
/// Zeros are ignored and the order does not matter. With `d` the non-zero
/// deltas sorted ascending, for `k = 1 ..= QUANTUM_MAX_DIVISOR` while
/// `d_min / k` is at least [`QUANTUM_FLOOR_NS`], each delta is assigned a step
/// count by rounding it half up against the running span `S` over step count
/// `N`. The first `k` — the largest quantum — under which every delta lies
/// within `q/16 + min(512 ns, q/64)` of its step count times `S/N` wins. The
/// value returned is the whole-microsecond `m` nearest `S/N` when
/// `|S - m·N| ≤ 512 ns`, else `S/N` rounded half up to the nanosecond. Every
/// intermediate is a `u128`; nothing is floating point.
///
/// When wrong, it over-reports, which keeps [`bound_for_read`] a true worst
/// case: a perfectly regular loop eight or more quanta long can return a
/// multiple of the quantum.
///
/// [`bound_for_read`]: crate::bound_for_read
///
/// ```
/// use extendedresearch_clock::quantum_from_deltas;
/// let deltas: Vec<u64> = [200_000, 300_000].repeat(8);
/// assert_eq!(quantum_from_deltas(&deltas), Some(100_000));
/// assert_eq!(quantum_from_deltas(&[100_000; 15]), None);
/// ```
///
/// `None` when fewer than [`QUANTUM_MIN_DELTAS`] deltas are non-zero, or when
/// no candidate fits every delta. No delta is discarded as an outlier.
pub fn quantum_from_deltas(deltas_ns: &[u64]) -> Option<u64> {
    let mut d: Vec<u128> = deltas_ns
        .iter()
        .filter(|&&x| x != 0)
        .map(|&x| u128::from(x))
        .collect();
    if d.len() < QUANTUM_MIN_DELTAS {
        return None;
    }
    d.sort_unstable();
    let &d_min = d.first()?;
    let mut steps: Vec<u128> = Vec::with_capacity(d.len());
    for k in 1..=u128::from(QUANTUM_MAX_DIVISOR) {
        if d_min < k * u128::from(QUANTUM_FLOOR_NS) {
            break;
        }
        if let Some(q) = candidate(&d, k, &mut steps) {
            return u64::try_from(q).ok();
        }
    }
    None
}

/// One candidate `k` over the sorted non-zero deltas `d`: the quantum when
/// every delta fits it, else `None`.
///
/// Every intermediate fits `u128` while the step count stays below 2^60, which
/// a list a clock produces keeps to. A list that drives an intermediate past
/// `u128::MAX` — a 1 µs delta beside deltas of centuries — is treated as not
/// fitting this candidate rather than wrapping or panicking.
fn candidate(d: &[u128], k: u128, steps: &mut Vec<u128>) -> Option<u128> {
    let (&d_min, rest) = d.split_first()?;
    let (mut span, mut total) = (d_min, k);
    steps.clear();
    steps.push(k);
    for &x in rest {
        // Round half up of x / (span / total).
        let n = x
            .checked_mul(2)?
            .checked_mul(total)?
            .checked_add(span)?
            .checked_div(span.checked_mul(2)?)?;
        if n == 0 {
            return None;
        }
        steps.push(n);
        span = span.checked_add(x)?;
        total = total.checked_add(n)?;
    }
    let absolute = QUANTUM_TOL_DEN
        .checked_mul(QUANTUM_SLACK_NS)?
        .checked_mul(total)?;
    let relative = QUANTUM_TOL_DEN.checked_mul(span)? / 64;
    let rhs = span.checked_add(absolute.min(relative))?;
    for (&x, &n) in d.iter().zip(steps.iter()) {
        let off = x.checked_mul(total)?.abs_diff(n.checked_mul(span)?);
        if QUANTUM_TOL_DEN.checked_mul(off)? > rhs {
            return None;
        }
    }
    let micro = span
        .checked_mul(2)?
        .checked_add(total.checked_mul(1000)?)?
        .checked_div(total.checked_mul(2000)?)?
        .checked_mul(1000)?;
    if span.abs_diff(micro.checked_mul(total)?) <= QUANTUM_SLACK_NS {
        return Some(micro);
    }
    span.checked_mul(2)?
        .checked_add(total)?
        .checked_div(total.checked_mul(2)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(list: &[u64]) -> Vec<u64> {
        list.to_vec()
    }

    #[test]
    fn a_loop_longer_than_the_quantum() {
        assert_eq!(
            quantum_from_deltas(&[200_000, 300_000].repeat(8)),
            Some(100_000)
        );
    }

    #[test]
    fn too_few_changes() {
        let mut fifteen = vec![100_000; 15];
        fifteen.push(0);
        assert_eq!(quantum_from_deltas(&fifteen), None);
        assert_eq!(
            quantum_from_deltas(&[0, 0, 100_000, 100_000, 300_000]),
            None
        );
        assert_eq!(quantum_from_deltas(&[]), None);
        assert_eq!(quantum_from_deltas(&[0; 40]), None);
    }

    #[test]
    fn a_grid_that_is_not_whole_microseconds() {
        let one_frame = [16_666_667, 16_666_666].repeat(8);
        assert_eq!(quantum_from_deltas(&one_frame), Some(16_666_667));
        let mut skipped = one_frame.clone();
        skipped.push(33_333_333);
        assert_eq!(quantum_from_deltas(&skipped), Some(16_666_667));
    }

    #[test]
    fn a_skipped_frame_and_a_pause_on_a_whole_microsecond_grid() {
        let mut deltas = vec![16_667_000; 15];
        deltas.extend([33_334_000, 300_006_000]);
        assert_eq!(quantum_from_deltas(&deltas), Some(16_667_000));
    }

    #[test]
    fn conversion_noise_snaps_to_the_microsecond() {
        let deltas = strs(&[
            0, 999_936, 0, 0, 1_000_192, 1_000_192, 999_936, 0, 1_000_192, 999_936, 999_936,
            999_936, 999_936, 1_000_192, 999_936, 999_936, 1_000_192, 0, 999_936, 999_936, 999_936,
        ]);
        assert_eq!(quantum_from_deltas(&deltas), Some(1_000_000));
        let jitter = strs(&[
            99_883, 100_170, 99_747, 100_200, 100_200, 99_600, 100_200, 100_200, 99_600, 100_400,
            99_683, 99_917, 100_200, 100_053, 99_947, 99_800,
        ]);
        assert_eq!(quantum_from_deltas(&jitter), Some(100_000));
    }

    #[test]
    fn no_common_grid_is_none_not_a_tiny_quantum() {
        let deltas: Vec<u64> = (0..16u64)
            .map(|i| 1_000_000 + (i * i * 7919) % 900_000)
            .collect();
        assert_eq!(quantum_from_deltas(&deltas), None);
    }

    #[test]
    fn order_and_zeros_do_not_matter() {
        let mut deltas = [16_666_667, 16_666_666].repeat(8);
        deltas.push(33_333_333);
        let expected = quantum_from_deltas(&deltas);
        deltas.reverse();
        assert_eq!(quantum_from_deltas(&deltas), expected);
        deltas.rotate_left(5);
        deltas.extend([0, 0, 0]);
        deltas.insert(3, 0);
        assert_eq!(quantum_from_deltas(&deltas), expected);
    }

    #[test]
    fn the_floor_stops_the_search() {
        // Deltas of 1500 ns: k = 1 gives 1500 and fits; nothing below 1 µs is tried.
        assert_eq!(quantum_from_deltas(&[1_500; 16]), Some(1_500));
        // Deltas below 1 µs have no candidate at all.
        assert_eq!(quantum_from_deltas(&[999; 16]), None);
    }

    #[test]
    fn extreme_magnitudes_do_not_overflow() {
        assert_eq!(quantum_from_deltas(&[u64::MAX; 16]), Some(u64::MAX));
        let mut mixed = vec![u64::MAX / 2; 15];
        mixed.push(1_000);
        // A 1 µs delta beside enormous ones: some candidate fits or none does,
        // and either way the arithmetic stays in u128.
        let _ = quantum_from_deltas(&mixed);
    }

    #[test]
    fn a_quantum_present_as_a_delta_is_returned() {
        // With the quantum itself among the deltas, k = 1 assigns every
        // multiple exactly, so the whole-µs grid comes back unchanged.
        for q in [5_000u64, 20_000, 100_000, 1_000_000, 16_667_000] {
            let mut deltas = vec![q];
            deltas.extend((0..20u64).map(|i| q * (1 + (i * 7) % 5)));
            assert_eq!(quantum_from_deltas(&deltas), Some(q), "q = {q}");
        }
    }
}
