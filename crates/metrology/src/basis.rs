//! Which position of a chain a stamp's [`Basis`] names, and why the links after
//! it contribute nothing to that stamp.
//!
//! # Two vocabularies describing one thing
//!
//! `extendedresearch-clock` says **where a reading was taken**: [`Basis`], as a
//! stamp carries it. This crate says **what a stage of a chain does**:
//! [`LinkKind`], as a chain describes it. They are the same fact from two
//! directions, and until they are joined a stamp's basis is a label nothing
//! reads. This module is the join, written as a table a test walks rather than
//! as a sentence in a design document.
//!
//! # The rule the join exists to enforce
//!
//! R14. **A stamp's basis names the chain position at which the number was
//! taken, and the links after that position contribute nothing to that stamp's
//! uncertainty.** A kernel socket timestamp is not a smaller scheduling term: it
//! is a stamp taken at [`LinkKind::HostReceive`], and scheduling happens after
//! it, so scheduling is not in the number at all. The record shows the shorter
//! chain rather than a smaller number, and only the first survives a reviewer
//! asking how it was arrived at.
//!
//! [`Composer::later_stamped_at`](crate::Composer::later_stamped_at) and
//! [`Composer::earlier_stamped_at`](crate::Composer::earlier_stamped_at) are
//! where the rule is applied; this module is where the position comes from.
//!
//! # The failure this module exists to prevent
//!
//! Counting a stage twice on the strength of two vocabularies that never met. A
//! recorder that takes a kernel socket timestamp, and also carries a term for
//! the scheduling delay because the chain lists it, reports a bound wider than
//! the truth by the whole scheduling term — and a recorder that takes an
//! application-level stamp and drops the scheduling term because it believes the
//! basis removed it reports one narrower than the truth by the same amount. The
//! second is the dangerous direction, and neither is visible in the number.
//!
//! # The join is not one-to-one, and says so
//!
//! Several bases name one position: [`Basis::AfterReadReturned`],
//! [`Basis::InPlatformCallback`] and [`Basis::BrowserEventHandler`] are three
//! ways of reaching [`LinkKind::HostStamp`], and the difference between them is
//! *how much* of the queueing is inside the stamp, not *where* the stamp is.
//!
//! One basis names two positions. [`Basis::ClockRead`] says the number came from
//! a direct read of this domain's clock, which is a statement about the method
//! and not about the path: in an acquisition chain that read is
//! [`LinkKind::HostStamp`], and in a stimulus chain it is
//! [`LinkKind::ApplicationSubmit`]. So [`link_kinds`] answers a set, and
//! [`Chain::position_of`] resolves it against the chain actually described —
//! refusing when the chain has none of them, or more than one.

use std::fmt;

use extendedresearch_clock::Basis;

use crate::chain::{Chain, LinkKind, Span};
use crate::ids::ChainId;

/// The chain position a basis is taken at is the application's own clock read.
const HOST_STAMP: &[LinkKind] = &[LinkKind::HostStamp];
/// The chain position a basis is taken at is the host's receive path.
const HOST_RECEIVE: &[LinkKind] = &[LinkKind::HostReceive];
/// A direct clock read is the application's stamp going one way and its submit
/// going the other; the chain decides which.
const HOST_STAMP_OR_SUBMIT: &[LinkKind] = &[LinkKind::HostStamp, LinkKind::ApplicationSubmit];
/// The chain position a basis is taken at is the record write.
const SERIALISATION: &[LinkKind] = &[LinkKind::Serialisation];
/// The chain position a basis is taken at is the composition of a frame.
const COMPOSITOR: &[LinkKind] = &[LinkKind::Compositor];
/// The chain position a basis is taken at is the device-side output buffer.
const OUTPUT_BUFFER: &[LinkKind] = &[LinkKind::OutputBuffer];
/// The basis names no position.
const NOWHERE: &[LinkKind] = &[];

/// The link kinds a basis may name, in the order a chain is searched for them.
///
/// Empty for [`Basis::Unspecified`], which states no position, and for any
/// basis this crate has not been taught — [`Basis`] is `#[non_exhaustive]`, and
/// a value added to it arrives here as "names no position" rather than as a
/// silent guess. [`Chain::position_of`] refuses on an empty set, so the gap is
/// reported rather than composed around.
///
/// ```
/// use extendedresearch_metrology::{Basis, LinkKind, link_kinds};
/// assert_eq!(link_kinds(Basis::KernelSocketTimestamp), &[LinkKind::HostReceive]);
/// // A direct clock read names a position only once the chain says which way
/// // the chain runs.
/// assert_eq!(
///     link_kinds(Basis::ClockRead),
///     &[LinkKind::HostStamp, LinkKind::ApplicationSubmit],
/// );
/// assert!(link_kinds(Basis::Unspecified).is_empty());
/// ```
pub fn link_kinds(basis: Basis) -> &'static [LinkKind] {
    match basis {
        // Not stated, so nothing is claimed about the path.
        Basis::Unspecified => NOWHERE,
        // Three ways for the application to read the clock itself. Which of the
        // three it was decides how much queueing is inside the stamp, and that
        // is a matter for the term covering the stamp, not for its position.
        Basis::AfterReadReturned | Basis::InPlatformCallback | Basis::BrowserEventHandler => {
            HOST_STAMP
        }
        // The kernel stamped it on the way in, and the interrupt edge is the
        // earliest instant the host can name. Both are the receive path, and
        // both leave the scheduling and the application's own read after the
        // stamp.
        Basis::KernelSocketTimestamp | Basis::InterruptEdge => HOST_RECEIVE,
        // The platform handed an occurrence time over with the event, so the
        // number was taken where the platform took it: on the way in.
        Basis::PlatformEventTimestamp => HOST_RECEIVE,
        // The number was taken when a record was read back, which is the write
        // that could delay or reorder it.
        Basis::FileRead => SERIALISATION,
        // A frame time is the compositor's, so scanout and the panel's own
        // response are still ahead of it. See the module header of
        // `extendedresearch-clock`'s `bound` for why that is pinned rather than
        // left to the reader.
        Basis::DisplayFrame => COMPOSITOR,
        // A sound scheduled into an output buffer is stamped at the buffer, and
        // conversion and emission follow it.
        Basis::AudioOutputBuffer => OUTPUT_BUFFER,
        // The method, not the path: the chain says which end it is.
        Basis::ClockRead => HOST_STAMP_OR_SUBMIT,
        // `Basis` is `#[non_exhaustive]`.
        _ => NOWHERE,
    }
}

/// Why a basis could not be resolved to a position of a chain.
///
/// Every variant names the basis and, where there is one, the chain — a refusal
/// that does not say what it refused over is a refusal nobody can act on (R28).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BasisError {
    /// The basis names no chain position at all.
    ///
    /// [`Basis::Unspecified`] is the ordinary case: a stamp that does not say
    /// where it was taken cannot shorten a chain, and a composer that let it
    /// would be shortening the chain on no evidence.
    NamesNoPosition {
        /// The basis.
        basis: Basis,
    },
    /// The chain describes no link of any kind the basis names.
    ///
    /// The stamp and the chain disagree about what happened. A stimulus chain
    /// stamped [`Basis::KernelSocketTimestamp`] is the shape of it: one of the
    /// two is wrong, and guessing which would put a position nobody described
    /// into the record.
    NoSuchLink {
        /// The chain searched.
        chain: ChainId,
        /// The basis.
        basis: Basis,
    },
    /// The chain describes the basis's link more than once, so the position is
    /// ambiguous.
    ///
    /// Two stages of one chain that both read the host clock are a chain
    /// describing two stamps; which of them this one is has to be stated rather
    /// than inferred from whichever came first.
    Ambiguous {
        /// The chain searched.
        chain: ChainId,
        /// The basis.
        basis: Basis,
        /// The first matching position.
        first: u16,
        /// The second matching position.
        second: u16,
    },
}

impl fmt::Display for BasisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BasisError::NamesNoPosition { basis } => {
                write!(f, "basis {basis:?} names no chain position")
            }
            BasisError::NoSuchLink { chain, basis } => write!(
                f,
                "chain {} describes no link that basis {basis:?} names",
                chain.0
            ),
            BasisError::Ambiguous {
                chain,
                basis,
                first,
                second,
            } => write!(
                f,
                "chain {} describes the link basis {basis:?} names at both {first} and {second}",
                chain.0
            ),
        }
    }
}

impl std::error::Error for BasisError {}

impl Chain {
    /// The position of this chain the stamp's basis names (R14).
    ///
    /// The chain is searched for a link whose kind [`link_kinds`] lists for the
    /// basis. Exactly one match answers its position.
    ///
    /// # Errors
    ///
    /// [`BasisError::NamesNoPosition`] when the basis names no kind,
    /// [`BasisError::NoSuchLink`] when this chain describes none of them, and
    /// [`BasisError::Ambiguous`] when it describes more than one.
    ///
    /// ```
    /// use extendedresearch_metrology::{Basis, Chain, ChainId, Link, LinkKind};
    /// let chain = Chain::new(ChainId(1), [
    ///     Link::of(LinkKind::Transduction),
    ///     Link::of(LinkKind::Transport),
    ///     Link::of(LinkKind::HostReceive),
    ///     Link::of(LinkKind::HostQueue),
    ///     Link::of(LinkKind::HostStamp),
    /// ]);
    /// assert_eq!(chain.position_of(Basis::KernelSocketTimestamp), Ok(2));
    /// assert_eq!(chain.position_of(Basis::AfterReadReturned), Ok(4));
    /// ```
    pub fn position_of(&self, basis: Basis) -> Result<u16, BasisError> {
        let kinds = link_kinds(basis);
        if kinds.is_empty() {
            return Err(BasisError::NamesNoPosition { basis });
        }
        let mut found: Option<u16> = None;
        for (index, link) in self.links.iter().enumerate() {
            if !kinds.contains(&link.kind) {
                continue;
            }
            let Ok(position) = u16::try_from(index) else {
                // A chain longer than a position can address is the composer's
                // refusal to make; nothing past it can be named here.
                break;
            };
            match found {
                None => found = Some(position),
                Some(first) => {
                    return Err(BasisError::Ambiguous {
                        chain: self.id,
                        basis,
                        first,
                        second: position,
                    });
                }
            }
        }
        found.ok_or(BasisError::NoSuchLink {
            chain: self.id,
            basis,
        })
    }

    /// The span of this chain a stamp taken at `basis` is accountable for:
    /// position 0 through the position the basis names (R14).
    ///
    /// The links after that position are not in the number, so no term covering
    /// them belongs in the stamp's budget and no gap among them makes it
    /// unbounded. A hardware stamp does not shrink a term; it shortens the
    /// chain, and this is the shorter chain.
    ///
    /// # Errors
    ///
    /// As [`Chain::position_of`].
    ///
    /// ```
    /// use extendedresearch_metrology::{Basis, Chain, ChainId, Link, LinkKind, Span};
    /// let chain = Chain::new(ChainId(1), [
    ///     Link::of(LinkKind::Transduction),
    ///     Link::of(LinkKind::Transport),
    ///     Link::of(LinkKind::HostReceive),
    ///     Link::of(LinkKind::HostQueue),
    ///     Link::of(LinkKind::HostStamp),
    /// ]);
    /// // The kernel stamped it, so the queue and the application's read are
    /// // after the number and outside it.
    /// assert_eq!(
    ///     chain.through(Basis::KernelSocketTimestamp),
    ///     Ok(Span { chain: ChainId(1), from: 0, to: 2 }),
    /// );
    /// ```
    pub fn through(&self, basis: Basis) -> Result<Span, BasisError> {
        Ok(Span {
            chain: self.id,
            from: 0,
            to: self.position_of(basis)?,
        })
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "a test that panics is reporting, not failing to handle"
)]
mod tests {
    use super::*;
    use crate::chain::Link;

    /// Every basis this crate knows, with the kinds it names. A basis added to
    /// the clock crate without a row here fails
    /// [`every_basis_but_unspecified_names_a_position`].
    const EVERY_BASIS: &[(Basis, &[LinkKind])] = &[
        (Basis::Unspecified, NOWHERE),
        (Basis::AfterReadReturned, HOST_STAMP),
        (Basis::InPlatformCallback, HOST_STAMP),
        (Basis::KernelSocketTimestamp, HOST_RECEIVE),
        (Basis::InterruptEdge, HOST_RECEIVE),
        (Basis::BrowserEventHandler, HOST_STAMP),
        (Basis::FileRead, SERIALISATION),
        (Basis::DisplayFrame, COMPOSITOR),
        (Basis::AudioOutputBuffer, OUTPUT_BUFFER),
        (Basis::ClockRead, HOST_STAMP_OR_SUBMIT),
        (Basis::PlatformEventTimestamp, HOST_RECEIVE),
    ];

    fn acquisition() -> Chain {
        Chain::new(
            ChainId(1),
            [
                Link::of(LinkKind::Transduction),
                Link::of(LinkKind::Transport),
                Link::of(LinkKind::HostReceive),
                Link::of(LinkKind::HostQueue),
                Link::of(LinkKind::HostStamp),
            ],
        )
    }

    fn stimulus() -> Chain {
        Chain::new(
            ChainId(2),
            [
                Link::of(LinkKind::ApplicationSubmit),
                Link::of(LinkKind::Compositor),
                Link::of(LinkKind::Conversion),
                Link::of(LinkKind::Emission),
            ],
        )
    }

    #[test]
    fn the_table_is_the_one_documented() {
        for (basis, kinds) in EVERY_BASIS {
            assert_eq!(link_kinds(*basis), *kinds, "{basis:?}");
        }
    }

    #[test]
    fn every_basis_but_unspecified_names_a_position() {
        for (basis, kinds) in EVERY_BASIS {
            if *basis == Basis::Unspecified {
                assert!(kinds.is_empty());
            } else {
                assert!(!kinds.is_empty(), "{basis:?} names no link kind");
            }
        }
    }

    #[test]
    fn a_basis_resolves_against_the_chain_that_was_described() {
        let chain = acquisition();
        assert_eq!(chain.position_of(Basis::InterruptEdge), Ok(2));
        assert_eq!(chain.position_of(Basis::KernelSocketTimestamp), Ok(2));
        assert_eq!(chain.position_of(Basis::PlatformEventTimestamp), Ok(2));
        assert_eq!(chain.position_of(Basis::AfterReadReturned), Ok(4));
        assert_eq!(chain.position_of(Basis::InPlatformCallback), Ok(4));
        assert_eq!(chain.position_of(Basis::BrowserEventHandler), Ok(4));
        assert_eq!(chain.position_of(Basis::ClockRead), Ok(4));
    }

    #[test]
    fn a_direct_clock_read_is_the_submit_in_a_stimulus_chain() {
        // The one basis that names a method rather than a position: the chain
        // decides which end of it the read sits at.
        assert_eq!(stimulus().position_of(Basis::ClockRead), Ok(0));
        assert_eq!(acquisition().position_of(Basis::ClockRead), Ok(4));
    }

    #[test]
    fn a_stimulus_basis_names_a_stimulus_position() {
        assert_eq!(stimulus().position_of(Basis::DisplayFrame), Ok(1));
        let audio = Chain::new(
            ChainId(3),
            [
                Link::of(LinkKind::ApplicationSubmit),
                Link::of(LinkKind::FrameworkQueue),
                Link::of(LinkKind::OutputBuffer),
                Link::of(LinkKind::Conversion),
                Link::of(LinkKind::Emission),
            ],
        );
        assert_eq!(audio.position_of(Basis::AudioOutputBuffer), Ok(2));
        assert_eq!(
            audio.through(Basis::AudioOutputBuffer),
            Ok(Span {
                chain: ChainId(3),
                from: 0,
                to: 2
            })
        );
    }

    #[test]
    fn a_read_back_record_is_stamped_at_the_write() {
        let recorded = Chain::new(
            ChainId(4),
            [
                Link::of(LinkKind::Transduction),
                Link::of(LinkKind::HostStamp),
                Link::of(LinkKind::Serialisation),
            ],
        );
        assert_eq!(recorded.position_of(Basis::FileRead), Ok(2));
    }

    #[test]
    fn unspecified_shortens_no_chain() {
        assert_eq!(
            acquisition().position_of(Basis::Unspecified),
            Err(BasisError::NamesNoPosition {
                basis: Basis::Unspecified
            })
        );
    }

    #[test]
    fn a_basis_the_chain_does_not_describe_is_refused() {
        assert_eq!(
            stimulus().position_of(Basis::KernelSocketTimestamp),
            Err(BasisError::NoSuchLink {
                chain: ChainId(2),
                basis: Basis::KernelSocketTimestamp
            })
        );
    }

    #[test]
    fn two_matching_links_are_ambiguous_rather_than_the_first() {
        let twice = Chain::new(
            ChainId(5),
            [
                Link::of(LinkKind::HostStamp),
                Link::of(LinkKind::Serialisation),
                Link::of(LinkKind::HostStamp),
            ],
        );
        assert_eq!(
            twice.position_of(Basis::AfterReadReturned),
            Err(BasisError::Ambiguous {
                chain: ChainId(5),
                basis: Basis::AfterReadReturned,
                first: 0,
                second: 2
            })
        );
        // And the two kinds a direct read may name count together.
        let both = Chain::new(
            ChainId(6),
            [
                Link::of(LinkKind::ApplicationSubmit),
                Link::of(LinkKind::HostStamp),
            ],
        );
        assert!(matches!(
            both.position_of(Basis::ClockRead),
            Err(BasisError::Ambiguous { .. })
        ));
    }

    #[test]
    fn every_refusal_says_what_it_refused_over() {
        let message = stimulus()
            .position_of(Basis::InterruptEdge)
            .expect_err("refuses")
            .to_string();
        assert!(message.contains("chain 2"), "{message}");
        assert!(message.contains("InterruptEdge"), "{message}");
    }
}
