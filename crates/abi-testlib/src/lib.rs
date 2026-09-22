//! A C library built only to be called from foundation's tests.
//!
//! It has the shape a package has under the layered architecture: a safe
//! core — [`Counter`], whose failures are a [`CounterError`] that implements
//! [`AbiError`] — and a C adapter of `extern "C"` functions over it. Python and
//! Node would call the core; .NET and C call the adapter, which is what
//! `dotnet/Interop.Tests` does.
//!
//! ```c
//! uint32_t testlib_abi_version(void);
//! int32_t  testlib_counter_create(const char *name, uint32_t limit, testlib_counter_t **out_counter);
//! void     testlib_counter_destroy(testlib_counter_t *counter);
//! int32_t  testlib_counter_name(const testlib_counter_t *counter, char *destination, uint64_t capacity, uint64_t *out_len);
//! int32_t  testlib_counter_name_bytes(const testlib_counter_t *counter, uint8_t *destination, uint64_t capacity, uint64_t *out_len);
//! int32_t  testlib_counter_increment(testlib_counter_t *counter, uint32_t *out_value);
//! int32_t  testlib_counter_close(testlib_counter_t *counter);
//! int32_t  testlib_panic(void);
//! uint64_t testlib_destroyed_count(void);
//! int32_t  testlib_color_count(uint32_t *out_count);
//! int32_t  testlib_color_at(uint32_t index, int32_t *out_value);
//! int32_t  testlib_color_name(int32_t value, char *destination, uint64_t capacity, uint64_t *out_len);
//! ```

// Exporting a symbol takes `#[unsafe(no_mangle)]`, and the adapter's calls
// into `borrow` are `unsafe` blocks, so this crate allows what the workspace
// denies. `Cargo.toml` says so beside the manifest.
#![allow(unsafe_code)]

use std::ffi::c_char;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use extendedresearch_abi::borrow;
use extendedresearch_abi::enumeration::Enumeration;
use extendedresearch_status::codes::{self, AbiError, DOMAIN_FLOOR, ERR_NULL, ERR_STATE, OK};
use extendedresearch_status::guard;

/// The ABI version this library implements.
pub const TESTLIB_ABI_VERSION: u32 = 1;

/// A counter was created with an empty name.
pub const TESTLIB_ERR_EMPTY_NAME: i32 = DOMAIN_FLOOR;

/// A counter reached its limit.
pub const TESTLIB_ERR_FULL: i32 = DOMAIN_FLOOR - 1;

/// Every domain code, as the conformance kit and the bindings read them.
pub const ERROR_CODES: &[(i32, &str)] = &[
    (TESTLIB_ERR_EMPTY_NAME, "TESTLIB_ERR_EMPTY_NAME"),
    (TESTLIB_ERR_FULL, "TESTLIB_ERR_FULL"),
];

/// Red.
pub const TESTLIB_COLOR_RED: i32 = 0;
/// Green.
pub const TESTLIB_COLOR_GREEN: i32 = 1;
/// Blue.
pub const TESTLIB_COLOR_BLUE: i32 = 7;

/// The colours, behind `testlib_color_count`, `_at` and `_name`.
pub static COLORS: Enumeration = Enumeration::new(&[
    (TESTLIB_COLOR_RED, "COLOR_RED"),
    (TESTLIB_COLOR_GREEN, "COLOR_GREEN"),
    (TESTLIB_COLOR_BLUE, "COLOR_BLUE"),
]);

// -- the core ---------------------------------------------------------------

/// A named counter with a limit: the core's one object.
#[derive(Debug)]
pub struct Counter {
    name: String,
    count: u32,
    limit: u32,
    closed: bool,
}

/// Why a counter refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CounterError {
    /// The name was empty.
    EmptyName,
    /// The counter is at its limit.
    Full(u32),
    /// The counter was closed.
    Closed,
}

impl fmt::Display for CounterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => f.write_str("a counter needs a name"),
            Self::Full(limit) => write!(f, "the counter is at its limit of {limit}"),
            Self::Closed => f.write_str("the counter is closed"),
        }
    }
}

impl AbiError for CounterError {
    fn code(&self) -> i32 {
        match self {
            Self::EmptyName => TESTLIB_ERR_EMPTY_NAME,
            Self::Full(_) => TESTLIB_ERR_FULL,
            Self::Closed => ERR_STATE,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::EmptyName => "TESTLIB_ERR_EMPTY_NAME",
            Self::Full(_) => "TESTLIB_ERR_FULL",
            Self::Closed => "ERR_STATE",
        }
    }
}

impl Counter {
    /// A counter at zero that refuses to pass `limit`.
    ///
    /// # Errors
    ///
    /// [`CounterError::EmptyName`] for an empty name.
    pub fn new(name: &str, limit: u32) -> Result<Self, CounterError> {
        if name.is_empty() {
            return Err(CounterError::EmptyName);
        }
        Ok(Self {
            name: name.to_owned(),
            count: 0,
            limit,
            closed: false,
        })
    }

    /// The counter's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Count one, and answer the new count.
    ///
    /// # Errors
    ///
    /// [`CounterError::Closed`] after [`Counter::close`], and
    /// [`CounterError::Full`] at the limit.
    pub fn increment(&mut self) -> Result<u32, CounterError> {
        if self.closed {
            return Err(CounterError::Closed);
        }
        if self.count >= self.limit {
            return Err(CounterError::Full(self.limit));
        }
        self.count += 1;
        Ok(self.count)
    }

    /// Refuse every later increment.
    pub fn close(&mut self) {
        self.closed = true;
    }
}

// -- the C adapter ----------------------------------------------------------

static DESTROYED: AtomicU64 = AtomicU64::new(0);

/// The ABI version this library implements.
#[unsafe(no_mangle)]
pub extern "C" fn testlib_abi_version() -> u32 {
    TESTLIB_ABI_VERSION
}

/// How many counters `testlib_counter_destroy` has freed, so a test can see a
/// .NET finalizer reach it.
#[unsafe(no_mangle)]
pub extern "C" fn testlib_destroyed_count() -> u64 {
    DESTROYED.load(Ordering::SeqCst)
}

/// Create a counter.
///
/// # Safety
///
/// `name` is null or a null-terminated string valid for the call;
/// `out_counter` is null or valid for writing one pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn testlib_counter_create(
    name: *const c_char,
    limit: u32,
    out_counter: *mut *mut Counter,
) -> i32 {
    guard::guard(|| {
        // SAFETY: the caller's promise for `out_counter`, passed on unchanged.
        let Some(slot) = (unsafe { borrow::out(out_counter) }) else {
            return ERR_NULL;
        };
        // SAFETY: the caller's promise for `name`, passed on unchanged.
        let name = match unsafe { borrow::text(name) } {
            Ok(Some(name)) => name,
            Ok(None) => return ERR_NULL,
            Err(code) => return code,
        };
        codes::status(Counter::new(name, limit), |counter| {
            slot.write(borrow::into_handle(counter));
            OK
        })
    })
}

/// Free a counter. Null is a no-op; any thread may call it.
///
/// # Safety
///
/// `counter` is null, or a handle `testlib_counter_create` answered that has
/// not been destroyed and that nothing else is using.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn testlib_counter_destroy(counter: *mut Counter) {
    guard::contain(|| {
        // SAFETY: the caller's promise above, passed on unchanged.
        if let Some(counter) = unsafe { borrow::reclaim(counter) } {
            drop(counter);
            DESTROYED.fetch_add(1, Ordering::SeqCst);
        }
    });
}

/// A counter's name, as text.
///
/// # Safety
///
/// `counter` is null or a live handle; `destination` is null or valid for
/// writing `capacity` bytes; `out_len` is null or valid for writing one `u64`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn testlib_counter_name(
    counter: *const Counter,
    destination: *mut c_char,
    capacity: u64,
    out_len: *mut u64,
) -> i32 {
    guard::guard(|| {
        // SAFETY: the caller's promise for `counter`, passed on unchanged.
        let Some(counter) = (unsafe { borrow::handle(counter) }) else {
            return ERR_NULL;
        };
        // SAFETY: the caller's promise for the buffer, passed on unchanged.
        unsafe { borrow::fill_text(counter.name(), destination, capacity, out_len) }
    })
}

/// A counter's name, as a byte run with no terminator.
///
/// # Safety
///
/// As [`testlib_counter_name`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn testlib_counter_name_bytes(
    counter: *const Counter,
    destination: *mut u8,
    capacity: u64,
    out_len: *mut u64,
) -> i32 {
    guard::guard(|| {
        // SAFETY: the caller's promise for `counter`, passed on unchanged.
        let Some(counter) = (unsafe { borrow::handle(counter) }) else {
            return ERR_NULL;
        };
        // SAFETY: the caller's promise for the buffer, passed on unchanged.
        unsafe { borrow::fill_bytes(counter.name().as_bytes(), destination, capacity, out_len) }
    })
}

/// Count one.
///
/// # Safety
///
/// `counter` is null or a live handle no other thread is using; `out_value` is
/// null or valid for writing one `u32`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn testlib_counter_increment(
    counter: *mut Counter,
    out_value: *mut u32,
) -> i32 {
    guard::guard(|| {
        // SAFETY: the caller's promise for `counter`, passed on unchanged.
        let Some(counter) = (unsafe { borrow::handle_mut(counter) }) else {
            return ERR_NULL;
        };
        // SAFETY: the caller's promise for `out_value`, passed on unchanged.
        let Some(slot) = (unsafe { borrow::out(out_value) }) else {
            return ERR_NULL;
        };
        codes::status(counter.increment(), |value| {
            slot.write(value);
            OK
        })
    })
}

/// Close a counter, so every later increment is `ERR_STATE`.
///
/// # Safety
///
/// `counter` is null or a live handle no other thread is using.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn testlib_counter_close(counter: *mut Counter) -> i32 {
    guard::guard(|| {
        // SAFETY: the caller's promise for `counter`, passed on unchanged.
        let Some(counter) = (unsafe { borrow::handle_mut(counter) }) else {
            return ERR_NULL;
        };
        counter.close();
        OK
    })
}

/// Panic inside the guard, so a caller sees `ERR_PANIC`.
#[unsafe(no_mangle)]
pub extern "C" fn testlib_panic() -> i32 {
    guard::guard(|| panic!("testlib_panic panics on purpose"))
}

/// `_count` for [`COLORS`].
///
/// # Safety
///
/// `out_count` is null or valid for writing one `u32`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn testlib_color_count(out_count: *mut u32) -> i32 {
    // SAFETY: the caller's promise above, passed on unchanged.
    guard::guard(|| unsafe { borrow::enumeration_count(&COLORS, out_count) })
}

/// `_at` for [`COLORS`].
///
/// # Safety
///
/// `out_value` is null or valid for writing one `i32`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn testlib_color_at(index: u32, out_value: *mut i32) -> i32 {
    // SAFETY: the caller's promise above, passed on unchanged.
    guard::guard(|| unsafe { borrow::enumeration_at(&COLORS, index, out_value) })
}

/// `_name` for [`COLORS`].
///
/// # Safety
///
/// As [`testlib_counter_name`], without the handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn testlib_color_name(
    value: i32,
    destination: *mut c_char,
    capacity: u64,
    out_len: *mut u64,
) -> i32 {
    guard::guard(|| {
        // SAFETY: the caller's promise above, passed on unchanged.
        unsafe { borrow::enumeration_name(&COLORS, value, destination, capacity, out_len) }
    })
}
