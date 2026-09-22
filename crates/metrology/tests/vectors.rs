//! Runs every conformance vector in `vectors/` against this crate.
//!
//! A vector is language-free JSON with one expected answer per row; any
//! implementation of the uncertainty model runs the same files (R62). Encoding,
//! as the files use it: every `u64` and `i64` is a decimal string, a `u32` and
//! an enumeration integer are JSON numbers, `Option::None` is the string
//! `"none"`, and a refusal is `"refused:<variant>"` with a second row field
//! naming what it refused over.
//!
//! A provenance is `"unknown"`, `"measured:<id>"`, `"specified:<id>"`,
//! `"estimated:<estimator>/<inputs>"` or `"bounded:<id>"`. A correlation is
//! `"undeclared"`, `"independent"` or `"correlated:<group>"`. A span is
//! `"<chain>:<from>-<to>"`.
//!
//! A composition row answers **every** field of the budget, not only the one
//! the vector is about: an implementation that got the total right and the
//! uncovered links wrong fails the row it passed by accident. The runner emits
//! a fixed key set, and the harness fails a row that produces a key nothing
//! expects as well as one that expects a key nothing produced.
//!
//! A vector's rows are never edited to match an implementation: when a row
//! fails, the implementation is wrong or the rule the vector states changed.
//! Every row's result is collected, and the test fails once with all of them.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]

#[path = "support/json.rs"]
mod json;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use extendedresearch_metrology::{
    ArgumentId, Bias, Budget, CalibrationId, Chain, ChainId, CombinedDispersion, Composer,
    CompositionError, Correction, Correlation, Dispersion, DistributionKind, DocumentId,
    EstimatorId, GroupId, InputsId, Link, LinkKind, Provenance, Span, Term, UNBOUNDED,
};
use json::Json;

/// Every vector, by id. A vector that disappears, or one added without being
/// listed here, fails the run rather than silently changing what is checked.
const VECTORS: &[&str] = &[
    "0001-bias-widths-add-crosswise",
    "0002-bias-addition-saturates-at-unbounded",
    "0003-an-unknown-term-poisons-the-total",
    "0004-a-partial-sum-is-never-the-total",
    "0005-two-corrections-over-one-span-are-refused",
    "0006-a-link-no-term-covers-makes-the-total-unbounded",
    "0007-dispersions-combine-only-when-the-correlation-is-declared",
    "0008-corrections-apply-before-bias-composition",
    "0009-no-correction-and-a-zero-correction-are-different-claims",
    "0010-a-term-may-cover-several-links",
    "0011-link-kind-integers-are-fixed",
    "0012-provenance-correlation-and-distribution-integers",
];

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

fn i64_at(value: &Json, key: &str) -> i64 {
    let s = text(value, key);
    s.parse()
        .unwrap_or_else(|_| panic!("{key:?} = {s:?} is not a decimal i64"))
}

fn int_at(value: &Json, key: &str) -> u64 {
    field(value, key)
        .as_integer()
        .unwrap_or_else(|| panic!("{key:?} is not a JSON integer"))
}

fn u32_at(value: &Json, key: &str) -> u32 {
    u32::try_from(int_at(value, key)).expect("a u32")
}

fn u16_at(value: &Json, key: &str) -> u16 {
    u16::try_from(int_at(value, key)).expect("a u16")
}

fn link_kind(n: u64) -> LinkKind {
    match n {
        1 => LinkKind::Transduction,
        2 => LinkKind::AnalogConditioning,
        3 => LinkKind::Quantisation,
        4 => LinkKind::DeviceFilter,
        5 => LinkKind::DeviceBuffer,
        6 => LinkKind::DeviceStamp,
        7 => LinkKind::Transport,
        8 => LinkKind::HostReceive,
        9 => LinkKind::HostQueue,
        10 => LinkKind::HostStamp,
        11 => LinkKind::ApplicationSubmit,
        12 => LinkKind::FrameworkQueue,
        13 => LinkKind::Compositor,
        14 => LinkKind::OutputBuffer,
        15 => LinkKind::Conversion,
        16 => LinkKind::Emission,
        17 => LinkKind::ClockMapping,
        18 => LinkKind::Serialisation,
        other => panic!("no link kind {other}"),
    }
}

fn distribution_kind(n: u64) -> DistributionKind {
    match n {
        0 => DistributionKind::Unspecified,
        1 => DistributionKind::Gaussian,
        2 => DistributionKind::Uniform,
        other => panic!("no distribution kind {other}"),
    }
}

/// `"unknown"`, `"measured:4"`, `"specified:7"`, `"estimated:3/9"` or
/// `"bounded:1"`.
fn provenance(spelling: &str) -> Provenance {
    let id = |rest: &str| rest.parse::<u32>().expect("a u32 identifier");
    match spelling.split_once(':') {
        None if spelling == "unknown" => Provenance::Unknown,
        Some(("measured", rest)) => Provenance::Measured {
            calibration: CalibrationId(id(rest)),
        },
        Some(("specified", rest)) => Provenance::Specified {
            document: DocumentId(id(rest)),
        },
        Some(("estimated", rest)) => {
            let (estimator, inputs) = rest.split_once('/').expect("estimator/inputs");
            Provenance::Estimated {
                estimator: EstimatorId(id(estimator)),
                inputs: InputsId(id(inputs)),
            }
        }
        Some(("bounded", rest)) => Provenance::Bounded {
            argument: ArgumentId(id(rest)),
        },
        _ => panic!("no provenance {spelling:?}"),
    }
}

/// `"undeclared"`, `"independent"` or `"correlated:1"`.
fn correlation(spelling: &str) -> Correlation {
    match spelling.split_once(':') {
        None if spelling == "undeclared" => Correlation::Undeclared,
        None if spelling == "independent" => Correlation::Independent,
        Some(("correlated", rest)) => {
            Correlation::CorrelatedWith(GroupId(rest.parse().expect("a u32 group")))
        }
        _ => panic!("no correlation {spelling:?}"),
    }
}

fn optional<'a>(value: &'a Json, key: &str) -> Option<&'a Json> {
    match value.get(key) {
        None => None,
        Some(Json::String(s)) if s == "none" => None,
        Some(other) => Some(other),
    }
}

fn chain_of(value: &Json) -> Chain {
    Chain::new(
        ChainId(u32_at(value, "id")),
        field(value, "links")
            .as_array()
            .expect("links")
            .iter()
            .map(|kind| Link::of(link_kind(kind.as_integer().expect("a link kind integer"))))
            .collect::<Vec<Link>>(),
    )
}

fn term_of(value: &Json) -> Term {
    let span = Span {
        chain: ChainId(u32_at(value, "chain")),
        from: u16_at(value, "from"),
        to: u16_at(value, "to"),
    };
    let mut term = Term::new(
        span,
        Bias::new(u64_at(value, "early_ns"), u64_at(value, "late_ns")),
        provenance(text(value, "bias_provenance")),
    );
    if let Some(correction) = optional(value, "correction") {
        term = term.with_correction(Correction::new(
            i64_at(correction, "ns"),
            provenance(text(correction, "provenance")),
        ));
    }
    if let Some(dispersion) = optional(value, "dispersion") {
        term = term.with_dispersion(Dispersion::new(
            u64_at(dispersion, "sd_ns"),
            distribution_kind(int_at(dispersion, "kind")),
        ));
    }
    if let Some(declaration) = value.get("correlation") {
        term = term.with_correlation(correlation(declaration.as_str().expect("a correlation")));
    }
    term
}

// ---- encoding ---------------------------------------------------------------

fn put(out: &mut Observed, key: impl Into<String>, value: impl ToString) {
    out.insert(key.into(), value.to_string());
}

fn span_text(span: Span) -> String {
    format!("{}:{}-{}", span.chain.0, span.from, span.to)
}

fn list(items: impl IntoIterator<Item = String>) -> String {
    let joined: Vec<String> = items.into_iter().collect();
    if joined.is_empty() {
        "none".to_owned()
    } else {
        joined.join(",")
    }
}

fn width(value: Option<u64>) -> String {
    value.map_or_else(|| "unbounded".to_owned(), |ns| ns.to_string())
}

fn dispersion_text(dispersion: &CombinedDispersion) -> String {
    match dispersion {
        CombinedDispersion::None => "none".to_owned(),
        CombinedDispersion::Known(d) => format!("{}/{}", d.sd_ns, d.kind.kind()),
        CombinedDispersion::Undeclared { terms } => {
            format!("undeclared:{}", list(terms.iter().map(ToString::to_string)))
        }
    }
}

fn put_budget(out: &mut Observed, row: &str, budget: &Budget) {
    let key = |field: &str| format!("{row}.{field}");
    put(
        out,
        key("total.kind"),
        if budget.total.is_bounded() {
            "bounded"
        } else {
            "unbounded"
        },
    );
    put(out, key("total.early_ns"), width(budget.total.early_ns()));
    put(out, key("total.late_ns"), width(budget.total.late_ns()));
    let partial = budget.total.partial_sums();
    put(out, key("known.early_ns"), partial.early_ns);
    put(out, key("known.late_ns"), partial.late_ns);
    put(
        out,
        key("unbounded_terms"),
        list(
            budget
                .total
                .unbounded_terms()
                .iter()
                .map(ToString::to_string),
        ),
    );
    put(
        out,
        key("uncovered_links"),
        list(
            budget
                .total
                .uncovered_links()
                .iter()
                .copied()
                .map(span_text),
        ),
    );
    put(
        out,
        key("largest_bounded"),
        budget
            .largest_bounded
            .map_or_else(|| "none".to_owned(), |index| index.to_string()),
    );
    put(out, key("dispersion"), dispersion_text(&budget.dispersion));
    put(
        out,
        key("interval_ns"),
        budget
            .interval_ns
            .map_or_else(|| "none".to_owned(), |ns| ns.to_string()),
    );
}

/// The refusal's variant, and the things it names (R28).
fn put_refusal(out: &mut Observed, row: &str, error: CompositionError) {
    let (variant, names) = match error {
        CompositionError::SameChainForBothEndpoints { chain } => (
            "same_chain_for_both_endpoints",
            format!("chain={}", chain.0),
        ),
        CompositionError::EmptyChain { chain } => ("empty_chain", format!("chain={}", chain.0)),
        CompositionError::ChainTooLong { chain, links } => {
            ("chain_too_long", format!("chain={} links={links}", chain.0))
        }
        CompositionError::MalformedSpan { term, span } => (
            "malformed_span",
            format!("term={term} span={}", span_text(span)),
        ),
        CompositionError::UnknownChain { term, chain } => {
            ("unknown_chain", format!("term={term} chain={}", chain.0))
        }
        CompositionError::SpanPastChain {
            term,
            span,
            last_position,
        } => (
            "span_past_chain",
            format!("term={term} span={} last={last_position}", span_text(span)),
        ),
        CompositionError::SpanOverlap {
            first,
            second,
            at,
            calibrations,
        } => (
            "span_overlap",
            format!(
                "terms={first},{second} at={} calibrations={}",
                span_text(at),
                list(calibrations.iter().map(|id| {
                    id.map_or_else(|| "none".to_owned(), |c: CalibrationId| c.0.to_string())
                }))
            ),
        ),
        CompositionError::Overflow => ("overflow", String::new()),
        // The error type is `#[non_exhaustive]`; a variant added without a row
        // here fails rather than being reported as something else.
        other => panic!("no encoding for {other:?}"),
    };
    put(out, format!("{row}.refusal"), format!("refused:{variant}"));
    put(out, format!("{row}.refusal.names"), names);
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

fn compose_vector(vector: &Json) -> Observed {
    let chains: BTreeMap<&str, Chain> = field(vector, "chains")
        .as_object()
        .expect("chains")
        .iter()
        .map(|(name, value)| (name.as_str(), chain_of(value)))
        .collect();
    let terms: BTreeMap<&str, Term> = match vector.get("terms") {
        None => BTreeMap::new(),
        Some(table) => table
            .as_object()
            .expect("terms")
            .iter()
            .map(|(name, value)| (name.as_str(), term_of(value)))
            .collect(),
    };

    let mut out = Observed::new();
    for (name, row) in rows(vector) {
        let i = input(row);
        let mut composer = Composer::new(
            chains[text(i, "later")].clone(),
            chains[text(i, "earlier")].clone(),
        );
        for wanted in field(i, "terms").as_array().expect("terms") {
            composer.term(terms[wanted.as_str().expect("a term name")]);
        }
        if let Some(later_ns) = i.get("later_ns") {
            let later_ns = later_ns.as_str().expect("later_ns").parse().expect("u64");
            composer.stamps(later_ns, u64_at(i, "earlier_ns"));
        }
        match composer.compose() {
            Ok(budget) => put_budget(&mut out, name, &budget),
            Err(error) => put_refusal(&mut out, name, error),
        }
    }
    out
}

fn link_kind_vector(vector: &Json) -> Observed {
    let every = [
        ("transduction", LinkKind::Transduction),
        ("analog_conditioning", LinkKind::AnalogConditioning),
        ("quantisation", LinkKind::Quantisation),
        ("device_filter", LinkKind::DeviceFilter),
        ("device_buffer", LinkKind::DeviceBuffer),
        ("device_stamp", LinkKind::DeviceStamp),
        ("transport", LinkKind::Transport),
        ("host_receive", LinkKind::HostReceive),
        ("host_queue", LinkKind::HostQueue),
        ("host_stamp", LinkKind::HostStamp),
        ("application_submit", LinkKind::ApplicationSubmit),
        ("framework_queue", LinkKind::FrameworkQueue),
        ("compositor", LinkKind::Compositor),
        ("output_buffer", LinkKind::OutputBuffer),
        ("conversion", LinkKind::Conversion),
        ("emission", LinkKind::Emission),
        ("clock_mapping", LinkKind::ClockMapping),
        ("serialisation", LinkKind::Serialisation),
    ];
    let mut out = Observed::new();
    for (name, _) in rows(vector) {
        let kind = every
            .iter()
            .find(|(spelling, _)| spelling == name)
            .unwrap_or_else(|| panic!("no link kind named {name:?}"))
            .1;
        put(&mut out, name, kind.kind());
    }
    out
}

fn enumeration_vector(vector: &Json) -> Observed {
    let mut out = Observed::new();
    for (name, _) in rows(vector) {
        let kind = match name.as_str() {
            "provenance.unknown" => Provenance::Unknown.kind(),
            "provenance.measured" => Provenance::Measured {
                calibration: CalibrationId(1),
            }
            .kind(),
            "provenance.specified" => Provenance::Specified {
                document: DocumentId(1),
            }
            .kind(),
            "provenance.estimated" => Provenance::Estimated {
                estimator: EstimatorId(1),
                inputs: InputsId(2),
            }
            .kind(),
            "provenance.bounded" => Provenance::Bounded {
                argument: ArgumentId(1),
            }
            .kind(),
            "correlation.undeclared" => Correlation::Undeclared.kind(),
            "correlation.independent" => Correlation::Independent.kind(),
            "correlation.correlated_with" => Correlation::CorrelatedWith(GroupId(1)).kind(),
            "distribution.unspecified" => DistributionKind::Unspecified.kind(),
            "distribution.gaussian" => DistributionKind::Gaussian.kind(),
            "distribution.uniform" => DistributionKind::Uniform.kind(),
            other => panic!("no enumeration named {other:?}"),
        };
        put(&mut out, name, kind);
    }
    out
}

fn run(vector: &Json) -> Observed {
    match text(vector, "function") {
        "Composer::compose" => compose_vector(vector),
        "LinkKind" => link_kind_vector(vector),
        "enumerations" => enumeration_vector(vector),
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
        assert_eq!(
            text(&vector, "metrology_vector"),
            "0",
            "{id}: format version"
        );
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
    assert!(rows_checked >= 250, "only {rows_checked} rows checked");
}

#[test]
fn unbounded_in_the_vectors_is_the_clock_crates_sentinel() {
    assert_eq!(UNBOUNDED.to_string(), "18446744073709551615");
    assert_eq!(UNBOUNDED, extendedresearch_clock::UNBOUNDED);
}
