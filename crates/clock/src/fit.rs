//! Fitting a source clock onto a host clock from one-way observations, and the
//! integer mapping the fitted line defines.
//!
//! # The estimator
//!
//! An observation pairs the time a source stamped on a message with the time
//! the host received it. Its `delta = receipt − source` is the clock offset plus
//! the transport delay, and the delay is never negative, so the **minimum**
//! delta is the least-contaminated estimate of the offset: the message that
//! queued least carries the most signal. That minimum is [`Fit::offset_ns`],
//! and the source instant of the observation holding it is the line's origin,
//! [`Fit::reference_source_ns`].
//!
//! The skew is the slope of the lower envelope: the best observation in the
//! early half against the best in the late half, over the source time between
//! them. Fitting the envelope rather than least-squares over every point keeps
//! a burst of delayed messages from dragging the slope.
//!
//! # What one-way data cannot do
//!
//! **It cannot separate a constant transport delay from a clock offset.** A
//! source five milliseconds away with a perfect clock produces the same
//! observations as one beside the host whose clock is five milliseconds
//! behind. The fitted offset is therefore at most the true offset plus the
//! smallest delay, never less: the mapping never places an event too early,
//! and nothing bounds how late. Every [`Fit`] states this as
//! [`Fit::bias_low_ns`] `=` [`UNBOUNDED`] and [`Fit::bias_high_ns`] `= 0`. That
//! is the measured answer for this method, not a placeholder; bounding the
//! bias needs a round trip or a marker the source records in its own timeline.
//!
//! # Batch and streaming
//!
//! [`fit_one_way`] fits a slice. [`SlidingWindow`] holds the observations of a
//! caller-chosen span of source time for a live stream, and [`Subsample`]
//! holds a uniform thinning of a whole session for a recorder; both fit by
//! calling [`fit_one_way`] on exactly what they hold, so either one's fit is
//! identical to the batch fit over the same observations in the same order.
//!
//! # Integers, and the one float
//!
//! Every input and output is an integer. The two spreads — the residual spread,
//! and the envelope spread the skew uncertainty is derived from — are root mean
//! squares accumulated in IEEE-754 binary64, in observation order, and truncated
//! to whole nanoseconds. That is the one place floating point enters, and it is
//! kept because fits already recorded were computed that way: an exact integer
//! square root differs from it once the sum of squares passes 2^53. Another
//! implementation reproduces it by the same operations in the same order,
//! which the conformance vectors check.

use std::collections::VecDeque;

use crate::units::UNBOUNDED;

/// Below this many observations a rate cannot be estimated, and the fit
/// reports an offset only.
pub const MIN_OBSERVATIONS_FOR_SKEW: usize = 8;

/// The skew uncertainty of a fit whose window was too short, or whose two
/// envelope points spanned no source time, to estimate a rate.
///
/// A zero skew that means "not estimated" and a zero skew that means
/// "measured, and the clocks agree" are the same number; this is what tells
/// them apart.
pub const SKEW_NOT_ESTIMATED: u64 = u64::MAX;

/// The residual spread above which a fit is called degraded, for a stream that
/// declares no rate: 5 ms.
///
/// The right tolerance is one sample period of the stream the fit serves
/// ([`tolerance_for_rate`]), because five milliseconds is one period at
/// 200 Hz and five periods at 1 kHz.
pub const DEFAULT_TOLERANCE_NS: u64 = 5_000_000;

/// The span of source time [`SlidingWindow::default`] keeps: 60 s.
///
/// Long enough that the two envelope points of a skew fit sit tens of seconds
/// apart, which is what makes a slope of a few parts per million measurable
/// above transport jitter; short enough to follow a rate that wanders with
/// temperature over minutes.
pub const DEFAULT_WINDOW_NS: u64 = 60_000_000_000;

/// The observation count [`SlidingWindow::default`] keeps at most: 65 536,
/// which is 1 MiB of observations.
///
/// Sixty seconds of a 1 kHz stream is 60 000 observations, so the default
/// window is bounded by time up to about 1.09 kHz and by this count above it.
pub const DEFAULT_WINDOW_CAPACITY: usize = 1 << 16;

const BILLION: i128 = 1_000_000_000;

/// One pairing of a source clock reading with the host instant it was
/// received at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Observation {
    /// The source's own timestamp, on the source's clock.
    pub source_ns: u64,
    /// When the host took the message off the wire, on the host's clock.
    pub receipt_ns: u64,
}

impl Observation {
    /// `receipt − source`: the offset plus the transport delay.
    ///
    /// Computed in 128 bits and truncated to `i64` in two's complement, so a
    /// difference outside `i64` wraps rather than panics.
    pub fn delta_ns(self) -> i64 {
        (i128::from(self.receipt_ns) - i128::from(self.source_ns)) as i64
    }
}

/// A fitted line from a source clock onto a host clock: an origin, an offset
/// at it, and a skew about it.
///
/// `host = source + offset_ns + (source − reference_source_ns) × skew_ppb / 10⁹`,
/// in the integer arithmetic [`Line::map_to_reference`] states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Line {
    /// The source instant the line is fitted about. Offset and skew describe a
    /// line only together with it.
    pub reference_source_ns: u64,
    /// Host minus source at the origin.
    pub offset_ns: i64,
    /// The rate difference in parts per billion, which is nanoseconds per
    /// second.
    pub skew_ppb: i64,
}

impl Line {
    /// A source-clock instant on the host clock.
    ///
    /// `elapsed = (i64)source − (i64)reference_source_ns`, saturating;
    /// `drift = (i64)((i128)elapsed × skew_ppb / 10⁹)`, truncating toward zero;
    /// then `(i64)source + offset_ns + drift`, each addition saturating, and
    /// the result clamped at zero. The `(i64)` casts reinterpret the bits, so a
    /// source at or above 2^63 reads as negative.
    ///
    /// **The clamp at zero is not invertible**: every source instant whose
    /// image falls below zero maps to zero.
    ///
    /// ```
    /// use extendedresearch_clock::Line;
    /// let line = Line { reference_source_ns: 1_000_000_000, offset_ns: 5_000_000, skew_ppb: 100 };
    /// // Ten seconds past the origin at 100 ns/s is 1 000 ns of drift.
    /// assert_eq!(line.map_to_reference(11_000_000_000), 11_005_001_000);
    /// ```
    pub fn map_to_reference(&self, source_ns: u64) -> u64 {
        let elapsed = (source_ns as i64).saturating_sub(self.reference_source_ns as i64);
        let drift = (i128::from(elapsed) * i128::from(self.skew_ppb) / BILLION) as i64;
        (source_ns as i64)
            .saturating_add(self.offset_ns)
            .saturating_add(drift)
            .max(0) as u64
    }

    /// A host-clock instant on the source clock.
    ///
    /// `(host − offset_ns) × 10⁹ + reference_source_ns × skew_ppb`, divided by
    /// `10⁹ + skew_ppb`, all in `i128`, rounded half away from zero and
    /// clamped to `[0, u64::MAX]`. A skew at or below −10⁹ describes a clock
    /// running backwards and leaves the instant unmapped.
    ///
    /// The forward map truncates its drift and this rounds, so an instant
    /// mapped forward and back can land one nanosecond from where it started.
    pub fn inverse_map(&self, reference_ns: u64) -> u64 {
        let denominator = BILLION + i128::from(self.skew_ppb);
        if denominator <= 0 {
            return reference_ns;
        }
        let numerator = (i128::from(reference_ns) - i128::from(self.offset_ns)) * BILLION
            + i128::from(self.reference_source_ns) * i128::from(self.skew_ppb);
        let half = denominator / 2;
        let rounded = if numerator >= 0 {
            (numerator + half) / denominator
        } else {
            (numerator - half) / denominator
        };
        rounded.clamp(0, i128::from(u64::MAX)) as u64
    }
}

/// A one-way min-filter fit, with the precision and the accuracy it can
/// state.
///
/// The field names and units are those a clock-mapping record carries, so a
/// fit is copied into one field for field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fit {
    /// The source instant of the first observation holding the minimum delta.
    pub reference_source_ns: u64,
    /// The minimum delta: host minus source at the origin.
    pub offset_ns: i64,
    /// The slope of the lower envelope in parts per billion, which is
    /// nanoseconds per second. Zero when [`Self::skew_uncertainty_ppb`] is
    /// [`SKEW_NOT_ESTIMATED`].
    pub skew_ppb: i64,
    /// How uncertain the skew is, in parts per billion, or
    /// [`SKEW_NOT_ESTIMATED`].
    pub skew_uncertainty_ppb: u64,
    /// The root mean square of the residuals about the fitted line.
    /// **Precision, not accuracy**: it is the variance the fit removed, and
    /// says nothing about how far the line sits from the truth.
    pub residual_spread_ns: u64,
    /// How many observations the fit rests on.
    pub observation_count: u64,
    /// How far below the fitted offset the true offset may lie: always
    /// [`UNBOUNDED`] for one-way data.
    pub bias_low_ns: u64,
    /// How far above the fitted offset the true offset may lie: always `0` for
    /// one-way data, because delay is never negative.
    pub bias_high_ns: u64,
}

impl Fit {
    /// The line this fit maps with.
    pub fn line(&self) -> Line {
        Line {
            reference_source_ns: self.reference_source_ns,
            offset_ns: self.offset_ns,
            skew_ppb: self.skew_ppb,
        }
    }

    /// Whether a rate was estimated.
    pub fn skew_estimated(&self) -> bool {
        self.skew_uncertainty_ppb != SKEW_NOT_ESTIMATED
    }

    /// Whether the residual spread is at most `tolerance_ns`: the line between
    /// a fit called ok and one called degraded.
    pub fn is_within(&self, tolerance_ns: u64) -> bool {
        self.residual_spread_ns <= tolerance_ns
    }
}

/// The tolerance a stream's nominal rate implies: one sample period,
/// `10¹² / nominal_rate_millihz` nanoseconds, truncated.
///
/// `None` for a stream that declares no rate, which is the signal to use
/// [`DEFAULT_TOLERANCE_NS`] and to know that is what happened.
///
/// ```
/// use extendedresearch_clock::tolerance_for_rate;
/// assert_eq!(tolerance_for_rate(200_000), Some(5_000_000)); // 200 Hz
/// assert_eq!(tolerance_for_rate(0), None);
/// ```
pub fn tolerance_for_rate(nominal_rate_millihz: u64) -> Option<u64> {
    if nominal_rate_millihz == 0 {
        return None;
    }
    Some(1_000_000_000_000 / nominal_rate_millihz)
}

/// Fit a one-way min-filter line to observations in receipt order.
///
/// `None` when there are none. The skew uses the order the observations are
/// given in to split them into halves, so observations handed over out of
/// order still give the right offset and a meaningless skew.
///
/// ```
/// use extendedresearch_clock::{Observation, fit_one_way};
/// let observations: Vec<Observation> = (0..8u64)
///     .map(|i| Observation { source_ns: i * 1_000_000_000, receipt_ns: i * 1_000_000_100 + 1_000 })
///     .collect();
/// let fit = fit_one_way(&observations).unwrap();
/// assert_eq!((fit.offset_ns, fit.skew_ppb), (1_000, 100));
/// assert!(fit_one_way(&[]).is_none());
/// ```
///
/// # Arithmetic
///
/// - `offset_ns` is the minimum [`Observation::delta_ns`];
///   `reference_source_ns` is the source of the first observation holding it.
/// - With fewer than [`MIN_OBSERVATIONS_FOR_SKEW`] observations, `skew_ppb` is
///   `0` and `skew_uncertainty_ppb` is [`SKEW_NOT_ESTIMATED`]. Otherwise the
///   first `n / 2` are the early half and the rest the late half; in each, the
///   first observation holding its minimum delta is its envelope point. With
///   `span` the late point's source minus the early point's, in `i128`, a
///   `span` of zero or less estimates no skew; otherwise
///   `skew_ppb = (i64)((late.delta − early.delta) × 10⁹ / span)` and
///   `skew_uncertainty_ppb = (u64)(2 × envelope_spread × 10⁹ / span)`, each
///   truncating toward zero, `envelope_spread` being the spread of
///   `delta − offset_ns` over every observation.
/// - The residual of an observation is `delta − (offset_ns + drift)`, where
///   `drift = (i64)((source − reference_source_ns) × skew_ppb / 10⁹)` in
///   `i128`, the addition saturates and the subtraction wraps.
/// - A spread over values `r` is `(u64)sqrt(Σ r² / n)` in binary64: each `r`
///   converted to the nearest binary64, squared, and added to a running sum
///   from `0.0` in observation order; the sum divided by `n` as a binary64;
///   the square root correctly rounded; truncated toward zero.
pub fn fit_one_way(observations: &[Observation]) -> Option<Fit> {
    let best = observations.iter().min_by_key(|o| o.delta_ns())?;
    let offset_ns = best.delta_ns();
    let reference_source_ns = best.source_ns;

    let (skew_ppb, skew_uncertainty_ppb) = envelope_slope(observations, offset_ns);

    let residual_spread_ns = spread(observations.iter().map(|o| {
        let elapsed = i128::from(o.source_ns) - i128::from(reference_source_ns);
        let drift = (elapsed.wrapping_mul(i128::from(skew_ppb)) / BILLION) as i64;
        o.delta_ns().wrapping_sub(offset_ns.saturating_add(drift))
    }));

    Some(Fit {
        reference_source_ns,
        offset_ns,
        skew_ppb,
        skew_uncertainty_ppb,
        residual_spread_ns,
        observation_count: observations.len() as u64,
        bias_low_ns: UNBOUNDED,
        bias_high_ns: 0,
    })
}

/// The slope of the lower envelope and its uncertainty, or `(0,
/// SKEW_NOT_ESTIMATED)`.
fn envelope_slope(observations: &[Observation], min_delta_ns: i64) -> (i64, u64) {
    const NOT_ESTIMATED: (i64, u64) = (0, SKEW_NOT_ESTIMATED);
    if observations.len() < MIN_OBSERVATIONS_FOR_SKEW {
        return NOT_ESTIMATED;
    }
    let (early, late) = observations.split_at(observations.len() / 2);
    let (Some(early_best), Some(late_best)) = (
        early.iter().min_by_key(|o| o.delta_ns()),
        late.iter().min_by_key(|o| o.delta_ns()),
    ) else {
        return NOT_ESTIMATED;
    };

    let span_ns = i128::from(late_best.source_ns) - i128::from(early_best.source_ns);
    if span_ns <= 0 {
        // Two envelope points spanning no source time give no slope, and a
        // zero here would read as "measured, and the clocks agree".
        return NOT_ESTIMATED;
    }
    let rise = i128::from(late_best.delta_ns()) - i128::from(early_best.delta_ns());
    let skew_ppb = (rise * BILLION / span_ns) as i64;

    // Each envelope point is off by about the spread, so a slope over `span`
    // is off by about `2σ / span`. The two errors add rather than combine in
    // quadrature: both are minima drawn from one delay distribution, so they
    // are one-sided and not independent, and the larger figure is the honest
    // one when that is wrong.
    let envelope_spread = spread(
        observations
            .iter()
            .map(|o| o.delta_ns().wrapping_sub(min_delta_ns)),
    );
    let uncertainty = (2 * i128::from(envelope_spread) * BILLION / span_ns) as u64;
    (skew_ppb, uncertainty)
}

/// The root mean square of `values`, in the binary64 order the module header
/// states.
fn spread(values: impl ExactSizeIterator<Item = i64>) -> u64 {
    let count = values.len();
    let mut sum_squares: f64 = 0.0;
    for value in values {
        let value = value as f64;
        sum_squares += value * value;
    }
    (sum_squares / count as f64).sqrt() as u64
}

/// The observations of one stream for a live fit: those whose source time is
/// within `window_ns` of the latest seen, at most `capacity` of them.
///
/// [`SlidingWindow::fit`] is [`fit_one_way`] over [`SlidingWindow::observations`],
/// so it is identical to the batch fit over the same window. The fit is cached
/// until the next [`SlidingWindow::push`]; computing it is linear in the window
/// length, so a caller fitting a fast stream refits on its own schedule — once
/// a second, say — and maps each sample with the [`Line`] it kept.
///
/// ```
/// use extendedresearch_clock::{Observation, SlidingWindow};
/// let mut window = SlidingWindow::new(10_000_000_000, 1_024);
/// assert_eq!(window.map_to_reference(42), 42); // nothing fitted: the identity
/// window.push(Observation { source_ns: 1_000, receipt_ns: 6_000 });
/// assert_eq!(window.map_to_reference(2_000), 7_000);
/// ```
#[derive(Clone, Debug)]
pub struct SlidingWindow {
    window_ns: u64,
    capacity: usize,
    kept: VecDeque<Observation>,
    latest_source_ns: Option<u64>,
    fitted: Option<Option<Fit>>,
}

impl Default for SlidingWindow {
    /// [`DEFAULT_WINDOW_NS`] and [`DEFAULT_WINDOW_CAPACITY`].
    fn default() -> Self {
        Self::new(DEFAULT_WINDOW_NS, DEFAULT_WINDOW_CAPACITY)
    }
}

impl SlidingWindow {
    /// A window over `window_ns` of source time holding at most `capacity`
    /// observations. A capacity of zero keeps nothing.
    pub fn new(window_ns: u64, capacity: usize) -> Self {
        SlidingWindow {
            window_ns,
            capacity,
            kept: VecDeque::new(),
            latest_source_ns: None,
            fitted: None,
        }
    }

    /// Add an observation, in receipt order.
    ///
    /// Then, from the oldest end only, drop observations whose source time is
    /// more than `window_ns` before the latest source time pushed so far, and
    /// drop the oldest while more than `capacity` remain. An observation that
    /// arrives with an old source time stays until the ones received before it
    /// have gone.
    pub fn push(&mut self, observation: Observation) {
        self.fitted = None;
        if self.capacity == 0 {
            return;
        }
        let latest = self
            .latest_source_ns
            .map_or(observation.source_ns, |l| l.max(observation.source_ns));
        self.latest_source_ns = Some(latest);
        self.kept.push_back(observation);
        while self.kept.front().is_some_and(|front| {
            u128::from(front.source_ns) + u128::from(self.window_ns) < u128::from(latest)
        }) {
            self.kept.pop_front();
        }
        while self.kept.len() > self.capacity {
            self.kept.pop_front();
        }
    }

    /// Forget every observation, as when the source's clock is known to have
    /// stepped.
    pub fn clear(&mut self) {
        self.kept.clear();
        self.latest_source_ns = None;
        self.fitted = None;
    }

    /// The observations held, oldest first.
    pub fn observations(&self) -> impl ExactSizeIterator<Item = Observation> + '_ {
        self.kept.iter().copied()
    }

    /// How many observations are held.
    pub fn len(&self) -> usize {
        self.kept.len()
    }

    /// Whether no observation is held.
    pub fn is_empty(&self) -> bool {
        self.kept.is_empty()
    }

    /// The fit over the observations held, or `None` when there are none.
    pub fn fit(&mut self) -> Option<Fit> {
        if let Some(fitted) = self.fitted {
            return fitted;
        }
        let fitted = fit_one_way(self.kept.make_contiguous());
        self.fitted = Some(fitted);
        fitted
    }

    /// A source instant on the host clock through the current fit, or the
    /// instant itself when nothing is fitted.
    ///
    /// The identity is what a mapping record whose quality is unavailable
    /// answers. [`SlidingWindow::fit`] tells the two cases apart.
    pub fn map_to_reference(&mut self, source_ns: u64) -> u64 {
        self.fit()
            .map_or(source_ns, |f| f.line().map_to_reference(source_ns))
    }

    /// A host instant on the source clock through the current fit, or the
    /// instant itself when nothing is fitted.
    pub fn inverse_map(&mut self, reference_ns: u64) -> u64 {
        self.fit()
            .map_or(reference_ns, |f| f.line().inverse_map(reference_ns))
    }
}

/// A uniform thinning of a whole session's observations in bounded memory, for
/// a fit made when the session ends.
///
/// Every `stride`-th observation pushed is kept; when `capacity` are held,
/// every second one held is dropped, starting with the second, and the stride
/// doubles. What survives spans the session rather than its first `capacity`
/// observations, which is what a skew fit needs: a slope measured over the
/// first minute of an hour is a slope over a minute. A capacity of zero keeps
/// nothing.
///
/// The `n`-th push (counting from one) is kept when `n` is a multiple of the
/// stride at that moment, so the survivors sit near, not exactly on, one grid.
#[derive(Clone, Debug)]
pub struct Subsample {
    capacity: usize,
    kept: Vec<Observation>,
    stride: u64,
    seen: u64,
}

impl Subsample {
    /// An empty subsample holding fewer than `capacity` observations.
    pub fn new(capacity: usize) -> Self {
        Subsample {
            capacity,
            kept: Vec::new(),
            stride: 1,
            seen: 0,
        }
    }

    /// Add an observation, in receipt order.
    pub fn push(&mut self, observation: Observation) {
        if self.capacity == 0 {
            return;
        }
        self.seen += 1;
        if self.seen % self.stride != 0 {
            return;
        }
        self.kept.push(observation);
        if self.kept.len() >= self.capacity {
            let mut index = 0_u64;
            self.kept.retain(|_| {
                index += 1;
                index % 2 == 1
            });
            self.stride *= 2;
        }
    }

    /// The observations held, in the order pushed.
    pub fn observations(&self) -> &[Observation] {
        &self.kept
    }

    /// One in how many pushes is kept now.
    pub fn stride(&self) -> u64 {
        self.stride
    }

    /// [`fit_one_way`] over the observations held.
    pub fn fit(&self) -> Option<Fit> {
        fit_one_way(&self.kept)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "a test that panics is reporting, not failing to handle"
)]
mod tests {
    use super::*;

    fn o(source_ns: u64, receipt_ns: u64) -> Observation {
        Observation {
            source_ns,
            receipt_ns,
        }
    }

    /// Observations from a source whose clock is `offset_ns` behind and runs
    /// at `skew_ppb`, through a link with `floor_ns` of delay plus jitter.
    fn synthetic(
        count: u64,
        period_ns: u64,
        offset_ns: i64,
        skew_ppb: i64,
        floor_ns: u64,
        jitter: &[u64],
    ) -> Vec<Observation> {
        (0..count)
            .map(|index| {
                let source_ns = 1_000_000_000 + index * period_ns;
                let elapsed = i128::from(source_ns) - 1_000_000_000;
                let drift = elapsed * i128::from(skew_ppb) / BILLION;
                let jitter_ns = jitter
                    .get((index % jitter.len() as u64) as usize)
                    .copied()
                    .unwrap();
                let receipt = i128::from(source_ns)
                    + i128::from(offset_ns)
                    + drift
                    + i128::from(floor_ns)
                    + i128::from(jitter_ns);
                o(source_ns, receipt as u64)
            })
            .collect()
    }

    #[test]
    fn a_known_offset_is_recovered_from_the_minimum() {
        let observations = synthetic(500, 5_000_000, 3_000_000, 0, 0, &[0, 900_000, 4_000_000]);
        let fit = fit_one_way(&observations).unwrap();
        assert_eq!(fit.offset_ns, 3_000_000);
        assert_eq!(fit.observation_count, 500);
    }

    #[test]
    fn a_constant_delay_is_indistinguishable_from_an_offset() {
        let behind = synthetic(200, 5_000_000, 5_000_000, 0, 0, &[0, 100_000]);
        let far = synthetic(200, 5_000_000, 0, 0, 5_000_000, &[0, 100_000]);
        let (a, b) = (fit_one_way(&behind).unwrap(), fit_one_way(&far).unwrap());
        assert_eq!(a, b);
        assert_eq!((a.bias_low_ns, a.bias_high_ns), (UNBOUNDED, 0));
    }

    #[test]
    fn a_known_skew_is_recovered_within_a_stated_ceiling() {
        let observations = synthetic(
            1_000,
            5_000_000,
            1_000_000,
            40_000,
            0,
            &[0, 50_000, 120_000],
        );
        let fit = fit_one_way(&observations).unwrap();
        // A ceiling stated here rather than taken from the fit's own
        // uncertainty, so the fit cannot decide whether it passed: 5 ppm
        // against a 40 ppm skew.
        assert!(fit.skew_ppb.abs_diff(40_000) <= 5_000, "{}", fit.skew_ppb);
        assert!(fit.skew_estimated());
    }

    #[test]
    fn too_few_observations_report_no_skew_rather_than_zero_skew() {
        let fit = fit_one_way(&synthetic(4, 5_000_000, 0, 0, 0, &[0])).unwrap();
        assert_eq!(fit.skew_ppb, 0);
        assert_eq!(fit.skew_uncertainty_ppb, SKEW_NOT_ESTIMATED);
        assert!(!fit.skew_estimated());
    }

    #[test]
    fn no_observations_fit_nothing() {
        assert_eq!(fit_one_way(&[]), None);
        let mut window = SlidingWindow::default();
        assert_eq!(window.fit(), None);
        assert_eq!(window.map_to_reference(123), 123);
        assert_eq!(window.inverse_map(123), 123);
    }

    #[test]
    fn a_jittery_link_is_outside_a_one_period_tolerance() {
        let observations = synthetic(
            400,
            5_000_000,
            1_000_000,
            0,
            0,
            &[0, 3_000_000, 12_000_000, 500_000],
        );
        let tolerance = tolerance_for_rate(200_000).unwrap();
        assert_eq!(tolerance, 5_000_000);
        assert!(!fit_one_way(&observations).unwrap().is_within(tolerance));
        let clean = synthetic(400, 5_000_000, 1_000_000, 0, 0, &[0, 20_000, 40_000]);
        assert!(fit_one_way(&clean).unwrap().is_within(5_000_000));
    }

    #[test]
    fn the_line_maps_its_origin_to_origin_plus_offset() {
        let observations = synthetic(500, 5_000_000, 2_000_000, 10_000, 0, &[0, 30_000]);
        let fit = fit_one_way(&observations).unwrap();
        assert_eq!(
            fit.line().map_to_reference(fit.reference_source_ns),
            (fit.reference_source_ns as i64 + fit.offset_ns) as u64
        );
    }

    #[test]
    fn the_window_refits_after_a_push_and_not_before() {
        let mut window = SlidingWindow::new(1_000, 16);
        window.push(o(0, 10));
        assert_eq!(window.fit().map(|f| f.offset_ns), Some(10));
        window.push(o(1, 5));
        assert_eq!(window.fit().map(|f| f.offset_ns), Some(4));
        window.clear();
        assert!(window.is_empty());
        assert_eq!(window.fit(), None);
    }

    #[test]
    fn a_zero_capacity_keeps_nothing() {
        let mut window = SlidingWindow::new(1_000, 0);
        let mut sample = Subsample::new(0);
        window.push(o(0, 10));
        sample.push(o(0, 10));
        assert_eq!(window.len(), 0);
        assert!(sample.observations().is_empty());
        assert_eq!(sample.fit(), None);
    }
}
