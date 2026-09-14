//! The part of a package's napi-rs binding that every package writes the same
//! way.
//!
//! A package's Node binding calls the package's safe Rust core directly. It
//! holds no raw handle and writes no `unsafe`. What it needs from here:
//!
//! | | |
//! |---|---|
//! | [`Tokens`] | The error token protocol for one package: an [`AbiError`] becomes a `napi::Error` whose message is `"<PREFIX>_ERR_X: sentence"` |
//! | [`Tokens::to_u64`], [`from_u64`] | `BigInt` to `u64` and back, refusing a value that does not fit |
//! | [`status_exports!`], [`abi_version_exports!`], [`enumeration_exports!`] | The `#[napi]` functions a binding exports, expanded **in the consuming crate** |
//! | `@extendedresearch/binding-runtime` | The TypeScript half — `errors`, `harden` and `enums` — which is an npm package attached to foundation's releases rather than part of this crate |
//!
//! # Why the code travels in the message
//!
//! napi-rs carries a JavaScript error's `code` in its `status`, and `status` is
//! Node-API's own fixed set. `napi::Error<String>` can widen it for a
//! synchronous function, but `#[napi] async fn` requires `Error<Status>`. So
//! the native half reports the package's constant name as the first token of
//! the message, `": "`, then the sentence, and
//! `@extendedresearch/binding-runtime/errors` splits on the
//! first `": "`, looks the token up in the set `statusCodes()` reports, and
//! raises the package's error class. A token not in the set is not the
//! package's, so a napi-rs failure of its own passes through unrelabelled.
//!
//! # Why the exports are macros
//!
//! Whether a `#[napi]` item compiled in a dependency is registered with the
//! consumer's module has not been checked: napi-rs registers exports from
//! static constructors, and a linker may drop an unreferenced one from an
//! rlib. A macro puts each `#[napi]` item in the consumer's own crate, where
//! registration is the ordinary case. The consumer names `napi` and
//! `napi_derive` in its own manifest, because the generated code refers to
//! them by those names.
//!
//! # napi moves in lockstep with the consumer
//!
//! `napi-sys` declares no `links` value, so nothing stops a consumer from
//! resolving a second napi beside this crate's, and a `napi::Error` built here
//! would then be a different type from the consumer's. Require the same series
//! this crate does — `3` for napi and napi-derive, the pin
//! `docs/conventions/toolchain-pins.md` states. `cargo tree -i napi` in the
//! consumer lists exactly one version.

use std::fmt::Display;

use extendedresearch_abi::codes::{self, AbiError};
use napi::bindgen_prelude::BigInt;
use napi::{Error, Status};

/// The separator between the token and the sentence in an error's message.
///
/// `@extendedresearch/binding-runtime/errors` states the same string as
/// `SEPARATOR`.
pub const SEPARATOR: &str = ": ";

/// The error token protocol for one package.
///
/// ```
/// use extendedresearch_napi::Tokens;
///
/// static TOKENS: Tokens = Tokens::new("EXAMPLE");
///
/// assert_eq!(TOKENS.token("ERR_NULL"), "EXAMPLE_ERR_NULL");
/// assert_eq!(TOKENS.token("EXAMPLE_ERR_TIMEOUT"), "EXAMPLE_ERR_TIMEOUT");
/// assert_eq!(TOKENS.binding_token(), "EXAMPLE_ERR_BINDING");
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Tokens {
    prefix: &'static str,
}

impl Tokens {
    /// The protocol for the package whose constants begin `prefix` and `_`.
    #[must_use]
    pub const fn new(prefix: &'static str) -> Self {
        Self { prefix }
    }

    /// The prefix this protocol was made with.
    #[must_use]
    pub const fn prefix(&self) -> &'static str {
        self.prefix
    }

    /// A constant's name as the package's header spells it.
    ///
    /// A boundary name (`ERR_NULL`, `OK`) gains the prefix; a name that already
    /// carries it (`EXAMPLE_ERR_TRUNCATED`) is unchanged. [`AbiError::name`]
    /// answers the first kind for boundary codes and the second for domain
    /// codes, so this is the one step between them. It is
    /// `extendedresearch_abi::codes::token`, which the pyo3 layer applies too.
    #[must_use]
    pub fn token(&self, name: &str) -> String {
        codes::token(self.prefix, name)
    }

    /// The token for something the binding could not do, rather than a
    /// failure the package answered: `<PREFIX>_ERR_BINDING`.
    #[must_use]
    pub fn binding_token(&self) -> String {
        format!("{}_ERR_BINDING", self.prefix)
    }

    /// The token for a code this build has no name for: `<PREFIX>_ERR_UNKNOWN`.
    #[must_use]
    pub fn unknown_token(&self) -> String {
        format!("{}_ERR_UNKNOWN", self.prefix)
    }

    /// [`Self::binding_token`] and [`Self::unknown_token`], for the
    /// TypeScript side's set of codes.
    #[must_use]
    pub fn binding_codes(&self) -> Vec<String> {
        vec![self.binding_token(), self.unknown_token()]
    }

    /// Every status the package's header declares, as `(token, value)`: `OK`,
    /// each code in `boundary`, then each domain code in `domain`, in the
    /// order given.
    ///
    /// `boundary` lists the boundary codes the header declares, which need not
    /// be all of them: a package that never answers `ERR_STATE` leaves it out
    /// of its header and out of here, and `statusCodes()` then does not report
    /// it. A code foundation has no name for is skipped.
    ///
    /// `domain` is the table the package passes to
    /// `extendedresearch_abi::conformance::error_codes`. An entry that is not a
    /// domain code is skipped, so a table listing every status can be passed
    /// whole without naming a boundary code twice.
    #[must_use]
    pub fn status_table(&self, boundary: &[i32], domain: &[(i32, &str)]) -> Vec<(String, i32)> {
        let mut table = vec![(self.token("OK"), codes::OK)];
        table.extend(
            boundary
                .iter()
                .filter_map(|&code| codes::name(code).map(|name| (self.token(name), code))),
        );
        table.extend(
            domain
                .iter()
                .filter(|(code, _)| codes::is_domain(*code))
                .map(|(code, name)| (self.token(name), *code)),
        );
        table
    }

    /// The `napi::Error` for a core error: `"<token>: <Display>"`.
    #[must_use]
    pub fn error<E: AbiError + ?Sized>(&self, error: &E) -> Error {
        self.named(error.name(), error)
    }

    /// The `napi::Error` for a constant's name and a sentence.
    #[must_use]
    pub fn named(&self, name: &str, detail: impl Display) -> Error {
        Error::new(
            Status::GenericFailure,
            format!("{}{SEPARATOR}{detail}", self.token(name)),
        )
    }

    /// The `napi::Error` for something the binding could not do.
    #[must_use]
    pub fn binding_failure(&self, detail: impl Display) -> Error {
        Error::new(
            Status::GenericFailure,
            format!("{}{SEPARATOR}{detail}", self.binding_token()),
        )
    }

    /// Read a `BigInt` as a `u64`, refusing anything that would not survive.
    ///
    /// napi-rs's own `get_u64` truncates a value wider than 64 bits and reads
    /// a negative value's magnitude; either is a number the core receives wrong
    /// and reports nothing about. `what` names the argument in the refusal.
    ///
    /// # Errors
    ///
    /// [`Self::binding_failure`] for a negative value, or one wider than 64
    /// bits.
    pub fn to_u64(&self, value: &BigInt, what: &str) -> napi::Result<u64> {
        // V8 reports `0n` with no words, and `-0n` does not exist.
        let significant = value
            .words
            .iter()
            .rposition(|word| *word != 0)
            .map_or(0, |at| at + 1);
        if value.sign_bit && significant > 0 {
            return Err(self.binding_failure(format!(
                "{what} is negative and the call takes an unsigned 64-bit value"
            )));
        }
        match value.words.get(..significant) {
            Some([]) | None => Ok(0),
            Some([word]) => Ok(*word),
            Some(_) => Err(self.binding_failure(format!(
                "{what} does not fit in 64 bits and would be truncated"
            ))),
        }
    }
}

/// A `u64` as a `BigInt`, which holds every value exactly.
#[must_use]
pub fn from_u64(value: u64) -> BigInt {
    BigInt::from(value)
}

/// `result.report(&TOKENS)?` in a `#[napi]` function, for a core `Result`.
///
/// A binding cannot write `impl From<CoreError> for napi::Error` when both types
/// are foreign to it, so this is the conversion at each call instead.
pub trait Report<T> {
    /// `Ok` unchanged, and `Err` as [`Tokens::error`] builds it.
    ///
    /// # Errors
    ///
    /// The converted error, when `self` is `Err`.
    fn report(self, tokens: &Tokens) -> napi::Result<T>;
}

impl<T, E: AbiError> Report<T> for Result<T, E> {
    fn report(self, tokens: &Tokens) -> napi::Result<T> {
        self.map_err(|failure| tokens.error(&failure))
    }
}

/// Export `statusCodes()` and `bindingCodes()` from the consuming crate.
///
/// ```text
/// use extendedresearch_abi::codes::{ERR_NULL, ERR_PANIC, ERR_RANGE, ERR_UTF8};
///
/// static TOKENS: extendedresearch_napi::Tokens = extendedresearch_napi::Tokens::new("EXAMPLE");
/// const BOUNDARY_CODES: &[i32] = &[ERR_NULL, ERR_RANGE, ERR_UTF8, ERR_PANIC];
/// extendedresearch_napi::status_exports!(TOKENS, BOUNDARY_CODES, example::ERROR_CODES);
/// ```
///
/// Defines a `#[napi(object)] StatusCode { value, name }`,
/// `statusCodes(): StatusCode[]` from [`Tokens::status_table`], and
/// `bindingCodes(): string[]` from [`Tokens::binding_codes`].
/// `@extendedresearch/binding-runtime/errors` builds its set of codes from the
/// two. The boundary codes are the ones the
/// package's header declares, as [`Tokens::status_table`] describes.
#[macro_export]
macro_rules! status_exports {
    ($tokens:expr, $boundary:expr, $domain:expr $(,)?) => {
        /// One status the package can answer.
        #[::napi_derive::napi(object)]
        pub struct StatusCode {
            /// The `int32_t` the C ABI answers for it.
            pub value: i32,
            /// The constant's name as the header spells it.
            pub name: ::std::string::String,
        }

        /// Every status the package's header declares: `OK`, its boundary
        /// codes, and its own.
        #[::napi_derive::napi]
        pub fn status_codes() -> ::std::vec::Vec<StatusCode> {
            $tokens
                .status_table($boundary, $domain)
                .into_iter()
                .map(|(name, value)| StatusCode { value, name })
                .collect()
        }

        /// The two tokens this binding raises that the package has no constant
        /// for.
        #[::napi_derive::napi]
        pub fn binding_codes() -> ::std::vec::Vec<::std::string::String> {
            $tokens.binding_codes()
        }
    };
}

/// Export `abiVersion()` and `expectedAbiVersion()` from the consuming crate.
///
/// ```text
/// extendedresearch_napi::abi_version_exports!(example::abi_version(), example::ABI_VERSION);
/// ```
///
/// The TypeScript entry point compares the two for equality at import.
#[macro_export]
macro_rules! abi_version_exports {
    ($running:expr, $expected:expr $(,)?) => {
        /// The ABI version the linked library implements.
        #[::napi_derive::napi]
        pub fn abi_version() -> u32 {
            $running
        }

        /// The ABI version this binding was built against.
        #[::napi_derive::napi]
        pub fn expected_abi_version() -> u32 {
            $expected
        }
    };
}

/// Export one function per foundation `Enumeration` from the consuming crate,
/// each answering the table's members.
///
/// ```text
/// extendedresearch_napi::enumeration_exports! {
///     /// What a stream is for.
///     origins => example::ORIGINS;
///     /// Why a runtime refused a node.
///     refuse_reasons => example::REFUSE_REASONS;
/// }
/// ```
///
/// Defines a `#[napi(object)] EnumMember { value, name }` and, per line, a
/// function answering `EnumMember[]` in the table's order.
/// `@extendedresearch/binding-runtime/enums` turns each into a frozen contract. Invoke it once per crate: a second invocation
/// defines `EnumMember` twice.
#[macro_export]
macro_rules! enumeration_exports {
    ($( $(#[$meta:meta])* $function:ident => $table:expr; )+) => {
        /// One member of an enumeration: the value and the contract's name.
        #[::napi_derive::napi(object)]
        pub struct EnumMember {
            /// The value.
            pub value: i32,
            /// The contract's name for it.
            pub name: ::std::string::String,
        }

        $(
            $(#[$meta])*
            #[::napi_derive::napi]
            pub fn $function() -> ::std::vec::Vec<EnumMember> {
                $table
                    .entries()
                    .iter()
                    .map(|(value, name)| EnumMember {
                        value: *value,
                        name: (*name).to_owned(),
                    })
                    .collect()
            }
        )+
    };
}
