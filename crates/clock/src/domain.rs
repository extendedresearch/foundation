//! Which clock a reading was taken on, and the string formats that name it.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::anchor::Anchor;

/// Whether a domain's clock advances while the machine is suspended.
///
/// The integers are this crate's definition and are part of its contract, as
/// for [`Basis`](crate::Basis). [`SuspendBehaviour::Unspecified`] is not [`SuspendBehaviour::Included`], and
/// nothing may infer the second from the first.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SuspendBehaviour {
    /// Not established for this clock.
    Unspecified = 0,
    /// The clock advances across a suspend.
    Included = 1,
    /// The clock stops during a suspend.
    Excluded = 2,
}

/// An in-process handle for a domain's `(host_id, host_clock_epoch)` pair.
///
/// It is a hash of the pair, and only for comparing readings inside one
/// process. It never crosses a library boundary and never appears in a record
/// or a conformance vector, so the hash algorithm is not part of any contract;
/// what crosses a boundary is the two strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DomainId(u64);

/// A clock domain: which machine or realm, which run of its clock, and what is
/// known about that clock.
///
/// Two readings may be subtracted if and only if `host_id` and
/// `host_clock_epoch` are both byte-equal. Only those two strings identify a
/// domain; the other fields describe its clock.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Domain {
    /// Which machine (native) or which JS realm (browser).
    pub host_id: String,
    /// Which run of the clock, plus `.e<n>` after the n-th discontinuity.
    pub host_clock_epoch: String,
    /// The platform clock, as a bare identifier compared by exact match.
    pub monotonic_source: &'static str,
    /// Whether the clock advances across a suspend.
    pub suspend: SuspendBehaviour,
    /// The clock's resolution. `None` means not known.
    pub resolution_ns: Option<u64>,
    /// Where the domain's timeline meets the calendar, when that is known.
    pub anchor: Option<Anchor>,
}

impl Domain {
    /// The handle readings on this domain carry: a hash of `host_id` and
    /// `host_clock_epoch`, and of nothing else.
    pub fn id(&self) -> DomainId {
        let mut hasher = DefaultHasher::new();
        self.host_id.hash(&mut hasher);
        self.host_clock_epoch.hash(&mut hasher);
        DomainId(hasher.finish())
    }
}

/// The native epoch: `boot.` and the lowercase, unpadded hex of
/// `wall_ns.saturating_sub(monotonic_ns) / 1_000_000_000`.
///
/// The inputs are one wall read followed by one monotonic read, taken once per
/// process, and not an anchor's midpoint. The derivation is not airtight by
/// design: a two-nanosecond wall step across a whole-second boundary names a
/// different epoch.
///
/// ```
/// use extendedresearch_clock::host_clock_epoch_from;
/// assert_eq!(host_clock_epoch_from(1_700_000_000_000_000_000, 60_000_000_000), "boot.6553f0c4");
/// assert_eq!(host_clock_epoch_from(5, 500), "boot.0");
/// ```
pub fn host_clock_epoch_from(wall_ns: u64, monotonic_ns: u64) -> String {
    format!(
        "boot.{:x}",
        wall_ns.saturating_sub(monotonic_ns) / 1_000_000_000
    )
}

/// The browser epoch and `host_id`: `doc.` and 16 lowercase, zero-padded hex
/// digits of `(nonce_hi << 32) | nonce_lo`.
///
/// The nonce is drawn once per JS realm, outside Rust, and arrives as two
/// halves. The function name is provisional.
///
/// ```
/// use extendedresearch_clock::doc_epoch_from;
/// assert_eq!(doc_epoch_from(0, 0xab), "doc.00000000000000ab");
/// ```
pub fn doc_epoch_from(nonce_hi: u32, nonce_lo: u32) -> String {
    format!(
        "doc.{:016x}",
        (u64::from(nonce_hi) << 32) | u64::from(nonce_lo)
    )
}

/// The epoch after one more discontinuity: the suffix replaced by `.e<n+1>`.
///
/// The suffix is everything from the second `.`, since neither base format
/// (`boot.<hex>`, `doc.<hex>`) contains a `.` after its prefix. A suffix of the
/// form `.e<n>` gives `.e<n+1>`; no suffix, or one of any other form, gives
/// `.e1`. `None` when `n + 1` does not fit `u64`.
pub(crate) fn next_epoch(epoch: &str) -> Option<String> {
    let base_end = epoch
        .char_indices()
        .filter(|&(_, c)| c == '.')
        .nth(1)
        .map_or(epoch.len(), |(i, _)| i);
    let (base, suffix) = epoch.split_at(base_end);
    let n = match suffix.strip_prefix(".e") {
        Some(digits) if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) => {
            digits.parse::<u64>().ok()?
        }
        _ => 0,
    };
    Some(format!("{base}.e{}", n.checked_add(1)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn domain(host_id: &str, epoch: &str) -> Domain {
        Domain {
            host_id: host_id.to_owned(),
            host_clock_epoch: epoch.to_owned(),
            monotonic_source: "CLOCK_BOOTTIME",
            suspend: SuspendBehaviour::Included,
            resolution_ns: Some(1),
            anchor: None,
        }
    }

    #[test]
    fn the_native_epoch() {
        let mono = 60_000_000_000;
        assert_eq!(
            host_clock_epoch_from(1_700_000_000_000_000_000, mono),
            "boot.6553f0c4"
        );
        assert_eq!(
            host_clock_epoch_from(1_700_000_000_400_000_000, mono),
            "boot.6553f0c4"
        );
        assert_eq!(
            host_clock_epoch_from(1_700_003_600_000_000_000, mono),
            "boot.6553fed4"
        );
        assert_eq!(
            host_clock_epoch_from(1_700_000_000_999_999_999, mono),
            "boot.6553f0c4"
        );
        assert_eq!(
            host_clock_epoch_from(1_700_000_001_000_000_001, mono),
            "boot.6553f0c5"
        );
        assert_eq!(host_clock_epoch_from(5, 500), "boot.0");
        assert_eq!(host_clock_epoch_from(0, 0), "boot.0");
        assert_eq!(host_clock_epoch_from(u64::MAX, 0), "boot.44b82fa09");
    }

    #[test]
    fn the_browser_epoch() {
        assert_eq!(doc_epoch_from(0, 0), "doc.0000000000000000");
        assert_eq!(doc_epoch_from(0, 0xab), "doc.00000000000000ab");
        assert_eq!(
            doc_epoch_from(0xffff_ffff, 0xffff_ffff),
            "doc.ffffffffffffffff"
        );
        assert_eq!(doc_epoch_from(1, 0), "doc.0000000100000000");
    }

    #[test]
    fn the_suffix_is_replaced_never_appended() {
        assert_eq!(
            next_epoch("boot.6553f0c4").as_deref(),
            Some("boot.6553f0c4.e1")
        );
        assert_eq!(
            next_epoch("boot.6553f0c4.e1").as_deref(),
            Some("boot.6553f0c4.e2")
        );
        assert_eq!(
            next_epoch("boot.6553f0c4.e9").as_deref(),
            Some("boot.6553f0c4.e10")
        );
        assert_eq!(
            next_epoch("doc.00000000000000ab").as_deref(),
            Some("doc.00000000000000ab.e1")
        );
        // A suffix that is not `.e<n>` is still the suffix, and is replaced.
        assert_eq!(next_epoch("boot.1.x").as_deref(), Some("boot.1.e1"));
        assert_eq!(next_epoch("boot.1.e").as_deref(), Some("boot.1.e1"));
        assert_eq!(next_epoch("boot.1.e1.e2").as_deref(), Some("boot.1.e1"));
        assert_eq!(next_epoch("plain").as_deref(), Some("plain.e1"));
        assert_eq!(
            next_epoch("boot.1.e18446744073709551614").as_deref(),
            Some("boot.1.e18446744073709551615")
        );
        assert_eq!(next_epoch("boot.1.e18446744073709551615"), None);
        assert_eq!(next_epoch("boot.1.e99999999999999999999"), None);
    }

    #[test]
    fn identity_is_the_two_strings() {
        let a = domain("fixture-host", "boot.6553f0c4");
        let mut b = a.clone();
        b.monotonic_source = "QueryPerformanceCounter";
        b.suspend = SuspendBehaviour::Excluded;
        b.resolution_ns = None;
        assert_eq!(a.id(), b.id());
        assert_ne!(a.id(), domain("fixture-host", "boot.6553F0C4").id());
        assert_ne!(a.id(), domain("fixture-host ", "boot.6553f0c4").id());
        assert_ne!(a.id(), domain("other-host", "boot.6553f0c4").id());
        // The two fields are not concatenated before hashing.
        assert_ne!(domain("ab", "c").id(), domain("a", "bc").id());
    }

    #[test]
    fn suspend_integers() {
        assert_eq!(SuspendBehaviour::Unspecified as u8, 0);
        assert_eq!(SuspendBehaviour::Included as u8, 1);
        assert_eq!(SuspendBehaviour::Excluded as u8, 2);
    }
}
