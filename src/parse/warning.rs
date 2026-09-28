//! What a load did with the parts of a file it could not read as written.

use std::fmt;

/// One thing a load dropped, or read against a rule of the OpenDRIVE spec,
/// in [`Provenance::warnings`](crate::Provenance::warnings).
///
/// Each variant names where in the file it happened, and `Display` gives the
/// message. New kinds are not a breaking change. Elements the crate does not
/// read at all, such as `<userData>` on a road, raise no warning.
#[derive(Debug, Clone, PartialEq)]
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
    /// A `<lane>` with no `<width>` or `<border>` the crate could read. It
    /// has no extent, so it did not bake, and neither did its road marks. A
    /// lane outside it stacks on the lane inside it.
    LaneDropped {
        /// The `<road id>` it is on.
        road_id: String,
        /// The zero-based lane-section index, as in
        /// [`LaneProvenance::section`](crate::LaneProvenance::section).
        section: usize,
        /// Its `<lane id>`.
        lane: i32,
    },
    /// A `<lane>` with `<border>`s, in a lane section that also has
    /// `<width>`s, on it or on another lane. The spec makes the two
    /// exclusive, and uses the widths where a section has both, so read
    /// literally a lane with only borders there has no extent. The crate
    /// reads each lane on its own: its widths if it has any, and its borders
    /// if not.
    WidthAndBorder {
        /// The `<road id>` it is on.
        road_id: String,
        /// The zero-based lane-section index.
        section: usize,
        /// Its `<lane id>`.
        lane: i32,
    },
    /// A `<lane>` that takes its extent from `<border>`s, on a road whose
    /// `<laneOffset>` is not 0 in its lane section. The spec forbids the two
    /// together. The crate measures the border from the reference line and
    /// ignores the offset for it.
    BorderWithLaneOffset {
        /// The `<road id>` it is on.
        road_id: String,
        /// The zero-based lane-section index.
        section: usize,
        /// Its `<lane id>`.
        lane: i32,
    },
    /// A `<lane>` whose `<border>` lies inside its inner neighbour's outer
    /// border, which the spec forbids. The crate gives the lane 0 width
    /// there. Checked at the lane's stations.
    BorderCrossesInnerLane {
        /// The `<road id>` it is on.
        road_id: String,
        /// The zero-based lane-section index.
        section: usize,
        /// Its `<lane id>`.
        lane: i32,
        /// The first station where it crosses, in metres along the
        /// reference line.
        s: f64,
    },
    /// A `<lateralProfile>` whose `<shape>`s start inside the road's right
    /// edge. The spec says each profile covers the whole road. The crate
    /// holds the first shape's value from its `t` out to the edge.
    ShapeShortOfRoad {
        /// The `<road id>` it is on.
        road_id: String,
        /// The profile's `s`.
        s: f64,
    },
    /// A `<road>` whose `rule` is neither `RHT` nor `LHT`, the two values
    /// the spec allows. The crate reads it as `RHT`, the spec's default.
    UnknownTrafficRule {
        /// Its `<road id>`.
        road_id: String,
        /// The value of its `rule`.
        rule: String,
    },
    /// A junction `<connection>` without an `incomingRoad`, or without the
    /// road it leads into: a `connectingRoad`, or a `linkedRoad` in a
    /// `type="direct"` junction. The spec requires both. The crate drops the
    /// connection, so no lane links across it.
    ConnectionDropped {
        /// Its `incomingRoad`, empty if it has none.
        incoming_road_id: String,
        /// The `<junction id>` it is in.
        junction_id: String,
        /// Its `<connection id>`, empty if it has none.
        connection_id: String,
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

impl Warning {
    /// The `<road id>` it happened on.
    pub fn road_id(&self) -> &str {
        match self {
            Self::RoadSkipped { road_id, .. }
            | Self::LaneDropped { road_id, .. }
            | Self::WidthAndBorder { road_id, .. }
            | Self::BorderWithLaneOffset { road_id, .. }
            | Self::BorderCrossesInnerLane { road_id, .. }
            | Self::ShapeShortOfRoad { road_id, .. }
            | Self::UnknownTrafficRule { road_id, .. }
            | Self::ConnectionDropped {
                incoming_road_id: road_id,
                ..
            } => road_id,
        }
    }
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
                "road {road_id:?}, lane section {section}: lane {lane} dropped, it has no usable <width> or <border>"
            ),
            Self::WidthAndBorder {
                road_id,
                section,
                lane,
            } => write!(
                f,
                "road {road_id:?}, lane section {section}: lane {lane} has <border>s in a section with <width>s"
            ),
            Self::BorderWithLaneOffset {
                road_id,
                section,
                lane,
            } => write!(
                f,
                "road {road_id:?}, lane section {section}: lane {lane} has <border>s under a <laneOffset>, which they ignore"
            ),
            Self::BorderCrossesInnerLane {
                road_id,
                section,
                lane,
                s,
            } => write!(
                f,
                "road {road_id:?}, lane section {section}: lane {lane}'s <border> crosses inside the lane within it at s {s:.2} m"
            ),
            Self::ShapeShortOfRoad { road_id, s } => write!(
                f,
                "road {road_id:?}: the <shape>s at s {s:.2} m start inside the road's right edge"
            ),
            Self::UnknownTrafficRule { road_id, rule } => write!(
                f,
                "road {road_id:?}: rule {rule:?} is neither \"RHT\" nor \"LHT\", read as \"RHT\""
            ),
            Self::ConnectionDropped {
                incoming_road_id,
                junction_id,
                connection_id,
            } => write!(
                f,
                "junction {junction_id:?}: connection {connection_id:?} from road {incoming_road_id:?} dropped, it lacks the road it comes from or leads into"
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
