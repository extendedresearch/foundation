//! Brute-force checks of the bounds the core states, by enumeration and by a
//! seeded generator, with no property-testing dependency.
//!
//! Each check models the true instants behind a set of readings, runs the core
//! on the readings alone, and asserts that the bound the core states contains
//! every true value — and, where the bound is claimed tight, that its edges are
//! reached.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]

use extendedresearch_clock::{
    Anchor, Basis, Bound, Domain, Reading, Rounding, SuspendBehaviour, UNBOUNDED, bound_for_read,
    ns_from_ms, quantum_from_deltas, wall_at,
};

/// SplitMix64: small, seeded, and the same on every platform.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[low, high]`.
    fn range(&mut self, low: u64, high: u64) -> u64 {
        low + self.next() % (high - low + 1)
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[(self.next() % items.len() as u64) as usize]
    }
}

fn domain(resolution_ns: Option<u64>, anchor: Option<Anchor>) -> Domain {
    Domain {
        host_id: "fixture-host".to_owned(),
        host_clock_epoch: "boot.1".to_owned(),
        monotonic_source: "CLOCK_MONOTONIC",
        suspend: SuspendBehaviour::Unspecified,
        resolution_ns,
        anchor,
    }
}

fn bound(early_ns: u64, late_ns: u64) -> Bound {
    Bound {
        early_ns,
        late_ns,
        basis: Basis::ClockRead,
    }
}

/// For every pair of readings and bounds on a small grid, the set of true
/// intervals consistent with them is exactly `[ns - below_ns, ns + above_ns]`:
/// contained, and both edges reached.
#[test]
fn interval_bounds_are_exact() {
    let id = domain(None, None).id();
    let mut cases = 0;
    for (ea, la, eb, lb) in (0..4u64).flat_map(|a| {
        (0..4u64)
            .flat_map(move |b| (0..4u64).flat_map(move |c| (0..4u64).map(move |d| (a, b, c, d))))
    }) {
        for ra in 4..10u64 {
            for rb in 4..10u64 {
                let later = Reading::new(ra, id, bound(ea, la));
                let earlier = Reading::new(rb, id, bound(eb, lb));
                let i = later.since(earlier).unwrap();
                let (low, high) = (
                    i128::from(i.ns) - i128::from(i.below_ns),
                    i128::from(i.ns) + i128::from(i.above_ns),
                );
                let (mut seen_low, mut seen_high) = (i128::MAX, i128::MIN);
                for ta in ra - ea..=ra + la {
                    for tb in rb - eb..=rb + lb {
                        let truth = i128::from(ta) - i128::from(tb);
                        assert!(
                            low <= truth && truth <= high,
                            "{ra} {rb} ({ea},{la}) ({eb},{lb}): {truth} outside [{low}, {high}]"
                        );
                        seen_low = seen_low.min(truth);
                        seen_high = seen_high.max(truth);
                        cases += 1;
                    }
                }
                assert_eq!((seen_low, seen_high), (low, high), "the bound is not tight");
            }
        }
    }
    assert_eq!(cases, 147_456);
}

/// At realistic magnitudes, a random true pair inside random bounds lies in
/// the interval, and the widths are the saturating sums.
#[test]
fn interval_bounds_contain_random_truths() {
    let id = domain(None, None).id();
    let mut rng = Rng(20_260_916);
    for _ in 0..200_000 {
        let widths = [0, 1, 100, 100_000, 16_666_667, 1 << 62, UNBOUNDED];
        let (ea, la, eb, lb) = (
            rng.pick(&widths),
            rng.pick(&widths),
            rng.pick(&widths),
            rng.pick(&widths),
        );
        let ta = rng.range(1 << 40, 1 << 50);
        let tb = rng.range(1 << 40, 1 << 50);
        // A reading r with bound (e, l) has its true instant in [r - l, r + e].
        let (ra, rb) = (
            ta - rng.range(0, la.min(1 << 39)) + rng.range(0, ea.min(1 << 39)),
            tb - rng.range(0, lb.min(1 << 39)) + rng.range(0, eb.min(1 << 39)),
        );
        let later = Reading::new(ra, id, bound(ea, la));
        let earlier = Reading::new(rb, id, bound(eb, lb));
        let i = later.since(earlier).unwrap();
        let sum =
            |a: u64, b: u64| u64::try_from(u128::from(a) + u128::from(b)).unwrap_or(UNBOUNDED);
        assert_eq!((i.below_ns, i.above_ns), (sum(ea, lb), sum(la, eb)));
        let truth = i128::from(ta) - i128::from(tb);
        let low = i128::from(i.ns) - i128::from(i.below_ns);
        let high = i128::from(i.ns) + i128::from(i.above_ns);
        assert!(low <= truth && truth <= high, "{ta} {tb} {ra} {rb}");
    }
}

/// The two rounding models of `bound_for_read`, with a fixed per-bucket
/// threshold for the two-sided clock so readings never go backwards.
fn read(t: u64, q: u64, rounding: Rounding) -> u64 {
    let bucket = t / q;
    let r = bucket * q;
    let threshold = (bucket.wrapping_mul(2_654_435_761).wrapping_add(12_345)) % q;
    match rounding {
        Rounding::FloorOnly => r,
        Rounding::TwoSided if t % q >= threshold => r + q,
        Rounding::TwoSided => r,
    }
}

const ROUNDINGS: [Rounding; 2] = [Rounding::FloorOnly, Rounding::TwoSided];

struct AnchorTally {
    anchors: u64,
    nones: u64,
    ties_that_matter: u64,
    min_slack: (u64, u64),
    mapped: u64,
}

/// One case: brackets built from true instants under one constant calendar
/// offset, run through `from_brackets` and `wall_at`, checked against the
/// truth. A `None` resolution is a clock with a real 1 µs quantum the caller
/// does not know.
#[allow(clippy::too_many_arguments)]
fn anchor_case(
    tally: &mut AnchorTally,
    qm: Option<u64>,
    mono: Rounding,
    qw: Option<u64>,
    wall: Rounding,
    offset: u64,
    trues: &[(u64, u64, u64)],
    zero_wall: &[bool],
) {
    let real_qm = qm.unwrap_or(1000);
    let real_qw = qw.unwrap_or(1000);
    let brackets: Vec<(u64, u64, u64)> = trues
        .iter()
        .zip(zero_wall)
        .map(|(&(t1, tw, t2), &zero)| {
            let w = if zero {
                0
            } else {
                read(tw + offset, real_qw, wall)
            };
            (read(t1, real_qm, mono), w, read(t2, real_qm, mono))
        })
        .collect();
    let built = Anchor::from_brackets(&brackets, qm, mono, "CLOCK_REALTIME", qw, wall);
    let kept: Vec<_> = brackets.iter().filter(|b| b.1 != 0 && b.2 >= b.0).collect();
    let Some((anchor, error)) = built else {
        assert!(kept.is_empty(), "None with a valid bracket: {brackets:?}");
        tally.nones += 1;
        return;
    };
    tally.anchors += 1;
    // Selection: the first bracket of minimum span.
    let span = kept.iter().map(|b| b.2 - b.0).min().unwrap();
    let first = kept.iter().find(|b| b.2 - b.0 == span).unwrap();
    assert_eq!(
        (
            anchor.monotonic_ns,
            anchor.wall_unix_ns,
            anchor.read_span_ns
        ),
        (first.0 + span / 2, first.1, span)
    );
    if kept
        .iter()
        .filter(|b| b.2 - b.0 == span)
        .any(|b| (b.0, b.1) != (first.0, first.1))
    {
        tally.ties_that_matter += 1;
    }
    // Containment: calendar(M) - W lies in [-early, late].
    let truth =
        i128::from(anchor.monotonic_ns) + i128::from(offset) - i128::from(anchor.wall_unix_ns);
    if qm.is_none() || qw.is_none() {
        assert_eq!(error, Bound::UNKNOWN);
    } else {
        assert_eq!(error.basis, Basis::ClockRead);
        let early = i128::from(error.early_ns);
        let late = i128::from(error.late_ns);
        assert!(
            -early <= truth && truth <= late,
            "{brackets:?}: {truth} outside [-{early}, {late}]"
        );
        tally.min_slack.0 = tally.min_slack.0.min(u64::try_from(truth + early).unwrap());
        tally.min_slack.1 = tally.min_slack.1.min(u64::try_from(late - truth).unwrap());
    }
    // The mapping: a reading of any true instant maps to a calendar range
    // containing that instant's calendar time.
    let d = domain(qm, Some(anchor));
    for &(t1, tw, t2) in trues {
        for t in [t1, tw, t2, tw + 7] {
            let r = Reading::new(
                read(t, real_qm, mono),
                d.id(),
                bound_for_read(Some(real_qm), mono),
            );
            let mapped = wall_at(&d, r, mono).unwrap();
            let calendar = i128::from(t) + i128::from(offset);
            let low = i128::from(mapped.unix_ns) - i128::from(mapped.early_ns);
            let high = i128::from(mapped.unix_ns) + i128::from(mapped.late_ns);
            assert!(
                low <= calendar && calendar <= high,
                "mapping: {calendar} outside [{low}, {high}]"
            );
            tally.mapped += 1;
        }
    }
}

/// `from_brackets` and `wall_at` over every small grid of two brackets, on all
/// four rounding combinations. On these grids the early edge is reached to
/// within 1 ns when the monotonic leg floors (its true interval is half-open)
/// and 2 ns when it is two-sided, and the late edge likewise by the wall leg.
#[test]
fn anchor_error_contains_the_truth_exhaustively() {
    for mono in ROUNDINGS {
        for wall in ROUNDINGS {
            let mut tally = AnchorTally {
                anchors: 0,
                nones: 0,
                ties_that_matter: 0,
                min_slack: (u64::MAX, u64::MAX),
                mapped: 0,
            };
            for qm in [1, 2, 3] {
                for qw in [1, 2, 4] {
                    for a1 in 0..6 {
                        for b1 in 0..5 {
                            for a2 in 0..6 {
                                for b2 in 0..5 {
                                    for off in 0..4 {
                                        let t1 = 1000 + a1;
                                        let t2 = 1010 + a2;
                                        let trues = [
                                            (t1, t1 + b1 / 2, t1 + b1),
                                            (t2, t2 + b2 / 2, t2 + b2),
                                        ];
                                        for mask in 0..4u8 {
                                            let zero = [mask & 1 != 0, mask & 2 != 0];
                                            anchor_case(
                                                &mut tally,
                                                Some(qm),
                                                mono,
                                                Some(qw),
                                                wall,
                                                1_000_000 + off,
                                                &trues,
                                                &zero,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            let expect_slack = |r| if r == Rounding::FloorOnly { 1 } else { 2 };
            assert_eq!(tally.anchors, 97_200, "{mono:?} {wall:?}");
            assert_eq!(tally.nones, 32_400, "{mono:?} {wall:?}");
            assert!(tally.ties_that_matter > 0);
            assert_eq!(
                tally.min_slack,
                (expect_slack(mono), expect_slack(wall)),
                "{mono:?} {wall:?}: the error is not reached where the model says it is"
            );
            assert!(tally.mapped > 0);
        }
    }
}

/// Realistic magnitudes, up to five brackets, `None` resolutions and forced
/// zero wall reads, on all four rounding combinations.
#[test]
fn anchor_error_contains_the_truth_at_random() {
    let mut rng = Rng(20_260_917);
    for mono in ROUNDINGS {
        for wall in ROUNDINGS {
            let mut tally = AnchorTally {
                anchors: 0,
                nones: 0,
                ties_that_matter: 0,
                min_slack: (u64::MAX, u64::MAX),
                mapped: 0,
            };
            for _ in 0..20_000 {
                let qm = rng.pick(&[
                    None,
                    Some(1),
                    Some(42),
                    Some(100),
                    Some(5_000),
                    Some(100_000),
                ]);
                let qw = rng.pick(&[None, Some(1), Some(100), Some(1_000), Some(1_000_000)]);
                let offset = rng.range(1_000_000_000_000_000, 2_000_000_000_000_000);
                let count = rng.range(1, 5) as usize;
                let mut t = rng.range(0, 1_000_000_000);
                let mut trues = Vec::new();
                let mut zero = Vec::new();
                for _ in 0..count {
                    let dw = if rng.next() % 2 == 0 {
                        0
                    } else {
                        rng.range(0, 300_000)
                    };
                    let d2 = if rng.next() % 2 == 0 {
                        0
                    } else {
                        rng.range(0, 300_000)
                    };
                    trues.push((t, t + dw, t + dw + d2));
                    zero.push(rng.next() % 100 < 15);
                    t += dw + d2 + rng.range(0, 1_000_000);
                }
                anchor_case(&mut tally, qm, mono, qw, wall, offset, &trues, &zero);
            }
            assert!(
                tally.anchors > 15_000 && tally.nones > 0,
                "{mono:?} {wall:?}"
            );
        }
    }
}

/// On a whole-µs grid, any list whose smallest delta is one quantum returns
/// that quantum, whatever the order and however many zeros are mixed in.
#[test]
fn quantum_on_exact_grids() {
    let mut rng = Rng(7);
    for _ in 0..5_000 {
        let q = rng.pick(&[5_000u64, 20_000, 100_000, 1_000_000, 16_667_000]);
        let count = rng.range(16, 40) as usize;
        let mut deltas: Vec<u64> = (0..count).map(|_| q * rng.range(1, 30)).collect();
        deltas[rng.range(0, count as u64 - 1) as usize] = q;
        for _ in 0..rng.range(0, 5) {
            deltas.insert(rng.range(0, deltas.len() as u64) as usize, 0);
        }
        assert_eq!(quantum_from_deltas(&deltas), Some(q), "{deltas:?}");
        deltas.reverse();
        assert_eq!(quantum_from_deltas(&deltas), Some(q));
    }
}

/// Order independence on arbitrary lists, including ones that return `None`.
#[test]
fn quantum_ignores_order_and_zeros() {
    let mut rng = Rng(11);
    let mut some = 0;
    for _ in 0..2_000 {
        let q = rng.range(1_000, 20_000_000);
        let count = rng.range(10, 24) as usize;
        let mut deltas: Vec<u64> = (0..count)
            .map(|_| q * rng.range(1, 9) + rng.range(0, q / 20))
            .collect();
        let expected = quantum_from_deltas(&deltas);
        some += u64::from(expected.is_some());
        let len = deltas.len() as u64;
        for i in 0..deltas.len() {
            let j = rng.range(0, len - 1) as usize;
            deltas.swap(i, j);
        }
        deltas.push(0);
        assert_eq!(quantum_from_deltas(&deltas), expected, "{deltas:?}");
    }
    assert!(some > 0);
}

/// Truncating the product is wrong on 1 594 of the first 100 000 multiples of
/// 100 µs; rounding is wrong on none of them, nor on 200 000 grid points from
/// one hour and from thirty days.
#[test]
fn ms_to_ns_on_the_100_us_grid() {
    let mut truncation_wrong = 0;
    for k in 0..100_000u64 {
        let ms = k as f64 / 10.0;
        assert_eq!(ns_from_ms(ms), Ok(k * 100_000), "{ms}");
        truncation_wrong += u64::from((ms * 1e6) as u64 != k * 100_000);
    }
    assert_eq!(truncation_wrong, 1_594);
    for start in [36_000_000u64, 25_920_000_000] {
        for k in start..start + 200_000 {
            assert_eq!(ns_from_ms(k as f64 / 10.0), Ok(k * 100_000));
        }
    }
}

/// At Unix-epoch magnitudes the conversion is off the exact value by at most
/// 128 ns over 200 000 whole milliseconds.
#[test]
fn ms_to_ns_at_epoch_scale() {
    let worst = (1_789_547_317_781u64..1_789_547_317_781 + 200_000)
        .map(|ms| ns_from_ms(ms as f64).unwrap().abs_diff(ms * 1_000_000))
        .max()
        .unwrap();
    assert_eq!(worst, 128);
}
