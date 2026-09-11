//! The conformance kit catches each defect it claims to.
//!
//! A kit that passed everything would be indistinguishable from one that
//! checked nothing, so every rule it enforces has a function here that breaks
//! exactly that rule, and a test that the kit refuses it.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
#![allow(unsafe_code, clippy::undocumented_unsafe_blocks)]

use std::ffi::c_char;
use std::panic::{AssertUnwindSafe, catch_unwind};

use extendedresearch_abi::codes::{ERR_NULL, ERR_RANGE, OK};
use extendedresearch_abi::conformance::{bytes_answer, text_answer};

/// A hand-written text answer with one knob per rule, so each test breaks one.
#[derive(Clone, Copy, Default)]
struct Text {
    count_terminator: bool,
    skip_terminator: bool,
    write_on_refusal: bool,
    overrun: bool,
    accept_null_out_len: bool,
    forget_len_on_refusal: bool,
}

impl Text {
    fn call(self, value: &[u8], destination: *mut c_char, capacity: u64, out_len: *mut u64) -> i32 {
        let reported = value.len() as u64 + u64::from(self.count_terminator);
        if out_len.is_null() {
            return if self.accept_null_out_len {
                OK
            } else {
                ERR_NULL
            };
        }
        let out_len = unsafe { &mut *out_len };
        *out_len = reported;
        if destination.is_null() {
            return OK;
        }
        let destination = destination.cast::<u8>();
        let needed = value.len() as u64 + u64::from(!self.skip_terminator);
        if capacity < needed {
            if self.forget_len_on_refusal {
                *out_len = 0;
            }
            if self.write_on_refusal && capacity > 0 {
                unsafe { destination.write(b'!') };
            }
            return ERR_RANGE;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(value.as_ptr(), destination, value.len());
            if !self.skip_terminator {
                destination.add(value.len()).write(0);
            }
            if self.overrun {
                // Past the terminator, into the byte the kit left as room.
                destination.add(value.len() + 1).write(b'!');
            }
        }
        OK
    }
}

fn refuses(text: Text, value: &'static [u8]) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        text_answer(|d, c, l| text.call(value, d, c, l))
    }))
    .is_err()
}

#[test]
fn a_correct_function_passes() {
    assert_eq!(
        text_answer(|d, c, l| Text::default().call(b"name", d, c, l)),
        "name"
    );
}

#[test]
fn counting_the_terminator_in_out_len_is_caught() {
    assert!(refuses(
        Text {
            count_terminator: true,
            ..Text::default()
        },
        b"name"
    ));
}

#[test]
fn not_writing_a_terminator_is_caught() {
    assert!(refuses(
        Text {
            skip_terminator: true,
            ..Text::default()
        },
        b"name"
    ));
}

#[test]
fn writing_into_the_buffer_on_a_refusal_is_caught() {
    assert!(refuses(
        Text {
            write_on_refusal: true,
            ..Text::default()
        },
        b"name"
    ));
}

#[test]
fn writing_past_the_answer_is_caught() {
    assert!(refuses(
        Text {
            overrun: true,
            ..Text::default()
        },
        b"name"
    ));
}

#[test]
fn accepting_a_null_out_len_is_caught() {
    assert!(refuses(
        Text {
            accept_null_out_len: true,
            ..Text::default()
        },
        b"name"
    ));
}

#[test]
fn a_refusal_that_does_not_report_the_size_is_caught() {
    assert!(refuses(
        Text {
            forget_len_on_refusal: true,
            ..Text::default()
        },
        b"name"
    ));
}

#[test]
fn text_with_an_interior_null_is_caught() {
    assert!(refuses(Text::default(), b"na\0me"));
}

#[test]
fn text_that_is_not_utf8_is_caught() {
    assert!(refuses(Text::default(), b"\xFF\xFE"));
}

#[test]
fn a_byte_run_that_demands_a_terminator_is_caught() {
    // Treating a byte run like text — refusing capacity == len — is the other
    // half of the one-byte rule.
    let as_text = Text::default();
    let refused = catch_unwind(AssertUnwindSafe(|| {
        bytes_answer(|d, c, l| as_text.call(b"abc", d.cast(), c, l))
    }))
    .is_err();
    assert!(refused);
}

#[test]
fn a_correct_byte_run_passes() {
    let as_bytes = Text {
        skip_terminator: true,
        ..Text::default()
    };
    assert_eq!(
        bytes_answer(|d, c, l| as_bytes.call(&[1, 0, 2], d.cast(), c, l)),
        [1, 0, 2],
        "a byte run may contain zeros"
    );
}
