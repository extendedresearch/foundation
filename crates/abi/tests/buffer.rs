//! The safe half of measure-then-copy, through the views `borrow` makes.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
// Reading a `MaybeUninit` back needs `assume_init`, and every buffer here starts
// initialised to a sentinel so that it is sound.
#![allow(unsafe_code, clippy::undocumented_unsafe_blocks)]

use std::mem::MaybeUninit;

use extendedresearch_abi::buffer::{fill_bytes, fill_text, measure};
use extendedresearch_abi::codes::{ERR_NULL, ERR_RANGE, OK};

fn buffer<const N: usize>() -> [MaybeUninit<u8>; N] {
    [MaybeUninit::new(0xAA); N]
}

fn read(buffer: &[MaybeUninit<u8>]) -> Vec<u8> {
    buffer
        .iter()
        .map(|byte| unsafe { byte.assume_init() })
        .collect()
}

fn slot() -> MaybeUninit<u64> {
    MaybeUninit::new(u64::MAX)
}

fn value(slot: &MaybeUninit<u64>) -> u64 {
    unsafe { slot.assume_init() }
}

#[test]
fn measuring_writes_the_length_and_nothing_else() {
    let mut len = slot();
    assert_eq!(fill_bytes(b"abcd", None, Some(&mut len)), OK);
    assert_eq!(value(&len), 4);
}

#[test]
fn a_missing_out_len_is_refused_before_anything_is_written() {
    let mut room = buffer::<8>();
    assert_eq!(fill_bytes(b"abcd", Some(&mut room), None), ERR_NULL);
    assert_eq!(fill_text("abcd", Some(&mut room), None), ERR_NULL);
    assert_eq!(measure(4, None), ERR_NULL);
    assert_eq!(
        read(&room),
        [0xAA; 8],
        "wrote with nowhere to report the length"
    );
}

#[test]
fn a_short_buffer_is_refused_and_still_reports_what_was_needed() {
    // The property the whole shape rests on: a caller that guessed low can
    // allocate exactly, because the refusal carried the size.
    let mut small = buffer::<2>();
    let mut len = slot();
    assert_eq!(
        fill_bytes(b"abcd", Some(&mut small), Some(&mut len)),
        ERR_RANGE
    );
    assert_eq!(
        value(&len),
        4,
        "a refusal must still say how much was needed"
    );
    assert_eq!(read(&small), [0xAA; 2], "a refusal wrote into the buffer");
}

#[test]
fn text_length_excludes_the_terminator_and_capacity_includes_it() {
    let mut len = slot();
    assert_eq!(fill_text("abcd", None, Some(&mut len)), OK);
    assert_eq!(value(&len), 4, "out_len never counts the terminator");

    // Exactly out_len is one byte short for text, and enough for bytes.
    let mut exact = buffer::<4>();
    assert_eq!(
        fill_text("abcd", Some(&mut exact), Some(&mut slot())),
        ERR_RANGE
    );
    let mut exact_bytes = buffer::<4>();
    assert_eq!(
        fill_bytes(b"abcd", Some(&mut exact_bytes), Some(&mut slot())),
        OK
    );
    assert_eq!(read(&exact_bytes), b"abcd");

    // out_len + 1 is right for text.
    let mut room = buffer::<5>();
    assert_eq!(fill_text("abcd", Some(&mut room), Some(&mut slot())), OK);
    assert_eq!(read(&room), b"abcd\0", "text is written null-terminated");
}

#[test]
fn an_empty_answer_is_success_not_an_error() {
    let mut len = slot();
    assert_eq!(fill_text("", None, Some(&mut len)), OK);
    assert_eq!(value(&len), 0);

    let mut one = buffer::<1>();
    assert_eq!(fill_text("", Some(&mut one), Some(&mut slot())), OK);
    assert_eq!(read(&one), [0]);

    let mut none: [MaybeUninit<u8>; 0] = [];
    assert_eq!(fill_bytes(b"", Some(&mut none), Some(&mut slot())), OK);
}

#[test]
fn a_larger_buffer_is_not_overwritten_past_what_was_asked_for() {
    let mut room = buffer::<8>();
    assert_eq!(fill_bytes(b"ab", Some(&mut room), Some(&mut slot())), OK);
    assert_eq!(
        read(&room),
        [b'a', b'b', 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA]
    );

    let mut room = buffer::<8>();
    assert_eq!(fill_text("ab", Some(&mut room), Some(&mut slot())), OK);
    assert_eq!(read(&room), [b'a', b'b', 0, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA]);
}

#[test]
fn measure_reports_without_a_buffer() {
    let mut len = slot();
    assert_eq!(measure(12, Some(&mut len)), OK);
    assert_eq!(value(&len), 12);
}
