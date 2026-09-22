//! The path an event travelled to become a number, and the closed range of it
//! a term covers.
//!
//! # Why the path is explicit
//!
//! An interval's uncertainty is a property of the **path** each event took, not
//! of the instruments at either end. Writing the path down is what lets terms
//! compose without double-counting, lets two calibrations that describe the
//! same stage be detected rather than both applied, and tells a researcher
//! which stage is worth money.
//!
//! It also decides what a hardware timestamp buys. A hardware stamp does not
//! shrink a term: it **shortens the chain**, and the record shows the shorter
//! chain rather than a smaller number (R14). The two are the same total and
//! very different claims, and only the first survives someone asking how it was
//! arrived at.
//!
//! # Coverage is a span, never a kind
//!
//! R16. A kind tag alone cannot say which of two devices a "device delay"
//! describes, and a trigger alignment covers a contiguous run of stages rather
//! than one. So a term names [`Span`]: one chain, and a closed range of
//! positions within it.
//!
//! # The failure this module exists to prevent
//!
//! A budget that accounts for only the stages someone remembered to describe.
//! It composes cleanly, it produces a plausible number, and the number is
//! missing however much the undescribed stages contribute. Writing the chain
//! down is what lets the composer find a position no term covers and answer
//! "unbounded" instead (R11).

use crate::ids::{ChainId, DeviceId};

/// One stage of a chain, and which device it happens in.
///
/// A link with no device is one that belongs to no single box — a transport, or
/// a mapping between two timelines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Link {
    /// What happens at this stage.
    pub kind: LinkKind,
    /// The device the stage happens in, where one owns it.
    pub device: Option<DeviceId>,
}

impl Link {
    /// A link of this kind, belonging to no particular device.
    pub const fn of(kind: LinkKind) -> Link {
        Link { kind, device: None }
    }

    /// A link of this kind, inside this device.
    pub const fn in_device(kind: LinkKind, device: DeviceId) -> Link {
        Link {
            kind,
            device: Some(device),
        }
    }
}

/// What happens at one stage of a chain.
///
/// # The value set is derived, not enumerated from any instrument
///
/// A measurement chain runs in one of two directions: a physical event becomes
/// a number (acquisition, 1 to 10, in order), or a command becomes a physical
/// event (stimulus, 11 to 16, in order). Two stages apply to either (17 and
/// 18). A chain uses the links it has and omits the rest — a device with no
/// on-board filter simply has no [`LinkKind::DeviceFilter`] link.
///
/// This is not a list of what any consumer happens to have. A chain needing a
/// stage the set lacks is a reviewed addition, and the integers append.
///
/// # The integers are a wire contract
///
/// R13. A link kind is written to a record as its integer, so a value is never
/// renumbered and never reused. The set is closed at each release. Until the
/// first record schema version is frozen — step 7 of the implementation order —
/// these values are this crate's proposal; from that point they are fixed.
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum LinkKind {
    // ---- acquisition: a physical event becomes a number ----
    /// The physical quantity becomes a signal.
    Transduction = 1,
    /// Amplification and analogue filtering.
    AnalogConditioning = 2,
    /// Sample-and-hold, ADC aperture.
    Quantisation = 3,
    /// On-device digital filtering; group delay.
    DeviceFilter = 4,
    /// On-device accumulation before transmission.
    DeviceBuffer = 5,
    /// The instrument applies its own timestamp.
    DeviceStamp = 6,
    /// Wire, radio, or bus.
    Transport = 7,
    /// Interrupt, driver, kernel.
    HostReceive = 8,
    /// Scheduling and userspace wakeup.
    HostQueue = 9,
    /// The application reads a clock.
    HostStamp = 10,

    // ---- stimulus: a command becomes a physical event ----
    /// The program issues the command.
    ApplicationSubmit = 11,
    /// Graphics or audio API queueing.
    FrameworkQueue = 12,
    /// OS composition or mixing.
    Compositor = 13,
    /// Device-side buffer before conversion.
    OutputBuffer = 14,
    /// DAC, scanout.
    Conversion = 15,
    /// Panel response, transducer rise.
    Emission = 16,

    // ---- either ----
    /// An estimated mapping onto another timeline.
    ClockMapping = 17,
    /// A record write that can delay or reorder a stamp.
    Serialisation = 18,
}

impl LinkKind {
    /// The integer this kind is written to a record as.
    ///
    /// The projection a record encoder and the C ABI both take, named rather
    /// than left to a cast at each call site.
    pub const fn kind(self) -> u16 {
        self as u16
    }
}

/// The ordered stages one event traversed, from the physical event to the
/// recorded number.
///
/// R12. Position 0 is the physical event, and positions increase towards the
/// recorded number. The sequence is finite, and a chain that describes no
/// stages describes nothing: the composer refuses one rather than reporting
/// that every stage it was not told about is covered.
///
/// This lives once per stream, not once per stamp. A 1 kHz stream recording for
/// an hour produces 3.6 million stamps and one of these.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Chain {
    /// The identifier a stamp carries to name this chain.
    pub id: ChainId,
    /// The stages, in order, position 0 first.
    pub links: Vec<Link>,
}

impl Chain {
    /// A chain of these links, in order.
    pub fn new(id: ChainId, links: impl Into<Vec<Link>>) -> Chain {
        Chain {
            id,
            links: links.into(),
        }
    }

    /// The last position, or `None` when the chain describes no stages.
    pub fn last_position(&self) -> Option<u16> {
        u16::try_from(self.links.len().checked_sub(1)?).ok()
    }

    /// The span covering every position of this chain, or `None` when it
    /// describes no stages.
    pub fn whole(&self) -> Option<Span> {
        Some(Span {
            chain: self.id,
            from: 0,
            to: self.last_position()?,
        })
    }
}

/// A closed range of link positions within one named chain.
///
/// Closed at both ends: `from == to` is one position, not an empty range. A
/// span with `to` below `from` describes nothing, and the composer refuses one
/// rather than reading it as empty — an empty span would silently cover no
/// link while looking like coverage, which is the R11 failure wearing a
/// different hat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    /// Which chain the positions are in.
    pub chain: ChainId,
    /// The first position covered.
    pub from: u16,
    /// The last position covered, inclusive.
    pub to: u16,
}

impl Span {
    /// The span covering one position.
    pub const fn at(chain: ChainId, position: u16) -> Span {
        Span {
            chain,
            from: position,
            to: position,
        }
    }

    /// True when `from` is at or below `to`, so the span covers at least one
    /// position.
    pub const fn is_well_formed(self) -> bool {
        self.from <= self.to
    }

    /// How many positions the span covers, or `0` when it is not well formed.
    pub const fn len(self) -> u32 {
        if self.is_well_formed() {
            (self.to as u32) - (self.from as u32) + 1
        } else {
            0
        }
    }

    /// True when the span covers no position, which only a malformed span does.
    pub const fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// True when `position` is in this span.
    pub const fn contains(self, position: u16) -> bool {
        self.is_well_formed() && self.from <= position && position <= self.to
    }

    /// The positions both spans cover, or `None` when they are on different
    /// chains or do not meet.
    ///
    /// Two spans on different chains never intersect, however their positions
    /// compare: a position is only meaningful inside the chain that names it.
    pub const fn intersection(self, other: Span) -> Option<Span> {
        if self.chain.0 != other.chain.0 || !self.is_well_formed() || !other.is_well_formed() {
            return None;
        }
        let from = if self.from > other.from {
            self.from
        } else {
            other.from
        };
        let to = if self.to < other.to {
            self.to
        } else {
            other.to
        };
        if from <= to {
            Some(Span {
                chain: self.chain,
                from,
                to,
            })
        } else {
            None
        }
    }

    /// True when the two spans share at least one position of one chain.
    pub const fn intersects(self, other: Span) -> bool {
        self.intersection(other).is_some()
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

    #[test]
    fn the_link_kind_integers_are_the_derived_set() {
        assert_eq!(LinkKind::Transduction.kind(), 1);
        assert_eq!(LinkKind::HostStamp.kind(), 10);
        assert_eq!(LinkKind::ApplicationSubmit.kind(), 11);
        assert_eq!(LinkKind::Emission.kind(), 16);
        assert_eq!(LinkKind::ClockMapping.kind(), 17);
        assert_eq!(LinkKind::Serialisation.kind(), 18);
    }

    #[test]
    fn a_chain_reports_its_last_position() {
        let empty = Chain::new(ChainId(1), []);
        assert_eq!(empty.last_position(), None);
        assert_eq!(empty.whole(), None);

        let three = Chain::new(
            ChainId(1),
            [
                Link::of(LinkKind::Transduction),
                Link::of(LinkKind::Transport),
                Link::of(LinkKind::HostStamp),
            ],
        );
        assert_eq!(three.last_position(), Some(2));
        assert_eq!(
            three.whole(),
            Some(Span {
                chain: ChainId(1),
                from: 0,
                to: 2
            })
        );
    }

    #[test]
    fn a_span_is_closed_at_both_ends() {
        let one = Span::at(ChainId(1), 4);
        assert!(one.contains(4));
        assert!(!one.contains(3));
        assert!(!one.contains(5));
        assert_eq!(one.len(), 1);
        assert!(!one.is_empty());
    }

    #[test]
    fn a_malformed_span_covers_nothing() {
        let bad = Span {
            chain: ChainId(1),
            from: 5,
            to: 4,
        };
        assert!(!bad.is_well_formed());
        assert!(bad.is_empty());
        assert!(!bad.contains(4));
        assert!(!bad.contains(5));
        assert_eq!(bad.intersection(bad), None);
    }

    #[test]
    fn spans_on_different_chains_never_intersect() {
        let a = Span {
            chain: ChainId(1),
            from: 0,
            to: 9,
        };
        let b = Span {
            chain: ChainId(2),
            from: 0,
            to: 9,
        };
        assert!(!a.intersects(b));
    }

    #[test]
    fn an_intersection_is_the_positions_both_cover() {
        let a = Span {
            chain: ChainId(1),
            from: 2,
            to: 6,
        };
        let b = Span {
            chain: ChainId(1),
            from: 5,
            to: 9,
        };
        assert_eq!(
            a.intersection(b),
            Some(Span {
                chain: ChainId(1),
                from: 5,
                to: 6
            })
        );
        assert_eq!(a.intersection(b), b.intersection(a));

        let touching = Span {
            chain: ChainId(1),
            from: 7,
            to: 7,
        };
        assert_eq!(a.intersection(touching), None);
        assert_eq!(
            a.intersection(Span::at(ChainId(1), 6)),
            Some(Span::at(ChainId(1), 6))
        );
    }
}
