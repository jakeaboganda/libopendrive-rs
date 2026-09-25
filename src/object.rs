//! Roadside objects: the poles, buildings, barriers and trees placed along a
//! road. Format-agnostic, like the lanes: an importer resolves each object to
//! a world pose, and nothing here knows where it came from.

use crate::coords::Point;

/// `[u, v, z]` turned by yaw about Z, then pitch about the turned Y, then
/// roll about the turned X: an offset in an object's own frame, in the
/// network's axes.
pub(crate) fn orient(yaw: f64, pitch: f64, roll: f64, [u, v, z]: [f64; 3]) -> [f64; 3] {
    let (sr, cr) = roll.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let (sy, cy) = yaw.sin_cos();
    let (v, z) = (v * cr - z * sr, v * sr + z * cr);
    let (u, z) = (u * cp + z * sp, -u * sp + z * cp);
    let (u, v) = (u * cy - v * sy, u * sy + v * cy);
    [u, v, z]
}

/// An opaque object identifier. Not a position in
/// [`RoadNetwork::objects`](crate::RoadNetwork::objects), so look objects up
/// with [`RoadNetwork::object`](crate::RoadNetwork::object).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ObjectId(pub usize);

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

/// The volume a placed object occupies, in its own frame: `length` along its
/// heading, `width` across it, `height` up from its origin.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Extent {
    /// A box centred on the origin in plan, rising from it. A dimension the
    /// map does not give is 0, so a post given only a height is a box with no
    /// footprint, and a parking bay given no height is a flat one.
    Box {
        /// Along the heading (metres).
        length: f32,
        /// Across the heading (metres).
        width: f32,
        /// Up from the origin (metres).
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

/// One point of an outline or a sweep: where it meets the ground and where
/// its top edge is, both in the network's frame.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Corner {
    /// The bottom of the corner.
    pub base: Point,
    /// The top of the corner. The same as `base` where the height is 0.
    pub top: Point,
}

/// One cross-section of a [`Shape::Sweep`]: its two edges at one station.
/// `left` is the edge toward +t, left of the road's reference line heading.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Section {
    /// The +t edge.
    pub left: Corner,
    /// The -t edge. The same as `left` for a sweep with no width, such as a
    /// fence.
    pub right: Corner,
}

/// Where an object is and what volume it fills.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Shape {
    /// A solid placed at a pose. The orientation is three angles in
    /// radians, applied as yaw, then pitch, then roll.
    Solid {
        /// The object's origin, the base of its extent.
        position: Point,
        /// Yaw about +Z, counter-clockwise from +X.
        heading: f32,
        /// Pitch against the ground plane.
        pitch: f32,
        /// Roll against the ground plane.
        roll: f32,
        /// The volume it occupies, or `None` for an object the map gives no
        /// size.
        extent: Option<Extent>,
    },
    /// A footprint polygon, walled from each corner's base to its top. Already
    /// in the network's frame, so there is no pose to apply.
    Outline {
        /// The polygon, in order. At least two.
        corners: Vec<Corner>,
        /// Whether the last corner joins the first. A closed outline encloses
        /// an area, such as a building. An open one is a line of wall, such as
        /// a fence.
        closed: bool,
    },
    /// A rectangular cross-section swept along a road, such as a guard rail
    /// or a wall. Already in the network's frame. Consecutive sections are
    /// joined by straight walls.
    Sweep {
        /// The cross-sections in order along the road. At least two.
        sections: Vec<Section>,
    },
}

/// One object in the world.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Object {
    /// This object's identity.
    pub id: ObjectId,
    /// What the object is.
    pub kind: ObjectType,
    /// The map's finer category within `kind`, such as a kind of barrier.
    /// Free text, and empty if the map gives none.
    pub subtype: String,
    /// The name the map gives it, often a model file. Empty if it has none.
    pub name: String,
    /// Whether the object can move, such as a gate or a barrier arm. Its
    /// shape is where it stands in the map.
    pub dynamic: bool,
    /// Where it is and what it fills.
    pub shape: Shape,
}
