//! What a load did with the parts of a file it could not read as written.

use std::fmt;

/// One thing a load dropped, or read against a rule of the OpenDRIVE spec,
/// in [`Provenance::warnings`](crate::Provenance::warnings).
///
/// Each variant names where in the file it happened, and `Display` gives the
/// message. New kinds are not a breaking change. Elements the crate does not
/// read at all, such as `<userData>` on a road, raise no warning.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum Warning {
    /// A `<road>` baked nothing, and everything on it went with it.
    RoadSkipped {
        /// Its `<road id>`.
        road_id: String,
        /// What it lacked.
        reason: RoadSkipReason,
    },
    /// A `<lane>` with no `<width>` the crate could read. It has no extent,
    /// so it did not bake, and neither did its road marks. A lane outside it
    /// stacks on the lane inside it.
    LaneDropped {
        /// The `<road id>` it is on.
        road_id: String,
        /// The zero-based lane-section index, as in
        /// [`LaneProvenance::section`](crate::LaneProvenance::section).
        section: usize,
        /// Its `<lane id>`.
        lane: i32,
    },
}

/// Why a `<road>` baked nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum RoadSkipReason {
    /// It has no `length`, or one that is not a finite number.
    NoLength,
    /// It has no `<planView>`.
    NoPlanView,
    /// Its `<planView>` has no `<geometry>` the crate can bake: each one is
    /// missing a coordinate, carries one that is not a finite number, or has
    /// a shape the crate does not know.
    NoGeometry,
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RoadSkipped { road_id, reason } => {
                write!(f, "road {road_id:?} skipped: {reason}")
            }
            Self::LaneDropped {
                road_id,
                section,
                lane,
            } => write!(
                f,
                "road {road_id:?}, lane section {section}: lane {lane} dropped, it has no usable <width>"
            ),
        }
    }
}

impl fmt::Display for RoadSkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoLength => "no finite length",
            Self::NoPlanView => "no <planView>",
            Self::NoGeometry => "no <geometry> it can bake",
        })
    }
}
