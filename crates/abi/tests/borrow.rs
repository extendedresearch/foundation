//! The raw-pointer half, driven the way a library's exported function drives it.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
// Calling an `unsafe fn` is what a library does at its boundary, and each call
// here passes pointers the test owns.
#![allow(unsafe_code, clippy::undocumented_unsafe_blocks)]

use std::ffi::{CString, c_char};
use std::sync::atomic::{AtomicUsize, Ordering};

use extendedresearch_abi::borrow::{
    fill_bytes, fill_text, handle, handle_mut, into_handle, out, reclaim, slice, text,
};
use extendedresearch_abi::codes::{ERR_NULL, ERR_RANGE, ERR_UTF8, OK};
use extendedresearch_abi::conformance;

#[test]
fn a_handle_goes_out_and_comes_back_and_dropping_it_frees_what_it_owns() {
    static DROPPED: AtomicUsize = AtomicUsize::new(0);
    struct Counted(u32);
    impl Drop for Counted {
        fn drop(&mut self) {
            DROPPED.fetch_add(1, Ordering::SeqCst);
        }
    }

    let raw = into_handle(Counted(7));
    assert_eq!(unsafe { handle(raw.cast_const()) }.map(|c| c.0), Some(7));
    if let Some(counted) = unsafe { handle_mut(raw) } {
        counted.0 = 8;
    }
    assert_eq!(unsafe { handle(raw.cast_const()) }.map(|c| c.0), Some(8));

    assert_eq!(DROPPED.load(Ordering::SeqCst), 0);
    drop(unsafe { reclaim(raw) });
    assert_eq!(DROPPED.load(Ordering::SeqCst), 1, "reclaiming must free");
}

#[test]
fn null_is_none_everywhere_and_destroying_null_is_a_no_op() {
    assert!(unsafe { handle::<u32>(std::ptr::null()) }.is_none());
    assert!(unsafe { handle_mut::<u32>(std::ptr::null_mut()) }.is_none());
    assert!(unsafe { reclaim::<u32>(std::ptr::null_mut()) }.is_none());
    assert!(unsafe { out::<u64>(std::ptr::null_mut()) }.is_none());
}

#[test]
fn an_out_parameter_is_written_through() {
    let mut len = 0u64;
    let slot = unsafe { out(&raw mut len) }.unwrap();
    slot.write(42);
    assert_eq!(len, 42);

    // An out-parameter for a handle is the same shape.
    let mut created: *mut u32 = std::ptr::null_mut();
    unsafe { out(&raw mut created) }
        .unwrap()
        .write(into_handle(5));
    assert_eq!(unsafe { handle(created.cast_const()) }, Some(&5));
    drop(unsafe { reclaim(created) });
}

#[test]
fn text_in_is_borrowed_null_is_absent_and_not_utf8_is_refused() {
    let name = CString::new("tracker").unwrap();
    assert_eq!(unsafe { text(name.as_ptr()) }, Ok(Some("tracker")));
    assert_eq!(unsafe { text(std::ptr::null()) }, Ok(None));

    let invalid = [0xFFu8, 0xFE, 0];
    assert_eq!(
        unsafe { text(invalid.as_ptr().cast::<c_char>()) },
        Err(ERR_UTF8),
        "not-UTF-8 must be refused rather than replaced"
    );
}

#[test]
fn an_empty_array_is_empty_whatever_its_pointer() {
    let empty: &[u8] = unsafe { slice(std::ptr::null(), 0) }.unwrap();
    assert!(empty.is_empty());
    let dangling = std::ptr::NonNull::<u32>::dangling().as_ptr().cast_const();
    assert_eq!(unsafe { slice(dangling, 0) }, Ok(&[][..]));
}

#[test]
fn a_null_array_with_a_length_is_refused() {
    assert_eq!(unsafe { slice::<u8>(std::ptr::null(), 3) }, Err(ERR_NULL));
}

#[test]
fn an_array_is_read_including_an_array_of_handles() {
    let values = [1u8, 2, 3];
    assert_eq!(unsafe { slice(values.as_ptr(), 3) }, Ok(&[1u8, 2, 3][..]));

    let a = into_handle(1u32);
    let b = into_handle(2u32);
    let handles = [a.cast_const(), b.cast_const()];
    let read = unsafe { slice(handles.as_ptr(), 2) }.unwrap();
    let values: Vec<u32> = read
        .iter()
        .map(|&h| *unsafe { handle(h) }.unwrap())
        .collect();
    assert_eq!(values, [1, 2]);
    drop(unsafe { reclaim(a) });
    drop(unsafe { reclaim(b) });
}

#[cfg(target_pointer_width = "32")]
#[test]
fn a_length_this_platform_cannot_address_is_refused() {
    let one = [0u8];
    assert_eq!(unsafe { slice(one.as_ptr(), u64::MAX) }, Err(ERR_RANGE));
}

#[test]
fn fill_text_passes_the_conformance_kit() {
    let answer = conformance::text_answer(|destination, capacity, out_len| unsafe {
        fill_text("stream/α", destination, capacity, out_len)
    });
    assert_eq!(answer, "stream/α");

    let empty = conformance::text_answer(|destination, capacity, out_len| unsafe {
        fill_text("", destination, capacity, out_len)
    });
    assert_eq!(empty, "");
}

#[test]
fn fill_bytes_passes_the_conformance_kit() {
    let answer = conformance::bytes_answer(|destination, capacity, out_len| unsafe {
        fill_bytes(&[0, 1, 2, 0xFF], destination, capacity, out_len)
    });
    assert_eq!(answer, [0, 1, 2, 0xFF]);

    let empty = conformance::bytes_answer(|destination, capacity, out_len| unsafe {
        fill_bytes(&[], destination, capacity, out_len)
    });
    assert!(empty.is_empty());
}

#[test]
fn a_destination_that_was_never_initialised_is_written_soundly() {
    // A C caller's `malloc` or a Vec's spare capacity: nothing has been written
    // to these bytes. Creating `&mut [u8]` over them would be undefined; the
    // destination is `MaybeUninit`, so it is not. Run under Miri to check.
    let mut buffer: Vec<u8> = Vec::with_capacity(16);
    let mut len = 0u64;
    let status = unsafe { fill_text("abc", buffer.as_mut_ptr().cast(), 16, &raw mut len) };
    assert_eq!(status, OK);
    assert_eq!(len, 3);
    unsafe { buffer.set_len(4) };
    assert_eq!(buffer, b"abc\0");
}

#[test]
fn a_capacity_beyond_what_the_answer_needs_is_not_reached_into() {
    // The destination view is never wider than the answer, so a caller that
    // over-reports capacity on a small buffer is not read or written past it.
    let mut small = [0xAAu8; 3];
    let mut len = 0u64;
    let status = unsafe { fill_bytes(b"ab", small.as_mut_ptr(), 1 << 40, &raw mut len) };
    assert_eq!(status, OK);
    assert_eq!(small, [b'a', b'b', 0xAA]);
}

#[test]
fn a_short_capacity_is_refused_through_the_raw_form_too() {
    let mut small = [0xAAu8; 2];
    let mut len = 0u64;
    let status = unsafe { fill_text("ab", small.as_mut_ptr().cast(), 2, &raw mut len) };
    assert_eq!(status, ERR_RANGE);
    assert_eq!(len, 2);
    assert_eq!(small, [0xAA; 2]);
}
