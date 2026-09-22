//! Runs every conformance vector in `vectors/` against this crate.
//!
//! A vector is language-free JSON with one expected answer per row; any
//! implementation of the clock core runs the same files. Encoding, as the files
//! use it: every `u64` and `i64` is a decimal string, a `u32` and an
//! enumeration integer are JSON numbers, an `f64` input is its IEEE-754 bit
//! pattern, `Option::None` is the string `"none"`, `Rounding` is `"floor_only"`
//! or `"two_sided"`, and a refusal is `"refused:<variant>"`. An object
//! expectation is compared field by field as `row.field`.
//!
//! A vector's rows are never edited to match an implementation: when a row
//! fails, the implementation is wrong or the rule the vector states changed.
//! Every row's result is collected, and the test fails once with all of them.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]

#[path = "support/json.rs"]
mod json;

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use extendedresearch_clock::{
    Anchor, Basis, Bound, Discontinuity, Domain, DomainId, DriftCheck, DriftThreshold, Line,
    MsError, Observation, Reading, Rounding, SinceError, SlidingWindow, Subsample, SuppliedClock,
    SuspendBehaviour, UNBOUNDED, WallError, bound_for_read, doc_epoch_from, fit_one_way,
    host_clock_epoch_from, ns_from_ms, quantum_from_deltas, tolerance_for_rate, wall_at,
};
use json::Json;

/// Every vector, by id. A vector that disappears, or one added without being
/// listed here, fails the run rather than silently changing what is checked.
const VECTORS: &[&str] = &[
    "0001-host-clock-epoch-from-one-reading-pair",
    "0002-interval-bounds-add-crosswise",
    "0003-a-suspend-seen-by-drift-ends-the-domain",
    "0004-quantum-from-deltas-fits-one-grid-or-returns-none",
    "0005-doc-epoch-from-two-nonce-halves",
    "0006-a-browser-host-id-is-its-epoch-and-keeps-no-suffix",
    "0007-readings-subtract-only-on-byte-equal-domains",
    "0008-a-bound-is-unknown-only-when-both-sides-are-unbounded",
    "0009-widen-never-narrows",
    "0010-a-clock-read-is-bounded-by-its-rounding",
    "0011-ms-to-ns-rounds-to-nearest-and-refuses-what-is-not-a-time",
    "0012-an-anchor-keeps-the-first-narrowest-bracket-and-states-its-error",
    "0013-a-reading-maps-to-the-calendar-through-the-anchor",
    "0014-the-drift-check-refuses-rather-than-guesses",
    "0015-basis-integers-are-fixed",
    "0016-suspend-behaviour-integers",
    "0017-monotonic-source-literals-are-bare-identifiers",
    "0018-a-fitted-line-maps-forward-and-back-in-integers",
    "0019-a-one-way-fit-is-a-min-filter-and-an-envelope-slope",
    "0020-a-tolerance-is-one-sample-period",
    "0021-a-session-subsample-halves-and-doubles-its-stride",
    "0022-a-sliding-window-keeps-a-span-of-source-time",
    "0023-monotonic-sources-include-or-exclude-suspend",
];

/// The two vectors that hold the monotonic-source literal set between them.
const LITERALS: &str = "0017-monotonic-source-literals-are-bare-identifiers";
const CLASSIFICATION: &str = "0023-monotonic-sources-include-or-exclude-suspend";

/// The fixture strings a vector does not carry.
const FIXTURE_SOURCE: &str = "CLOCK_MONOTONIC";
const FIXTURE_WALL_SOURCE: &str = "CLOCK_REALTIME";

type Observed = BTreeMap<String, String>;

fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vectors")
}

// ---- decoding ---------------------------------------------------------------

fn field<'a>(value: &'a Json, key: &str) -> &'a Json {
    value
        .get(key)
        .unwrap_or_else(|| panic!("missing key {key:?} in {value:?}"))
}

fn text<'a>(value: &'a Json, key: &str) -> &'a str {
    field(value, key)
        .as_str()
        .unwrap_or_else(|| panic!("{key:?} is not a string"))
}

fn u64_at(value: &Json, key: &str) -> u64 {
    let s = text(value, key);
    s.parse()
        .unwrap_or_else(|_| panic!("{key:?} = {s:?} is not a decimal u64"))
}

fn opt_u64_at(value: &Json, key: &str) -> Option<u64> {
    match text(value, key) {
        "none" => None,
        _ => Some(u64_at(value, key)),
    }
}

fn int_at(value: &Json, key: &str) -> u64 {
    field(value, key)
        .as_integer()
        .unwrap_or_else(|| panic!("{key:?} is not a JSON integer"))
}

fn rounding(s: &str) -> Rounding {
    match s {
        "floor_only" => Rounding::FloorOnly,
        "two_sided" => Rounding::TwoSided,
        other => panic!("unknown rounding {other:?}"),
    }
}

fn rounding_at_or_floor(value: &Json, key: &str) -> Rounding {
    value.get(key).map_or(Rounding::FloorOnly, |r| {
        rounding(r.as_str().expect("rounding"))
    })
}

fn basis(n: u64) -> Basis {
    match n {
        0 => Basis::Unspecified,
        1 => Basis::AfterReadReturned,
        2 => Basis::InPlatformCallback,
        3 => Basis::KernelSocketTimestamp,
        4 => Basis::InterruptEdge,
        5 => Basis::BrowserEventHandler,
        6 => Basis::FileRead,
        7 => Basis::DisplayFrame,
        8 => Basis::AudioOutputBuffer,
        9 => Basis::ClockRead,
        10 => Basis::PlatformEventTimestamp,
        other => panic!("no basis {other}"),
    }
}

fn bound_of(value: &Json) -> Bound {
    Bound {
        early_ns: u64_at(value, "early_ns"),
        late_ns: u64_at(value, "late_ns"),
        basis: basis(int_at(value, "basis")),
    }
}

fn domain(host_id: &str, host_clock_epoch: &str) -> Domain {
    Domain {
        host_id: host_id.to_owned(),
        host_clock_epoch: host_clock_epoch.to_owned(),
        monotonic_source: FIXTURE_SOURCE,
        suspend: SuspendBehaviour::Unspecified,
        resolution_ns: None,
        anchor: None,
    }
}

fn leak(s: &str) -> &'static str {
    Box::leak(s.to_owned().into_boxed_str())
}

fn anchor_of(value: &Json) -> Anchor {
    Anchor {
        monotonic_ns: u64_at(value, "monotonic_ns"),
        wall_unix_ns: u64_at(value, "wall_unix_ns"),
        wall_source: value
            .get("wall_source")
            .and_then(Json::as_str)
            .map_or(FIXTURE_WALL_SOURCE, leak),
        read_span_ns: u64_at(value, "read_span_ns"),
        wall_resolution_ns: opt_u64_at(value, "wall_resolution_ns"),
        wall_rounding: rounding_at_or_floor(value, "wall_rounding"),
    }
}

// ---- encoding ---------------------------------------------------------------

fn since_error(e: SinceError) -> String {
    match e {
        SinceError::DomainMismatch => "refused:domain_mismatch",
        SinceError::Overflow => "refused:overflow",
    }
    .to_owned()
}

fn put(out: &mut Observed, key: impl Into<String>, value: impl ToString) {
    out.insert(key.into(), value.to_string());
}

fn put_since(
    out: &mut Observed,
    row: &str,
    result: Result<extendedresearch_clock::Interval, SinceError>,
) {
    match result {
        Ok(i) => {
            put(out, format!("{row}.ns"), i.ns);
            put(out, format!("{row}.below_ns"), i.below_ns);
            put(out, format!("{row}.above_ns"), i.above_ns);
        }
        Err(e) => put(out, row, since_error(e)),
    }
}

fn put_bound(out: &mut Observed, row: &str, b: Bound) {
    put(out, format!("{row}.early_ns"), b.early_ns);
    put(out, format!("{row}.late_ns"), b.late_ns);
    put(out, format!("{row}.basis"), b.basis as u8);
}

fn rows(vector: &Json) -> &[(String, Json)] {
    field(vector, "rows")
        .as_object()
        .expect("rows is an object")
}

fn input(row: &Json) -> &Json {
    field(row, "input")
}

// ---- one runner per function ------------------------------------------------

fn host_clock_epoch_vector(vector: &Json) -> Observed {
    let mut out = Observed::new();
    for (name, row) in rows(vector) {
        let i = input(row);
        put(
            &mut out,
            name,
            host_clock_epoch_from(u64_at(i, "wall_unix_ns"), u64_at(i, "monotonic_ns")),
        );
    }
    out
}

fn doc_epoch_vector(vector: &Json) -> Observed {
    let mut out = Observed::new();
    for (name, row) in rows(vector) {
        let i = input(row);
        let half = |key| u32::try_from(int_at(i, key)).expect("a u32 half");
        put(
            &mut out,
            name,
            doc_epoch_from(half("nonce_hi"), half("nonce_lo")),
        );
    }
    out
}

fn since_vector(vector: &Json) -> Observed {
    let shared = vector.get("domain");
    let readings: BTreeMap<&str, Reading> = field(vector, "readings")
        .as_object()
        .expect("readings")
        .iter()
        .map(|(name, r)| {
            let mut d = match shared {
                Some(s) => domain(text(s, "host_id"), text(s, "host_clock_epoch")),
                None => domain(text(r, "host_id"), text(r, "host_clock_epoch")),
            };
            if let Some(source) = r.get("monotonic_source").and_then(Json::as_str) {
                d.monotonic_source = leak(source);
            }
            let clock = SuppliedClock::new(d);
            (name.as_str(), clock.at(u64_at(r, "ns"), bound_of(r)))
        })
        .collect();
    let mut out = Observed::new();
    for (name, row) in rows(vector) {
        let i = input(row);
        let later = readings[text(i, "later")];
        let earlier = readings[text(i, "earlier")];
        put_since(&mut out, name, later.since(earlier));
    }
    out
}

fn is_unknown_vector(vector: &Json) -> Observed {
    let mut out = Observed::new();
    for (name, row) in rows(vector) {
        put(&mut out, name, bound_of(input(row)).is_unknown());
    }
    out
}

fn widen_vector(vector: &Json) -> Observed {
    let clock = SuppliedClock::new(domain("fixture-host", "boot.1"));
    let mut out = Observed::new();
    for (name, row) in rows(vector) {
        let i = input(row);
        let r = clock.at(5, bound_of(i));
        let widened = r.widen(u64_at(i, "widen_early_ns"), u64_at(i, "widen_late_ns"));
        assert_eq!(widened.as_ns(), r.as_ns(), "widen kept the reading");
        assert_eq!(widened.domain(), r.domain(), "widen kept the domain");
        put_bound(&mut out, name, widened.bound());
    }
    out
}

fn bound_for_read_vector(vector: &Json) -> Observed {
    let mut out = Observed::new();
    for (name, row) in rows(vector) {
        let i = input(row);
        let b = bound_for_read(
            opt_u64_at(i, "resolution_ns"),
            rounding(text(i, "rounding")),
        );
        put_bound(&mut out, name, b);
    }
    out
}

fn ns_from_ms_vector(vector: &Json) -> Observed {
    let mut out = Observed::new();
    for (name, row) in rows(vector) {
        let hex = text(input(row), "ms");
        let bits = u64::from_str_radix(hex.strip_prefix("0x").expect("0x bit pattern"), 16)
            .expect("hex bit pattern");
        let value = match ns_from_ms(f64::from_bits(bits)) {
            Ok(ns) => ns.to_string(),
            Err(MsError::NotFinite) => "refused:not_finite".to_owned(),
            Err(MsError::Negative) => "refused:negative".to_owned(),
            Err(MsError::OutOfRange) => "refused:out_of_range".to_owned(),
        };
        put(&mut out, name, value);
    }
    out
}

fn quantum_vector(vector: &Json) -> Observed {
    let mut out = Observed::new();
    for (name, row) in rows(vector) {
        let deltas: Vec<u64> = field(input(row), "deltas_ns")
            .as_array()
            .expect("deltas_ns")
            .iter()
            .map(|d| d.as_str().expect("a decimal string").parse().expect("u64"))
            .collect();
        put(
            &mut out,
            name,
            quantum_from_deltas(&deltas).map_or("none".to_owned(), |q| q.to_string()),
        );
    }
    out
}

fn from_brackets_vector(vector: &Json) -> Observed {
    let mut out = Observed::new();
    for (name, row) in rows(vector) {
        let i = input(row);
        let brackets: Vec<(u64, u64, u64)> = field(i, "brackets")
            .as_array()
            .expect("brackets")
            .iter()
            .map(|b| {
                let n: Vec<u64> = b
                    .as_array()
                    .expect("a bracket")
                    .iter()
                    .map(|x| x.as_str().expect("decimal").parse().expect("u64"))
                    .collect();
                assert_eq!(n.len(), 3, "a bracket is [m1, wall, m2]");
                (n[0], n[1], n[2])
            })
            .collect();
        let built = Anchor::from_brackets(
            &brackets,
            opt_u64_at(i, "monotonic_resolution_ns"),
            rounding(text(i, "monotonic_rounding")),
            FIXTURE_WALL_SOURCE,
            opt_u64_at(i, "wall_resolution_ns"),
            rounding(text(i, "wall_rounding")),
        );
        match built {
            None => put(&mut out, name, "none"),
            Some((anchor, error)) => {
                assert_eq!(anchor.wall_source, FIXTURE_WALL_SOURCE);
                assert_eq!(
                    anchor.wall_resolution_ns,
                    opt_u64_at(i, "wall_resolution_ns")
                );
                assert_eq!(anchor.wall_rounding, rounding(text(i, "wall_rounding")));
                put(
                    &mut out,
                    format!("{name}.monotonic_ns"),
                    anchor.monotonic_ns,
                );
                put(
                    &mut out,
                    format!("{name}.wall_unix_ns"),
                    anchor.wall_unix_ns,
                );
                put(
                    &mut out,
                    format!("{name}.read_span_ns"),
                    anchor.read_span_ns,
                );
                put_bound(&mut out, name, error);
            }
        }
    }
    out
}

fn wall_domain(value: &Json) -> Domain {
    let mut d = domain(text(value, "host_id"), text(value, "host_clock_epoch"));
    d.resolution_ns = opt_u64_at(value, "resolution_ns");
    let anchor = field(value, "anchor");
    d.anchor = match anchor.as_str() {
        Some("none") => None,
        Some(other) => panic!("anchor {other:?}"),
        None => Some(anchor_of(anchor)),
    };
    d
}

fn wall_at_vector(vector: &Json) -> Observed {
    let shared = wall_domain(field(vector, "domain"));
    let mut out = Observed::new();
    for (name, row) in rows(vector) {
        let i = input(row);
        let d = i.get("domain").map_or_else(|| shared.clone(), wall_domain);
        let r = field(i, "reading");
        let on = match r.get("host_clock_epoch").and_then(Json::as_str) {
            Some(epoch) => domain(&d.host_id, epoch).id(),
            None => d.id(),
        };
        let reading = Reading::new(u64_at(r, "ns"), on, bound_of(r));
        match wall_at(&d, reading, rounding(text(i, "monotonic_rounding"))) {
            Ok(t) => {
                put(&mut out, format!("{name}.unix_ns"), t.unix_ns);
                put(&mut out, format!("{name}.early_ns"), t.early_ns);
                put(&mut out, format!("{name}.late_ns"), t.late_ns);
            }
            Err(e) => put(
                &mut out,
                name,
                match e {
                    WallError::DomainMismatch => "refused:domain_mismatch",
                    WallError::NoAnchor => "refused:no_anchor",
                    WallError::Overflow => "refused:overflow",
                },
            ),
        }
    }
    out
}

/// One drift run over supplied observations, and every field a row can ask
/// for, keyed as `outcomes`, `fired_at`, `host_id`, `host_clock_epoch`,
/// `clock_read_bound.<side>`, and `first.<field>` / `second.<field>`.
struct DriftRun {
    fields: Observed,
    /// The reading each observation was left with: re-tagged when it fired,
    /// `None` when it was refused.
    tagged: Vec<Option<Reading>>,
}

fn drift_run(
    start: &Domain,
    anchor: Anchor,
    threshold: DriftThreshold,
    monotonic: Rounding,
    observations: &[Json],
) -> DriftRun {
    let mut epochs: HashMap<DomainId, String> = HashMap::new();
    let mut remember = |d: &Domain| {
        epochs.insert(d.id(), d.host_clock_epoch.clone());
    };
    remember(start);
    let leg = bound_for_read(start.resolution_ns, monotonic);
    let mut check = DriftCheck::start(start, anchor, threshold);
    let mut current = start.clone();
    let mut outcomes = Vec::new();
    let mut fired_at = Vec::new();
    let mut events: Vec<Discontinuity> = Vec::new();
    let mut tagged = Vec::new();
    for (index, o) in observations.iter().enumerate() {
        let on = match o.get("reading_host_clock_epoch").and_then(Json::as_str) {
            Some(epoch) => domain(&current.host_id, epoch).id(),
            None => current.id(),
        };
        let reading = Reading::new(u64_at(o, "monotonic_ns"), on, leg);
        let result = check.observe(
            reading,
            u64_at(o, "wall_unix_ns"),
            u64_at(o, "read_span_ns"),
            opt_u64_at(o, "wall_resolution_ns"),
            rounding_at_or_floor(o, "wall_rounding"),
        );
        match result {
            Ok(None) => {
                outcomes.push("quiet".to_owned());
                tagged.push(Some(reading));
            }
            Ok(Some(d)) => {
                outcomes.push("fired".to_owned());
                fired_at.push(index.to_string());
                tagged.push(Some(d.after));
                current = d.next_domain.clone();
                remember(&current);
                events.push(d);
            }
            Err(e) => {
                outcomes.push(since_error(e));
                tagged.push(None);
            }
        }
    }
    let epoch_of = |id: DomainId| {
        epochs
            .get(&id)
            .cloned()
            .unwrap_or_else(|| "<unknown domain>".to_owned())
    };
    let mut fields = Observed::new();
    put(&mut fields, "outcomes", outcomes.join(","));
    put(&mut fields, "fired_at", fired_at.join(","));
    put(&mut fields, "host_id", &start.host_id);
    put(&mut fields, "host_clock_epoch", &start.host_clock_epoch);
    put(&mut fields, "clock_read_bound.early_ns", leg.early_ns);
    put(&mut fields, "clock_read_bound.late_ns", leg.late_ns);
    for (label, d) in ["first", "second"].iter().zip(&events) {
        let mut f = |k: &str, v: String| {
            fields.insert(format!("{label}.{k}"), v);
        };
        let next_anchor = d.next_domain.anchor.expect("next_domain carries an anchor");
        f("before_host_clock_epoch", epoch_of(d.before.domain()));
        f("before_ns", d.before.as_ns().to_string());
        f("before_early_ns", d.before.bound().early_ns.to_string());
        f("before_late_ns", d.before.bound().late_ns.to_string());
        f("before_basis", (d.before.bound().basis as u8).to_string());
        f("after_host_clock_epoch", epoch_of(d.after.domain()));
        f("next_host_id", d.next_domain.host_id.clone());
        f(
            "next_host_clock_epoch",
            d.next_domain.host_clock_epoch.clone(),
        );
        f("offset_change_ns", d.offset_change.ns.to_string());
        f(
            "offset_change_below_ns",
            d.offset_change.below_ns.to_string(),
        );
        f(
            "offset_change_above_ns",
            d.offset_change.above_ns.to_string(),
        );
        f(
            "next_anchor_monotonic_ns",
            next_anchor.monotonic_ns.to_string(),
        );
        f(
            "next_anchor_wall_unix_ns",
            next_anchor.wall_unix_ns.to_string(),
        );
        // The record's own invariants, whatever a row asks for.
        assert_eq!(d.after.domain(), d.next_domain.id());
        assert_eq!(d.after.since(d.before), Err(SinceError::DomainMismatch));
        assert_eq!(d.next_domain.host_id, start.host_id);
    }
    DriftRun { fields, tagged }
}

fn threshold_of(value: &Json) -> DriftThreshold {
    DriftThreshold {
        floor_ns: u64_at(value, "floor_ns"),
        rate_ppm: u32::try_from(int_at(value, "rate_ppm")).expect("rate_ppm is a u32"),
    }
}

fn drift_vector(vector: &Json) -> Observed {
    let shape = field(vector, "domain");
    let resolution_ns = opt_u64_at(shape, "resolution_ns");
    let monotonic = rounding(text(vector, "monotonic_rounding"));
    let host_id = match vector.get("nonce") {
        Some(n) => doc_epoch_from(
            u32::try_from(int_at(n, "nonce_hi")).expect("u32"),
            u32::try_from(int_at(n, "nonce_lo")).expect("u32"),
        ),
        None => text(shape, "host_id").to_owned(),
    };
    let make = |epoch: &str| Domain {
        resolution_ns,
        ..domain(&host_id, epoch)
    };
    let mut out = Observed::new();

    // One run over the whole vector, unless each row carries its own scenario.
    let whole = vector.get("observations").map(|observations| {
        let epoch = match vector.get("nonce") {
            Some(_) => host_id.clone(),
            None => text(shape, "host_clock_epoch").to_owned(),
        };
        drift_run(
            &make(&epoch),
            anchor_of(field(vector, "anchor")),
            threshold_of(field(vector, "threshold")),
            monotonic,
            observations.as_array().expect("observations"),
        )
    });

    for (name, row) in rows(vector) {
        let expect = field(row, "expect");
        let scenario = row.get("input").filter(|i| i.get("observations").is_some());
        if let Some(i) = scenario {
            let run = drift_run(
                &make(text(i, "host_clock_epoch")),
                anchor_of(field(i, "anchor")),
                threshold_of(field(i, "threshold")),
                monotonic,
                field(i, "observations").as_array().expect("observations"),
            );
            for (k, _) in expect
                .as_object()
                .expect("a scenario row expects an object")
            {
                let key = k.replacen("first_", "first.", 1);
                let value = run
                    .fields
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| format!("<no field {key}>"));
                put(&mut out, format!("{name}.{k}"), value);
            }
            continue;
        }
        let run = whole.as_ref().expect("a vector-wide run");
        if let Some(i) = row.get("input") {
            let at = |key| {
                let index = usize::try_from(int_at(i, key)).expect("index");
                run.tagged[index].expect("the observation was not refused")
            };
            put_since(&mut out, name, at("later").since(at("earlier")));
        } else if let Some(members) = expect.as_object() {
            for (k, _) in members {
                let key = format!("{name}.{k}");
                let value = run
                    .fields
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| format!("<no field {key}>"));
                put(&mut out, key, value);
            }
        } else {
            let value = run
                .fields
                .get(name)
                .cloned()
                .unwrap_or_else(|| format!("<no field {name}>"));
            put(&mut out, name, value);
        }
    }
    out
}

fn basis_vector(vector: &Json) -> Observed {
    let mut out = Observed::new();
    for (name, _) in rows(vector) {
        let b = match name.as_str() {
            "unspecified" => Basis::Unspecified,
            "after_read_returned" => Basis::AfterReadReturned,
            "in_platform_callback" => Basis::InPlatformCallback,
            "kernel_socket_timestamp" => Basis::KernelSocketTimestamp,
            "interrupt_edge" => Basis::InterruptEdge,
            "browser_event_handler" => Basis::BrowserEventHandler,
            "file_read" => Basis::FileRead,
            "display_frame" => Basis::DisplayFrame,
            "audio_output_buffer" => Basis::AudioOutputBuffer,
            "clock_read" => Basis::ClockRead,
            "platform_event_timestamp" => Basis::PlatformEventTimestamp,
            other => panic!("no basis named {other:?}"),
        };
        put(&mut out, name, b as u8);
    }
    out
}

fn suspend_vector(vector: &Json) -> Observed {
    let mut out = Observed::new();
    for (name, _) in rows(vector) {
        let s = match name.as_str() {
            "unspecified" => SuspendBehaviour::Unspecified,
            "included" => SuspendBehaviour::Included,
            "excluded" => SuspendBehaviour::Excluded,
            other => panic!("no suspend behaviour named {other:?}"),
        };
        put(&mut out, name, s as u8);
    }
    out
}

/// Vector 0017's test for a bare identifier. Both vectors over the literal set
/// apply it, and it is written once so they cannot apply different ones.
fn is_bare(literal: &str) -> bool {
    !literal.is_empty()
        && !literal
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '(' | ')' | ':' | ';' | ','))
}

fn monotonic_source_vector(vector: &Json) -> Observed {
    let mut out = Observed::new();
    for (name, row) in rows(vector) {
        let literal = text(input(row), "monotonic_source");
        put(
            &mut out,
            name,
            if is_bare(literal) {
                "bare_identifier"
            } else {
                "not_bare"
            },
        );
    }
    out
}

/// Every literal vector 0017 declares.
///
/// Reading the other file is the point rather than an accident: the
/// suspend-behaviour rule is split across repositories, and inside this one it
/// is split across two vectors. Deciding `declared` in 0023 from 0017's rows is
/// what stops the two sets drifting apart the way this repository's set and the
/// consumer's tables already had.
fn declared_literals() -> Vec<String> {
    let vector = load(&vectors_dir().join(format!("{LITERALS}.json")));
    rows(&vector)
        .iter()
        .filter(|(_, row)| field(row, "expect").as_str() == Some("bare_identifier"))
        .map(|(_, row)| text(input(row), "monotonic_source").to_owned())
        .collect()
}

/// The literals vector 0023 classifies, which is every row it marks declared.
fn classified_literals() -> Vec<String> {
    let vector = load(&vectors_dir().join(format!("{CLASSIFICATION}.json")));
    rows(&vector)
        .iter()
        .filter(|(_, row)| text(field(row, "expect"), "declared") == "yes")
        .map(|(_, row)| text(input(row), "monotonic_source").to_owned())
        .collect()
}

fn suspend_classification_vector(vector: &Json) -> Observed {
    let declared = declared_literals();
    let mut out = Observed::new();
    for (name, row) in rows(vector) {
        let literal = text(input(row), "monotonic_source");
        put(
            &mut out,
            format!("{name}.declared"),
            if declared.iter().any(|one| one == literal) {
                "yes"
            } else {
                "no"
            },
        );
        put(
            &mut out,
            format!("{name}.literal"),
            if is_bare(literal) {
                "bare_identifier"
            } else {
                "not_bare"
            },
        );
        // This repository owns the enumeration and the literal set; the
        // platform table that says which call advances across a suspend lives
        // in a consumer, and nothing here can run it. So the classification is
        // the vector's to state rather than an implementation's to produce, and
        // what is checkable here is that every behaviour it names is a variant
        // this crate declares — a vector naming a fourth one fails the run.
        let want = text(field(row, "expect"), "suspend_behaviour");
        let behaviour = match want {
            "0" => SuspendBehaviour::Unspecified,
            "1" => SuspendBehaviour::Included,
            "2" => SuspendBehaviour::Excluded,
            other => panic!("{name}: {other:?} is no SuspendBehaviour this crate declares"),
        };
        put(
            &mut out,
            format!("{name}.suspend_behaviour"),
            behaviour as u8,
        );
    }
    out
}

fn i64_at(value: &Json, key: &str) -> i64 {
    let s = text(value, key);
    s.parse()
        .unwrap_or_else(|_| panic!("{key:?} = {s:?} is not a decimal i64"))
}

fn u64_list(value: &Json, key: &str) -> Vec<u64> {
    field(value, key)
        .as_array()
        .unwrap_or_else(|| panic!("{key:?} is not an array"))
        .iter()
        .map(|x| x.as_str().expect("a decimal string").parse().expect("u64"))
        .collect()
}

fn observations_at(value: &Json, key: &str) -> Vec<Observation> {
    field(value, key)
        .as_array()
        .unwrap_or_else(|| panic!("{key:?} is not an array"))
        .iter()
        .map(|pair| {
            let n: Vec<u64> = pair
                .as_array()
                .expect("an observation")
                .iter()
                .map(|x| x.as_str().expect("decimal").parse().expect("u64"))
                .collect();
            assert_eq!(n.len(), 2, "an observation is [source_ns, receipt_ns]");
            Observation {
                source_ns: n[0],
                receipt_ns: n[1],
            }
        })
        .collect()
}

/// The generator a `synthetic` input stands for, as the vector states it.
fn synthetic(value: &Json) -> Vec<Observation> {
    let count = u64_at(value, "count");
    let period_ns = u64_at(value, "period_ns");
    let offset_ns = i64_at(value, "offset_ns");
    let skew_ppb = i64_at(value, "skew_ppb");
    let floor_ns = u64_at(value, "floor_ns");
    let jitter = u64_list(value, "jitter_ns");
    (0..count)
        .map(|i| {
            let source_ns = 1_000_000_000 + i * period_ns;
            let drift =
                (i128::from(source_ns) - 1_000_000_000) * i128::from(skew_ppb) / 1_000_000_000;
            let receipt = i128::from(source_ns)
                + i128::from(offset_ns)
                + drift
                + i128::from(floor_ns)
                + i128::from(jitter[(i % jitter.len() as u64) as usize]);
            Observation {
                source_ns,
                receipt_ns: u64::try_from(receipt).expect("a synthetic receipt fits u64"),
            }
        })
        .collect()
}

fn mapping_vector(vector: &Json) -> Observed {
    let mut out = Observed::new();
    for (name, row) in rows(vector) {
        let i = input(row);
        let ns = u64_at(i, "ns");
        let forward = match text(i, "direction") {
            "forward" => true,
            "inverse" => false,
            other => panic!("direction {other:?}"),
        };
        let value = match text(i, "quality") {
            // Nothing fitted: the live estimator before any observation, which
            // answers what an unavailable mapping answers.
            "unavailable" => {
                let mut window = SlidingWindow::default();
                if forward {
                    window.map_to_reference(ns)
                } else {
                    window.inverse_map(ns)
                }
            }
            "ok" | "degraded" => {
                let line = Line {
                    reference_source_ns: u64_at(i, "reference_source_ns"),
                    offset_ns: i64_at(i, "offset_ns"),
                    skew_ppb: i64_at(i, "skew_ppb"),
                };
                if forward {
                    line.map_to_reference(ns)
                } else {
                    line.inverse_map(ns)
                }
            }
            other => panic!("quality {other:?}"),
        };
        put(&mut out, name, value);
    }
    out
}

fn fit_vector(vector: &Json) -> Observed {
    let mut out = Observed::new();
    for (name, row) in rows(vector) {
        let i = input(row);
        let observations = match i.get("synthetic") {
            Some(s) => synthetic(s),
            None => observations_at(i, "observations"),
        };
        let Some(fit) = fit_one_way(&observations) else {
            put(&mut out, name, "none");
            continue;
        };
        let quality = if fit.is_within(u64_at(i, "tolerance_ns")) {
            "ok"
        } else {
            "degraded"
        };
        let mut f = |k: &str, v: String| put(&mut out, format!("{name}.{k}"), v);
        f("reference_source_ns", fit.reference_source_ns.to_string());
        f("offset_ns", fit.offset_ns.to_string());
        f("skew_ppb", fit.skew_ppb.to_string());
        f("skew_uncertainty_ppb", fit.skew_uncertainty_ppb.to_string());
        f("residual_spread_ns", fit.residual_spread_ns.to_string());
        f("observation_count", fit.observation_count.to_string());
        f("bias_low_ns", fit.bias_low_ns.to_string());
        f("bias_high_ns", fit.bias_high_ns.to_string());
        f("quality", quality.to_owned());
    }
    out
}

fn tolerance_vector(vector: &Json) -> Observed {
    let mut out = Observed::new();
    for (name, row) in rows(vector) {
        let rate = u64_at(input(row), "nominal_rate_millihz");
        put(
            &mut out,
            name,
            tolerance_for_rate(rate).map_or("none".to_owned(), |t| t.to_string()),
        );
    }
    out
}

fn sources(observations: impl Iterator<Item = Observation>) -> String {
    observations
        .map(|o| o.source_ns.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn subsample_vector(vector: &Json) -> Observed {
    let mut out = Observed::new();
    for (name, row) in rows(vector) {
        let i = input(row);
        let capacity = usize::try_from(u64_at(i, "capacity")).expect("capacity");
        let mut sample = Subsample::new(capacity);
        for n in 0..u64_at(i, "count") {
            sample.push(Observation {
                source_ns: n,
                receipt_ns: n,
            });
        }
        put(
            &mut out,
            format!("{name}.kept"),
            sources(sample.observations().iter().copied()),
        );
        put(&mut out, format!("{name}.stride"), sample.stride());
    }
    out
}

fn sliding_window_vector(vector: &Json) -> Observed {
    let mut out = Observed::new();
    for (name, row) in rows(vector) {
        let i = input(row);
        let capacity = usize::try_from(u64_at(i, "capacity")).expect("capacity");
        let mut window = SlidingWindow::new(u64_at(i, "window_ns"), capacity);
        for o in observations_at(i, "observations") {
            window.push(o);
        }
        put(
            &mut out,
            format!("{name}.kept"),
            sources(window.observations()),
        );
    }
    out
}

fn run(vector: &Json) -> Observed {
    match text(vector, "function") {
        "host_clock_epoch_from" => host_clock_epoch_vector(vector),
        "doc_epoch_from" => doc_epoch_vector(vector),
        "Reading::since" => since_vector(vector),
        "Bound::is_unknown" => is_unknown_vector(vector),
        "Reading::widen" => widen_vector(vector),
        "bound_for_read" => bound_for_read_vector(vector),
        "ns_from_ms" => ns_from_ms_vector(vector),
        "quantum_from_deltas" => quantum_vector(vector),
        "Anchor::from_brackets" => from_brackets_vector(vector),
        "wall_at" => wall_at_vector(vector),
        "DriftCheck" => drift_vector(vector),
        "Basis" => basis_vector(vector),
        "SuspendBehaviour" => suspend_vector(vector),
        "monotonic_source" => monotonic_source_vector(vector),
        "suspend_classification" => suspend_classification_vector(vector),
        "mapping" => mapping_vector(vector),
        "fit_one_way" => fit_vector(vector),
        "tolerance_for_rate" => tolerance_vector(vector),
        "Subsample" => subsample_vector(vector),
        "SlidingWindow" => sliding_window_vector(vector),
        other => panic!("no runner for function {other:?}"),
    }
}

/// The expectations, flattened to `row` or `row.field`, each a string.
fn expected(vector: &Json, id: &str) -> Observed {
    let mut out = Observed::new();
    for (name, row) in rows(vector) {
        match field(row, "expect") {
            Json::String(s) => put(&mut out, name, s),
            Json::Object(members) => {
                assert!(
                    !members.is_empty(),
                    "{id}: row {name} expects an empty object"
                );
                for (k, v) in members {
                    let s = v.as_str().unwrap_or_else(|| {
                        panic!("{id}: {name}.{k}: an expectation is a string, never {v:?}")
                    });
                    put(&mut out, format!("{name}.{k}"), s);
                }
            }
            other => panic!(
                "{id}: row {name}: an expectation is a string or a flat object, never {other:?}"
            ),
        }
    }
    out
}

fn load(path: &Path) -> Json {
    let text =
        fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    json::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn every_vector_passes() {
    let mut paths: Vec<PathBuf> = fs::read_dir(vectors_dir())
        .expect("the vectors directory")
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();
    let ids: Vec<String> = paths
        .iter()
        .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        ids, VECTORS,
        "the vectors on disk are the ones this test lists"
    );

    let mut failures = Vec::new();
    let mut rows_checked = 0;
    for (path, id) in paths.iter().zip(&ids) {
        let vector = load(path);
        assert_eq!(text(&vector, "clock_vector"), "0", "{id}: format version");
        assert_eq!(text(&vector, "id"), id, "{id}: id equals the file name");
        assert!(!text(&vector, "about").is_empty(), "{id}: about");
        let faces = field(&vector, "faces").as_array().expect("faces");
        assert!(
            !faces.is_empty()
                && faces
                    .iter()
                    .all(|f| matches!(f.as_str(), Some("native" | "browser"))),
            "{id}: faces is native, browser or both"
        );
        let want = expected(&vector, id);
        let got = run(&vector);
        for (key, value) in &want {
            rows_checked += 1;
            match got.get(key) {
                Some(g) if g == value => {}
                Some(g) => failures.push(format!("{id} / {key}: expected {value}, got {g}")),
                None => failures.push(format!("{id} / {key}: expected {value}, got nothing")),
            }
        }
        for key in got.keys().filter(|k| !want.contains_key(*k)) {
            failures.push(format!("{id} / {key}: produced a value no row expects"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {rows_checked} vector rows failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // Guards against a runner that silently reads nothing.
    assert!(rows_checked >= 150, "only {rows_checked} rows checked");
}

/// The literal set is written down twice here — 0017 asks which strings a face
/// may emit, 0023 asks what each one does across a suspend — and one of those
/// questions gaining a literal the other lacks is the defect this pair exists to
/// stop. It is exactly how this repository's set and the consumer's platform
/// tables came apart: four literals shared, one each way, and nothing comparing
/// them.
#[test]
fn both_vectors_name_the_same_literals() {
    let mut declared = declared_literals();
    let mut classified = classified_literals();
    declared.sort();
    classified.sort();
    assert_eq!(
        declared, classified,
        "{LITERALS} and {CLASSIFICATION} declare different literals"
    );
    assert_eq!(declared.len(), 6, "the literal set is six strings");
}

#[test]
fn unbounded_in_the_vectors_is_the_crates_sentinel() {
    assert_eq!(UNBOUNDED.to_string(), "18446744073709551615");
}
