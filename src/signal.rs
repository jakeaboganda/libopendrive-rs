//! Signals: the traffic signs, traffic lights and road paint that tell
//! traffic what to do. Like objects, they don't depend on the file format. A
//! signal keeps the catalogue codes the map gives it and looks nothing up.

use crate::coords::Point;
use crate::{LaneId, ObjectId};

/// An opaque signal identifier. Not a position in
/// [`RoadNetwork::signals`](crate::RoadNetwork::signals), so look signals up
/// with [`RoadNetwork::signal`](crate::RoadNetwork::signal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SignalId(pub usize);

/// An opaque controller identifier. Not a position in
/// [`RoadNetwork::controllers`](crate::RoadNetwork::controllers), so look
/// controllers up with
/// [`RoadNetwork::controller`](crate::RoadNetwork::controller).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ControllerId(pub usize);

/// The unit of a signal's [`value`](Signal::value).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Unit {
    /// Metres, `m`.
    Metre,
    /// Kilometres, `km`.
    Kilometre,
    /// Feet, `ft`.
    Foot,
    /// Miles, `mile`.
    Mile,
    /// Metres per second, `m/s`.
    MetrePerSecond,
    /// Miles per hour, `mph`.
    MilePerHour,
    /// Kilometres per hour, `km/h`.
    KilometrePerHour,
    /// Kilograms, `kg`.
    Kilogram,
    /// Metric tons, `t`.
    Tonne,
    /// A percentage, such as a grade, `%`.
    Percent,
    /// A unit this crate does not recognise.
    Unknown,
}

impl Unit {
    /// The unit's usual symbol, such as `km/h`, or `unknown`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Metre => "m",
            Self::Kilometre => "km",
            Self::Foot => "ft",
            Self::Mile => "mile",
            Self::MetrePerSecond => "m/s",
            Self::MilePerHour => "mph",
            Self::KilometrePerHour => "km/h",
            Self::Kilogram => "kg",
            Self::Tonne => "t",
            Self::Percent => "%",
            Self::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for Unit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One signal: what it means, the lanes it applies to, and the board it is.
///
/// Where a signal applies and where it stands can differ. A sign on a gantry
/// over one road can apply to another. `lanes` and `applies_at` say where it
/// applies, and the pose says where the board is.
///
/// The board is a box in its own frame: `length` along its heading, `width`
/// across it, and `height` up from its origin. The origin is the middle of
/// its bottom edge.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Signal {
    /// This signal's identity.
    pub id: SignalId,
    /// The name the map gives it. Empty if it has none.
    pub name: String,
    /// Whether it can change what it shows, such as a traffic light.
    pub dynamic: bool,
    /// The country whose catalogue `kind` and `subtype` come from, such as
    /// `DE`. Free text, since maps also use names such as `OpenDRIVE` for a
    /// generic catalogue. Empty if the map gives none.
    pub country: String,
    /// The year of that country's rules. Empty if the map gives none.
    pub country_revision: String,
    /// The signal's type in the country's catalogue, such as `274` for a
    /// German speed limit. Free text.
    pub kind: String,
    /// The subtype within `kind`. Free text, and often `-1` for none.
    pub subtype: String,
    /// A number the signal shows, such as a speed limit.
    pub value: Option<f64>,
    /// The unit of `value`, if the map gives one.
    pub unit: Option<Unit>,
    /// Text the signal shows, such as a town name. Empty if it has none.
    pub text: String,
    /// Whether the signal has been struck out, such as a crossed-out sign.
    pub invalidated: bool,
    /// Whether the signal is temporary, such as a sign at road works.
    pub temporary: bool,
    /// The controllers the signal belongs to. Usually none, or one.
    pub controllers: Vec<ControllerId>,
    /// The signals this one is linked to by a dependency, such as the plate
    /// that limits a speed sign to lorries.
    pub dependencies: Vec<Dependency>,
    /// The signals and objects this one relates to, such as the stop line of
    /// a traffic light.
    pub references: Vec<Reference>,
    /// The lanes the signal applies to, on every road the map applies it
    /// to. A lane appears once.
    pub lanes: Vec<LaneId>,
    /// The points on the road surface where the signal takes effect: its
    /// own first, then one for each further place the map applies it.
    pub applies_at: Vec<Point>,
    /// The middle of the board's bottom edge.
    pub position: Point,
    /// The direction the board faces, toward the traffic it addresses: yaw
    /// about +Z, counter-clockwise from +X, in radians in `(-π, π]`.
    pub heading: f32,
    /// Pitch about the turned Y, in radians, nose down for a positive angle.
    pub pitch: f32,
    /// Roll about the turned X, in radians, left side up for a positive
    /// angle.
    pub roll: f32,
    /// The board's thickness along its heading, in metres, if the map gives
    /// it.
    pub length: Option<f32>,
    /// Metres across, if the map gives it.
    pub width: Option<f32>,
    /// Metres up from the origin, if the map gives it.
    pub height: Option<f32>,
}

/// A group of signals that always show the same state, such as the lights of
/// one approach to a junction. The map does not say what they show, or when.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Controller {
    /// This controller's identity.
    pub id: ControllerId,
    /// The name the map gives it. Empty if it has none.
    pub name: String,
    /// Its priority among the other controllers, if the map gives one.
    pub sequence: Option<u32>,
    /// The signals it controls, in the order the map gives them.
    pub signals: Vec<Control>,
}

/// One signal a [`Controller`] controls.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Control {
    /// The signal.
    pub signal: SignalId,
    /// How the controller controls it. Free text, and empty if the map gives
    /// none.
    pub kind: String,
}

/// A signal another depends on.
///
/// The map's `<dependency>` names it. OpenDRIVE 1.7 calls it the signal
/// controlled, and 1.9 the one controlling, so this keeps the link as the
/// map gives it and reads nothing into its direction.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Dependency {
    /// The signal named.
    pub signal: SignalId,
    /// What the dependency is. Free text, and empty if the map gives none.
    pub kind: String,
}

/// Something a signal relates to.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Reference {
    /// The signal or object.
    pub to: Referenced,
    /// What the relation is, such as `stopline`. Free text, and empty if the
    /// map gives none.
    pub kind: String,
}

/// What a [`Reference`] names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Referenced {
    /// A signal.
    Signal(SignalId),
    /// An object.
    Object(ObjectId),
}
