//! The part of a package's PyO3 binding that every package writes the same way.
//!
//! A package's Python binding calls the package's safe Rust core directly. It
//! holds no raw handle and writes no `unsafe`; what it needs from here is the
//! translation of the core's answers into Python:
//!
//! | | |
//! |---|---|
//! | [`exceptions!`] | Creates the package's exception hierarchy **in the consuming crate**: a base error, a `PanicError` beneath it, any domain exceptions, and the table from domain code to exception |
//! | [`error`], [`code_error`], [`Raise`] | Turn an [`AbiError`] into the `PyErr` the package's family names for it |
//! | [`int_enum`] | Builds a Python `IntEnum` from a foundation [`Enumeration`], with the short-name rule below |
//!
//! # Why the exceptions are created in the consumer
//!
//! An exception class made by `create_exception!` lives in a `static` inside
//! the crate that expands it. A class created in this crate would be one static
//! per extension module that links this crate, so two packages loaded into one
//! interpreter would each raise their own copy of a class with the same name,
//! and `except` on one would miss the other. [`exceptions!`] expands in the
//! package's binding, so each package owns exactly one hierarchy, under its own
//! module name.
//!
//! # Which exception a code raises
//!
//! | Code | Raises | Why |
//! |---|---|---|
//! | `ERR_UTF8` | `ValueError` | The argument's value was wrong, and that is what a Python caller catches for it |
//! | `ERR_RANGE` | `IndexError` | An index or length past what the call can reach |
//! | `ERR_PANIC` | the package's `PanicError` | The package's state is unknown; it subclasses the base error so a broad `except` still sees it |
//! | `ERR_NULL`, `ERR_STATE`, any other boundary code | the package's base error | Nothing more specific in Python fits, and a caller catching "anything this package refused" catches it |
//! | a domain code the family maps | the mapped exception | The package's own choice, which may be a builtin such as `TimeoutError` |
//! | a domain code the family does not map | the package's base error | Still a failure, still catchable, and the message carries its name |
//!
//! Every message starts with the constant's name as the package's header spells
//! it — `CA3_ERR_UTF8: …`, `CA3_ERR_TRUNCATED: …` — then the error's
//! `Display`. A boundary name gains the family's `prefix` through
//! `extendedresearch_abi::codes::token`, the step the Node and .NET layers take,
//! so one failure reads the same in all three.
//!
//! # pyo3 moves in lockstep with the consumer
//!
//! `pyo3-ffi` declares `links = "python"`, and Cargo permits one package per
//! `links` value in a build graph. The consumer's pyo3 requirement must resolve
//! to the version this crate's resolves to — the `0.29` series today, the same
//! requirement ranvier's `bindings/python/Cargo.toml` states. A mismatch is a
//! resolution error, not a second copy. `cargo tree -i pyo3-ffi` in the
//! consumer lists exactly one version.
//!
//! This crate enables `abi3-py311` and leaves `extension-module` to the
//! consumer's `cdylib`.

use extendedresearch_abi::codes::{self, AbiError, ERR_PANIC, ERR_RANGE, ERR_UTF8, is_domain};
use extendedresearch_abi::enumeration::Enumeration;
use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Re-exported so [`exceptions!`] expands against the same pyo3 this crate
/// was built with. A consumer still names pyo3 in its own manifest, at the same
/// requirement, for its `#[pymodule]`.
pub use pyo3;

/// A package's exception hierarchy, as [`exceptions!`] generates it.
///
/// Implemented by the unit struct the macro declares. Nothing else needs to
/// implement it by hand.
pub trait ExceptionFamily {
    /// The prefix every constant of the package begins with, such as `CA3`.
    ///
    /// A message leads with the constant's name as the header spells it, so a
    /// boundary name gains this prefix (`CA3_ERR_NULL`), exactly as it does in
    /// the Node and .NET layers.
    const PREFIX: &'static str;

    /// The package's base error: `ERR_NULL`, `ERR_STATE`, and any code nothing
    /// more specific claims.
    fn base(message: String) -> PyErr;

    /// The package's `PanicError`, a subclass of the base error.
    fn panic(message: String) -> PyErr;

    /// The exception a domain code raises, when the family maps one.
    fn domain(code: i32) -> Option<fn(String) -> PyErr>;

    /// Add every exception the macro created to `module`, under its own name.
    ///
    /// # Errors
    ///
    /// When `module` refuses an attribute.
    fn register(module: &Bound<'_, PyModule>) -> PyResult<()>;
}

/// Create a package's exception hierarchy in the calling crate.
///
/// ```
/// use extendedresearch_pyo3::exceptions;
///
/// pub const THING_ERR_REFUSED: i32 = -16;
/// pub const THING_ERR_SLOW: i32 = -17;
///
/// exceptions! {
///     /// Every exception `_thing` raises.
///     pub family ThingExceptions in _thing;
///     prefix "THING";
///     base ThingError: "Anything thing refused.";
///     panic PanicError: "A panic was caught; thing's state is unknown.";
///     exception RefusedError(ThingError): "Thing refused the request.";
///     domain THING_ERR_REFUSED => RefusedError;
///     domain THING_ERR_SLOW => pyo3::exceptions::PyTimeoutError;
/// }
/// ```
///
/// - `family Name in module;` declares the unit struct that implements
///   [`ExceptionFamily`], and the Python module name the classes report as
///   their `__module__`.
/// - `prefix` is what every constant of the package begins with. A boundary
///   name gains it in each message (`THING_ERR_NULL: …`); it is required,
///   because the unprefixed name is the one no header declares.
/// - `base` subclasses `Exception`; `panic` subclasses `base`.
/// - Each `exception Name(Parent)` creates one more class. `Parent` is any
///   exception type: the base, another created here, or a builtin such as
///   `pyo3::exceptions::PyOSError`.
/// - Each `domain CODE => Type` raises `Type` for that code. `Type` may be a
///   class created here or a builtin.
///
/// Call `ThingExceptions::register(module)` from the `#[pymodule]`, and pass
/// `ThingExceptions` to [`error`] or [`Raise::raise`].
#[macro_export]
macro_rules! exceptions {
    (
        $(#[$meta:meta])*
        $vis:vis family $family:ident in $module:ident;
        prefix $prefix:literal;
        base $base:ident: $base_doc:literal;
        panic $panic:ident: $panic_doc:literal;
        $( exception $name:ident($parent:ty): $doc:literal; )*
        $( domain $code:expr => $target:ty; )*
    ) => {
        $crate::pyo3::create_exception!(
            $module,
            $base,
            $crate::pyo3::exceptions::PyException,
            $base_doc
        );
        $crate::pyo3::create_exception!($module, $panic, $base, $panic_doc);
        $( $crate::pyo3::create_exception!($module, $name, $parent, $doc); )*

        $(#[$meta])*
        $vis struct $family;

        impl $crate::ExceptionFamily for $family {
            const PREFIX: &'static str = $prefix;

            fn base(message: ::std::string::String) -> $crate::pyo3::PyErr {
                $base::new_err(message)
            }

            fn panic(message: ::std::string::String) -> $crate::pyo3::PyErr {
                $panic::new_err(message)
            }

            fn domain(
                code: i32,
            ) -> ::core::option::Option<fn(::std::string::String) -> $crate::pyo3::PyErr> {
                $(
                    if code == $code {
                        return ::core::option::Option::Some(|message| {
                            $crate::pyo3::PyErr::new::<$target, _>(message)
                        });
                    }
                )*
                let _ = code;
                ::core::option::Option::None
            }

            fn register(
                module: &$crate::pyo3::Bound<'_, $crate::pyo3::types::PyModule>,
            ) -> $crate::pyo3::PyResult<()> {
                use $crate::pyo3::types::PyModuleMethods as _;
                let py = module.py();
                module.add(::core::stringify!($base), py.get_type::<$base>())?;
                module.add(::core::stringify!($panic), py.get_type::<$panic>())?;
                $( module.add(::core::stringify!($name), py.get_type::<$name>())?; )*
                ::core::result::Result::Ok(())
            }
        }
    };
}

/// The exception `F` raises for a core error.
///
/// See the crate documentation for which code raises what.
pub fn error<F, E>(error: &E) -> PyErr
where
    F: ExceptionFamily,
    E: AbiError + ?Sized,
{
    code_error::<F>(error.code(), error.name(), &error.to_string())
}

/// The exception `F` raises for a code, its constant's name, and a sentence.
///
/// For a failure that did not arrive as an [`AbiError`] value — a code read
/// from a table, or one the binding itself decided on. `name` may be the
/// unprefixed boundary name or the header's full spelling; the message carries
/// the full spelling either way.
pub fn code_error<F: ExceptionFamily>(code: i32, name: &str, detail: &str) -> PyErr {
    let message = format!("{}: {detail}", codes::token(F::PREFIX, name));
    match code {
        ERR_UTF8 => PyValueError::new_err(message),
        ERR_RANGE => PyIndexError::new_err(message),
        ERR_PANIC => F::panic(message),
        domain if is_domain(domain) => match F::domain(domain) {
            Some(raise) => raise(message),
            None => F::base(message),
        },
        _ => F::base(message),
    }
}

/// `result.raise::<Family>()?` in a `#[pyfunction]`, for a core `Result`.
///
/// A binding cannot write `impl From<CoreError> for PyErr` when the core error
/// is defined in another crate — both types are foreign to it — so this is the
/// conversion at each call instead.
pub trait Raise<T> {
    /// `Ok` unchanged, and `Err` as the exception `F` raises for it.
    ///
    /// # Errors
    ///
    /// The exception [`error`] builds, when `self` is `Err`.
    fn raise<F: ExceptionFamily>(self) -> PyResult<T>;
}

impl<T, E: AbiError> Raise<T> for Result<T, E> {
    fn raise<F: ExceptionFamily>(self) -> PyResult<T> {
        self.map_err(|failure| error::<F, E>(&failure))
    }
}

/// How many leading bytes every name shares, cut back to just past an
/// underscore.
///
/// `["ORIGIN_UNSPECIFIED", "ORIGIN_RAW"]` gives 7, the length of `ORIGIN_`.
/// Names with nothing in common, or a common run with no underscore in it,
/// give 0 and nothing is stripped. The prefix is derived from the names rather
/// than written down, because a family's C constant prefix and its contract
/// prefix can differ (ranvier's `RANVIER_ASSERTED_BY_*` names are
/// `ASSERTION_METHOD_*`).
#[must_use]
pub fn shared_prefix<S: AsRef<str>>(names: &[S]) -> usize {
    let Some(first) = names.first().map(AsRef::as_ref) else {
        return 0;
    };
    let shared = names
        .iter()
        .map(|name| {
            first
                .bytes()
                .zip(name.as_ref().bytes())
                .take_while(|(a, b)| a == b)
                .count()
        })
        .min()
        .unwrap_or(0);
    first
        .get(..shared)
        .and_then(|head| head.rfind('_'))
        .map_or(0, |at| at + 1)
}

/// The short name for one member: `name` with the first `strip` bytes removed,
/// or the whole name when that leaves nothing or leaves a leading digit.
///
/// A leading digit is not a Python identifier (`Hz.50` does not parse), and an
/// empty name is no name, so either keeps the contract's full spelling.
#[must_use]
pub fn short_name(name: &str, strip: usize) -> &str {
    match name.get(strip..) {
        Some(short) if !short.is_empty() && !short.starts_with(|c: char| c.is_ascii_digit()) => {
            short
        }
        _ => name,
    }
}

/// Every name an enumeration binds, in the order they are bound: for each
/// member, its short name, then the contract's full name as an alias when it
/// differs.
///
/// The short name comes first so `IntEnum` makes it canonical: `Origin.RAW` is
/// the member, and `Origin.ORIGIN_RAW` resolves to the same member.
#[must_use]
pub fn member_names(table: &Enumeration) -> Vec<(&'static str, i32)> {
    let names: Vec<&str> = table.entries().iter().map(|(_, name)| *name).collect();
    let strip = shared_prefix(&names);
    let mut bound = Vec::with_capacity(names.len() * 2);
    for (value, name) in table.entries() {
        let short = short_name(name, strip);
        bound.push((short, *value));
        if short != *name {
            bound.push((*name, *value));
        }
    }
    bound
}

/// Build a Python `IntEnum` named `class` from a foundation [`Enumeration`].
///
/// `module` is what the class reports as its `__module__`, which is what
/// `pickle` and `repr` use; pass the Python package's name. `doc` becomes the
/// class's `__doc__`. Members are bound as [`member_names`] lists them.
///
/// # Errors
///
/// When `enum` cannot be imported, or `IntEnum` refuses the members — which it
/// does for a name bound twice to different values.
pub fn int_enum<'py>(
    py: Python<'py>,
    class: &str,
    module: &str,
    doc: &str,
    table: &Enumeration,
) -> PyResult<Bound<'py, PyAny>> {
    let members = PyDict::new(py);
    for (name, value) in member_names(table) {
        members.set_item(name, value)?;
    }
    let options = PyDict::new(py);
    options.set_item("module", module)?;
    let built = py
        .import("enum")?
        .getattr("IntEnum")?
        .call((class, &members), Some(&options))?;
    built.setattr("__doc__", doc)?;
    Ok(built)
}
