//! Each conversion, run against an embedded interpreter.
//!
//! `auto-initialize` in the dev-dependencies starts CPython on first use, so
//! these assertions are Python's own answers: `isinstance`, `issubclass`,
//! `IntEnum` lookups.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]

use std::fmt;

use extendedresearch_abi::enumeration::Enumeration;
use extendedresearch_pyo3::{ExceptionFamily, Raise, code_error, error, exceptions, int_enum};
use extendedresearch_status::codes::{self, AbiError, DOMAIN_FLOOR};
use pyo3::exceptions::{PyIndexError, PyOSError, PyTimeoutError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyModule;

const THING_ERR_REFUSED: i32 = DOMAIN_FLOOR;
const THING_ERR_SLOW: i32 = DOMAIN_FLOOR - 1;
const THING_ERR_NETWORK: i32 = DOMAIN_FLOOR - 2;
const THING_ERR_UNMAPPED: i32 = DOMAIN_FLOOR - 3;

exceptions! {
    /// The test package's exceptions.
    pub family ThingExceptions in _thing;
    prefix "THING";
    base ThingError: "Anything thing refused.";
    panic PanicError: "A panic was caught.";
    exception RefusedError(ThingError): "Thing refused.";
    exception NetworkError(PyOSError): "The network failed.";
    domain THING_ERR_REFUSED => RefusedError;
    domain THING_ERR_SLOW => PyTimeoutError;
    domain THING_ERR_NETWORK => NetworkError;
}

/// A core error type the way a package writes one.
struct Failure {
    code: i32,
    name: &'static str,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("it went wrong")
    }
}

impl AbiError for Failure {
    fn code(&self) -> i32 {
        self.code
    }
    fn name(&self) -> &'static str {
        self.name
    }
}

fn raised(code: i32, name: &'static str) -> PyErr {
    error::<ThingExceptions, _>(&Failure { code, name })
}

#[test]
fn utf8_is_value_error_and_range_is_index_error() {
    Python::attach(|py| {
        assert!(raised(codes::ERR_UTF8, "ERR_UTF8").is_instance_of::<PyValueError>(py));
        assert!(raised(codes::ERR_RANGE, "ERR_RANGE").is_instance_of::<PyIndexError>(py));
    });
}

#[test]
fn null_state_and_an_unnamed_boundary_code_are_the_base_error() {
    Python::attach(|py| {
        for (code, name) in [
            (codes::ERR_NULL, "ERR_NULL"),
            (codes::ERR_STATE, "ERR_STATE"),
            (-9, "ERR_NINE"),
        ] {
            let err = raised(code, name);
            assert!(err.is_instance_of::<ThingError>(py), "{name}");
            assert!(!err.is_instance_of::<PanicError>(py), "{name}");
        }
    });
}

#[test]
fn a_panic_is_panic_error_beneath_the_base() {
    Python::attach(|py| {
        let err = raised(codes::ERR_PANIC, "ERR_PANIC");
        assert!(err.is_instance_of::<PanicError>(py));
        assert!(err.is_instance_of::<ThingError>(py));
    });
}

#[test]
fn domain_codes_raise_what_the_family_maps() {
    Python::attach(|py| {
        let refused = raised(THING_ERR_REFUSED, "THING_ERR_REFUSED");
        assert!(refused.is_instance_of::<RefusedError>(py));
        assert!(refused.is_instance_of::<ThingError>(py));

        let slow = raised(THING_ERR_SLOW, "THING_ERR_SLOW");
        assert!(slow.is_instance_of::<PyTimeoutError>(py));

        let network = raised(THING_ERR_NETWORK, "THING_ERR_NETWORK");
        assert!(network.is_instance_of::<NetworkError>(py));
        assert!(network.is_instance_of::<PyOSError>(py));
        assert!(!network.is_instance_of::<ThingError>(py));
    });
}

#[test]
fn an_unmapped_domain_code_is_the_base_error() {
    Python::attach(|py| {
        let err = raised(THING_ERR_UNMAPPED, "THING_ERR_UNMAPPED");
        assert!(err.is_instance_of::<ThingError>(py));
        assert!(!err.is_instance_of::<RefusedError>(py));
    });
}

#[test]
fn the_message_leads_with_the_constant_s_name() {
    Python::attach(|py| {
        let err = raised(THING_ERR_REFUSED, "THING_ERR_REFUSED");
        assert_eq!(
            err.value(py).to_string(),
            "THING_ERR_REFUSED: it went wrong"
        );
        let direct = code_error::<ThingExceptions>(codes::ERR_NULL, "ERR_NULL", "no handle");
        assert_eq!(direct.value(py).to_string(), "THING_ERR_NULL: no handle");
    });
}

#[test]
fn a_boundary_name_gains_the_prefix_once() {
    Python::attach(|py| {
        let range = raised(codes::ERR_RANGE, "ERR_RANGE");
        assert_eq!(
            range.value(py).to_string(),
            "THING_ERR_RANGE: it went wrong",
            "the spelling napi's Tokens and .NET's AbiErrors report"
        );
        let spelled = code_error::<ThingExceptions>(codes::ERR_STATE, "THING_ERR_STATE", "closed");
        assert_eq!(
            spelled.value(py).to_string(),
            "THING_ERR_STATE: closed",
            "a name that already carries the prefix is not prefixed again"
        );
    });
}

#[test]
fn raise_converts_only_the_error_side() {
    Python::attach(|py| {
        let fine: Result<u8, Failure> = Ok(3);
        assert_eq!(fine.raise::<ThingExceptions>().unwrap(), 3);
        let failed: Result<u8, Failure> = Err(Failure {
            code: THING_ERR_REFUSED,
            name: "THING_ERR_REFUSED",
        });
        let err = failed.raise::<ThingExceptions>().unwrap_err();
        assert!(err.is_instance_of::<RefusedError>(py));
    });
}

#[test]
fn register_adds_every_created_class_under_its_own_name() {
    Python::attach(|py| {
        let module = PyModule::new(py, "_thing").unwrap();
        ThingExceptions::register(&module).unwrap();
        for name in ["ThingError", "PanicError", "RefusedError", "NetworkError"] {
            let class = module.getattr(name).unwrap();
            assert_eq!(
                class
                    .getattr("__name__")
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                name
            );
            assert_eq!(
                class
                    .getattr("__module__")
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                "_thing"
            );
        }
        // The builtins a family maps to are not the family's to register.
        assert!(module.getattr("PyTimeoutError").is_err());
    });
}

static ORIGINS: Enumeration = Enumeration::new(&[
    (0, "ORIGIN_UNSPECIFIED"),
    (1, "ORIGIN_RAW"),
    (2, "ORIGIN_DERIVED"),
]);

static RATES: Enumeration = Enumeration::new(&[(0, "RATE_UNSPECIFIED"), (50, "RATE_50HZ")]);

static LONE: Enumeration = Enumeration::new(&[(7, "ONLY")]);

#[test]
fn an_int_enum_binds_short_names_with_the_full_names_as_aliases() {
    Python::attach(|py| {
        let origin = int_enum(py, "Origin", "thing", "What a stream is for.", &ORIGINS).unwrap();
        let raw = origin.getattr("RAW").unwrap();
        assert_eq!(raw.extract::<i32>().unwrap(), 1);
        assert!(origin.getattr("ORIGIN_RAW").unwrap().is(&raw));
        assert_eq!(
            raw.getattr("name").unwrap().extract::<String>().unwrap(),
            "RAW",
            "the short name is the canonical member"
        );
        assert_eq!(origin.len().unwrap(), 3, "aliases are not extra members");
        assert_eq!(
            origin
                .getattr("__module__")
                .unwrap()
                .extract::<String>()
                .unwrap(),
            "thing"
        );
        assert_eq!(
            origin
                .getattr("__doc__")
                .unwrap()
                .extract::<String>()
                .unwrap(),
            "What a stream is for."
        );
        let int_enum_class = py.import("enum").unwrap().getattr("IntEnum").unwrap();
        assert!(
            py.import("builtins")
                .unwrap()
                .getattr("issubclass")
                .unwrap()
                .call1((&origin, &int_enum_class))
                .unwrap()
                .extract::<bool>()
                .unwrap()
        );
    });
}

#[test]
fn a_name_that_would_start_with_a_digit_keeps_its_full_spelling() {
    Python::attach(|py| {
        let rate = int_enum(py, "Rate", "thing", "", &RATES).unwrap();
        assert_eq!(
            rate.getattr("UNSPECIFIED")
                .unwrap()
                .extract::<i32>()
                .unwrap(),
            0
        );
        assert_eq!(
            rate.getattr("RATE_50HZ").unwrap().extract::<i32>().unwrap(),
            50
        );
        assert!(rate.getattr("50HZ").is_err());
    });
}

#[test]
fn a_single_member_keeps_its_full_name() {
    Python::attach(|py| {
        let lone = int_enum(py, "Lone", "thing", "", &LONE).unwrap();
        assert_eq!(lone.getattr("ONLY").unwrap().extract::<i32>().unwrap(), 7);
        assert_eq!(lone.len().unwrap(), 1);
    });
}
