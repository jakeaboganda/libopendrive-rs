//! Roadside objects: the poles, buildings, barriers and trees placed along a
//! road. Format-agnostic, like the lanes: an importer resolves each object to
//! a world pose, and nothing here knows where it came from.

use crate::coords::Point;

/// What an object is.
///
/// The set is the static object vocabulary maps are written in. Anything
/// else, a vendor's own name or a moving participant that older formats
/// allowed here, is [`ObjectType::Unknown`], and the object still bakes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ObjectType {
    /// An object with no category assigned.
    None,
    /// Something in the way, such as a rock or a bollard.
    Obstacle,
    /// A post: a sign post, a guide post, a bollard.
    Pole,
    /// A single tree.
    Tree,
    /// Plants other than a single tree, such as a hedge.
    Vegetation,
    /// A crash barrier or guard rail.
    Barrier,
    /// A building.
    Building,
    /// A marked parking bay.
    ParkingSpace,
    /// A patch of different surface, such as a pothole repair.
    Patch,
    /// A railing.
    Railing,
    /// A raised island separating traffic.
    TrafficIsland,
    /// A pedestrian crossing.
    Crosswalk,
    /// A street light.
    StreetLamp,
    /// A gantry spanning the road.
    Gantry,
    /// A noise barrier.
    SoundBarrier,
    /// Paint on the road surface that is not a lane marking.
    RoadMark,
    /// A type this crate does not recognise.
    Unknown,
}

impl ObjectType {
    /// A stable lowercase name, for a legend, a log line, or a viewer readout.
    /// Format-neutral, so it is not necessarily the token any particular map
    /// format spells the type with.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Obstacle => "obstacle",
            Self::Pole => "pole",
            Self::Tree => "tree",
            Self::Vegetation => "vegetation",
            Self::Barrier => "barrier",
            Self::Building => "building",
            Self::ParkingSpace => "parking-space",
            Self::Patch => "patch",
            Self::Railing => "railing",
            Self::TrafficIsland => "traffic-island",
            Self::Crosswalk => "crosswalk",
            Self::StreetLamp => "street-lamp",
            Self::Gantry => "gantry",
            Self::SoundBarrier => "sound-barrier",
            Self::RoadMark => "road-mark",
            Self::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for ObjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The volume an object occupies, in its own frame: `length` along its
/// heading, `width` across it, `height` up from its origin.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Extent {
    /// A box centred on the origin in plan, rising from it.
    Box {
        /// Along the heading (metres).
        length: f32,
        /// Across the heading (metres).
        width: f32,
        /// Up from the origin (metres). Zero for a flat footprint such as a
        /// parking bay.
        height: f32,
    },
    /// An upright cylinder centred on the origin, rising from it.
    Cylinder {
        /// Metres.
        radius: f32,
        /// Up from the origin (metres).
        height: f32,
    },
}

/// One object placed in the world.
///
/// The orientation is three angles in radians, applied as yaw, then pitch,
/// then roll.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Object {
    /// What the object is.
    pub kind: ObjectType,
    /// The name the map gives it, often a model file. Empty if it has none.
    pub name: String,
    /// The object's origin, the base of its extent, in the network's frame.
    pub position: Point,
    /// Yaw about +Z, counter-clockwise from +X.
    pub heading: f32,
    /// Pitch against the ground plane.
    pub pitch: f32,
    /// Roll against the ground plane.
    pub roll: f32,
    /// The volume it occupies, or `None` for an object the map gives no size.
    pub extent: Option<Extent>,
}
