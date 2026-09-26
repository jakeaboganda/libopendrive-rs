//! The baked road-network model: lanes, their connectivity graph, and the
//! queries over both. Format-agnostic: nothing here knows OpenDRIVE.

use std::ops::Range;

use crate::coords::Point;
use crate::geometry::{Polyline, Projection, RoadSample};
use crate::grid::{Aabb, Grid};
use crate::object::{Object, ObjectId};
use crate::structure::{Coverage, Structure, StructureId};

/// An opaque lane identifier. **Not** a vector index into `RoadNetwork.lanes`.
/// An importer may assign arbitrary ids, such as OpenDRIVE lane keys, so look
/// lanes up with [`RoadNetwork::lane`], never by position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LaneId(pub usize);

/// What a lane is for.
///
/// The set mirrors the lane functions a road cross-section distinguishes,
/// because that is the vocabulary the maps are written in, and collapsing it
/// would throw away the only thing that tells a sidewalk from a bus lane.
///
/// Two questions get asked of a lane type often enough to answer here rather
/// than at every call site: whether through traffic belongs on it
/// ([`LaneType::is_drivable`]) and what to call it ([`LaneType::as_str`]).
/// Everything else is the consumer's policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LaneType {
    /// Surface inside the road with no function assigned to it: the paved area
    /// a cross-section has to account for but does not name. Not a gap. It has
    /// a width and a surface like any other lane, and a map that leaves it out
    /// has holes in it.
    None,
    /// An ordinary traffic lane.
    Driving,
    /// One lane carrying traffic both ways, such as a centre turn lane.
    Bidirectional,
    /// Reserved for buses.
    Bus,
    /// Reserved for taxis.
    Taxi,
    /// Reserved for high-occupancy vehicles.
    Hov,
    /// An acceleration lane joining a road.
    Entry,
    /// A deceleration lane leaving a road.
    Exit,
    /// A ramp onto a motorway.
    OnRamp,
    /// A ramp off a motorway.
    OffRamp,
    /// A ramp linking two motorways.
    ConnectingRamp,
    /// A short bypass lane at an intersection, usually for turning traffic.
    SlipLane,
    /// A lane vehicles park in.
    Parking,
    /// A hard shoulder for emergency stops.
    Stop,
    /// A lane traffic may not use, such as a painted gore area.
    Restricted,
    /// A cycle lane.
    Biking,
    /// A footway.
    Sidewalk,
    /// Hard shoulder, outboard of the running lanes.
    Shoulder,
    /// The paved strip between a running lane and whatever is beside it.
    Border,
    /// The kerb between the carriageway and the footway.
    Curb,
    /// The strip separating opposing carriageways.
    Median,
    /// A lane closed for works.
    RoadWorks,
    /// Tram track sharing the road surface.
    Tram,
    /// Railway track.
    Rail,
    /// Vendor-defined surface. The format reserves three of these without
    /// saying what they are for.
    Special1,
    /// Vendor-defined surface. See [`LaneType::Special1`].
    Special2,
    /// Vendor-defined surface. See [`LaneType::Special1`].
    Special3,
    /// A surface whose declared type this crate does not recognise.
    ///
    /// A lane has geometry whether or not its type means anything here, so it
    /// bakes as one of these rather than being dropped. Dropping it left holes
    /// in the road surface instead, which is harder to notice than a lane of
    /// the wrong colour.
    Unknown,
}

impl LaneType {
    /// Whether ordinary through traffic belongs on this lane.
    ///
    /// This is the routing predicate. It decides which lanes a vehicle may
    /// change into, so it is deliberately narrower than "has a paved surface":
    /// a parking lane and a hard shoulder are drivable in the everyday sense
    /// and are excluded, because a route that ran through them would be wrong.
    pub fn is_drivable(&self) -> bool {
        matches!(
            self,
            Self::Driving
                | Self::Bidirectional
                | Self::Bus
                | Self::Taxi
                | Self::Hov
                | Self::Entry
                | Self::Exit
                | Self::OnRamp
                | Self::OffRamp
                | Self::ConnectingRamp
                | Self::SlipLane
        )
    }

    /// A stable lowercase name, for a legend, a log line, or a viewer readout.
    /// Format-neutral, so it is not necessarily the token any particular map
    /// format spells the type with.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Driving => "driving",
            Self::Bidirectional => "bidirectional",
            Self::Bus => "bus",
            Self::Taxi => "taxi",
            Self::Hov => "hov",
            Self::Entry => "entry",
            Self::Exit => "exit",
            Self::OnRamp => "on-ramp",
            Self::OffRamp => "off-ramp",
            Self::ConnectingRamp => "connecting-ramp",
            Self::SlipLane => "slip-lane",
            Self::Parking => "parking",
            Self::Stop => "stop",
            Self::Restricted => "restricted",
            Self::Biking => "biking",
            Self::Sidewalk => "sidewalk",
            Self::Shoulder => "shoulder",
            Self::Border => "border",
            Self::Curb => "curb",
            Self::Median => "median",
            Self::RoadWorks => "road-works",
            Self::Tram => "tram",
            Self::Rail => "rail",
            Self::Special1 => "special1",
            Self::Special2 => "special2",
            Self::Special3 => "special3",
            Self::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for LaneType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Travel direction of a lane relative to its geometry's start→end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Direction {
    /// Travel runs along the centerline, start to end.
    Forward,
    /// Travel runs against it, end to start.
    Backward,
}

/// One lane: a strip of road surface described by its centerline and width.
/// Not necessarily drivable; a sidewalk and a median are lanes too, and
/// [`Lane::kind`] is what separates them.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Lane {
    /// This lane's identity. Not a position in any list.
    pub id: LaneId,
    /// What the lane is for.
    pub kind: LaneType,
    /// Which way traffic runs along `center`.
    pub direction: Direction,
    /// Lane centerline.
    pub center: Polyline,
    /// Nominal lane width (metres), meaning the widest this lane gets.
    ///
    /// The whole width of a lane that holds one, and the full-section width of
    /// a lane that tapers. This names the lane, a 3.5 m lane. It is not the
    /// width at a given station; [`Lane::width_at`] answers that, and the
    /// tessellator uses it.
    pub width: f32,
    /// Per-centerline-vertex width (metres), parallel to `center.points()`.
    ///
    /// Empty means a lane of constant `width`, which covers most of them. Any
    /// non-empty profile must have exactly `center.points().len()` entries.
    /// A gore area at an off-ramp is what this exists for. It is 0 m wide
    /// where it begins and 5 m wide further along, and a single width put it
    /// at neither.
    pub widths: Vec<f32>,
    /// Per-centerline-vertex superelevation angle (radians, signed), parallel to
    /// `center.points()`. Positive raises the **+offset** edge, the left-hand
    /// normal of the centerline's *stored* tangent (its geometry direction), which
    /// for a `Backward` lane is opposite its travel direction. Consumers deriving
    /// a surface normal must roll about `center.tangents()`, not travel, or a
    /// backward lane's normal disagrees with its own (correct) baked heights.
    /// Empty means a flat lane (bank ≡ 0); any non-empty profile must have exactly
    /// `center.points().len()` entries. The centerline points already carry the
    /// banked *height* (reference-line pivot); this angle is the surface tilt
    /// that the mesh cant and [`Lane::sample_at`] read.
    pub bank: Vec<f32>,
    /// Lanes reachable by driving off this lane's exit (travel-direction) end.
    /// May fan out (a junction) or be empty (a dead end / unlinked lane). Built
    /// by an importer from road/lane links and junctions; empty otherwise.
    pub successors: Vec<LaneId>,
    /// Lanes that drive into this lane, the reverse of `successors`.
    pub predecessors: Vec<LaneId>,
    /// Adjacent same-section, same-direction lanes you can change into (lateral
    /// lane-change edges). Empty if there's no neighbor to change to.
    pub neighbors: Vec<LaneId>,
}

/// The "compiled map": everything a consumer needs, baked and format-agnostic.
///
/// Immutable once built. The lane list is private so the network can derive
/// lookup structures from it in [`RoadNetwork::new`] without any way for them
/// to go stale. A map that changed under its own index would answer
/// `nearest_lane` with a lane that is no longer there.
///
/// Serializes as its lanes, objects and structures; the index is rebuilt on the way back
/// in, so a network that crossed a process boundary is indistinguishable from
/// one that was just imported.
#[derive(Debug, Clone, Default)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(into = "NetworkData", from = "NetworkData")
)]
pub struct RoadNetwork {
    lanes: Vec<Lane>,
    objects: Vec<Object>,
    structures: Vec<Structure>,
    /// Lanes of kind [`LaneType::Driving`] bucketed by their XY footprint, for
    /// [`Self::nearest_lane`]. Derived from `lanes`, so it takes no part in
    /// equality.
    index: LaneIndex,
}

/// Two networks are equal when their lanes, objects and structures are; the
/// index is a function of the lanes.
impl PartialEq for RoadNetwork {
    fn eq(&self, other: &Self) -> bool {
        self.lanes == other.lanes
            && self.objects == other.objects
            && self.structures == other.structures
    }
}

/// What a [`RoadNetwork`] serializes as: its content without the index.
#[cfg(feature = "serde")]
#[derive(serde::Serialize, serde::Deserialize)]
struct NetworkData {
    lanes: Vec<Lane>,
    objects: Vec<Object>,
    structures: Vec<Structure>,
}

#[cfg(feature = "serde")]
impl From<NetworkData> for RoadNetwork {
    fn from(data: NetworkData) -> Self {
        Self::new(data.lanes)
            .with_objects(data.objects)
            .with_structures(data.structures)
    }
}

#[cfg(feature = "serde")]
impl From<RoadNetwork> for NetworkData {
    fn from(net: RoadNetwork) -> Self {
        Self {
            lanes: net.lanes,
            objects: net.objects,
            structures: net.structures,
        }
    }
}

impl From<Vec<Lane>> for RoadNetwork {
    fn from(lanes: Vec<Lane>) -> Self {
        Self::new(lanes)
    }
}

/// Segments of a lane's centerline per indexed span. A query projects only
/// onto the spans the grid offers it, so its cost tracks how finely the road
/// near it is sampled, not how long the lanes there are.
const SPAN_SEGMENTS: usize = 4;

/// The driving lanes' centerlines, cut into spans of [`SPAN_SEGMENTS`]
/// segments, with each span's XY footprint and a grid over them. Spans are in
/// lane order, then along the lane, so the grid's lowest-index tie break
/// picks what projecting onto each lane whole in turn would.
///
/// Only [`LaneType::Driving`] is indexed, not everything
/// [`LaneType::is_drivable`] admits. Snapping a body to the road must land it
/// on an ordinary traffic lane, so a bus lane or a slip lane beside it never
/// wins on distance alone.
#[derive(Debug, Clone, Default)]
struct LaneIndex {
    bounds: Vec<Aabb>,
    /// Each span's lane, as a position in `lanes`, and its segments.
    spans: Vec<(usize, Range<usize>)>,
    grid: Grid,
}

impl LaneIndex {
    fn build(lanes: &[Lane]) -> Self {
        let (mut bounds, mut spans) = (Vec::new(), Vec::new());
        for (i, lane) in lanes.iter().enumerate() {
            if lane.kind != LaneType::Driving {
                continue;
            }
            let points = lane.center.points();
            let segments = points.len() - 1;
            for first in (0..segments).step_by(SPAN_SEGMENTS) {
                let span = first..(first + SPAN_SEGMENTS).min(segments);
                // A span has at least one segment, so two points: Some.
                if let Some(b) =
                    Aabb::around(points[span.start..=span.end].iter().map(|p| (p.x, p.y)))
                {
                    bounds.push(b);
                    spans.push((i, span));
                }
            }
        }
        let grid = Grid::build(&bounds);
        Self {
            bounds,
            spans,
            grid,
        }
    }
}

impl Lane {
    /// Superelevation angle (radians, signed) at arc length `s`, interpolated
    /// between vertices; zero everywhere on a flat lane. Positive raises the
    /// left edge. See [`Lane::bank`].
    pub fn bank_at(&self, s: f32) -> f32 {
        if self.bank.is_empty() {
            return 0.0;
        }
        debug_assert_eq!(
            self.bank.len(),
            self.center.points().len(),
            "a non-empty bank profile must be parallel to the centerline"
        );
        let (i, t) = self.center.locate(s);
        self.bank[i] + (self.bank[i + 1] - self.bank[i]) * t
    }

    /// Lane width (metres) at arc length `s`, interpolated between vertices;
    /// the constant [`Lane::width`] on a lane with no profile. See
    /// [`Lane::widths`].
    pub fn width_at(&self, s: f32) -> f32 {
        if self.widths.is_empty() {
            return self.width;
        }
        debug_assert_eq!(
            self.widths.len(),
            self.center.points().len(),
            "a non-empty width profile must be parallel to the centerline"
        );
        let (i, t) = self.center.locate(s);
        self.widths[i] + (self.widths[i + 1] - self.widths[i]) * t
    }

    /// The road surface at arc length `s` along this lane: the banked centerline
    /// point, its stored-tangent heading, the bank angle, and the surface
    /// up-normal. What draping a body onto the (possibly canted) lane needs.
    pub fn sample_at(&self, s: f32) -> RoadSample {
        let pose = self.center.pose_at(s);
        RoadSample::new(pose.position, pose.heading, self.bank_at(s))
    }
}

impl RoadNetwork {
    /// Bake a lane list into a network, indexing the [`LaneType::Driving`]
    /// lanes by their ground footprint. Linear in the total number of centerline points.
    pub fn new(lanes: Vec<Lane>) -> Self {
        let index = LaneIndex::build(&lanes);
        Self {
            lanes,
            objects: Vec::new(),
            structures: Vec::new(),
            index,
        }
    }

    /// This network with `objects` placed on it, replacing any it had.
    pub fn with_objects(mut self, objects: Vec<Object>) -> Self {
        self.objects = objects;
        self
    }

    /// This network with `structures` over its lanes, replacing any it had.
    pub fn with_structures(mut self, structures: Vec<Structure>) -> Self {
        self.structures = structures;
        self
    }

    /// Every tunnel and bridge, in the order the importer emitted them.
    pub fn structures(&self) -> &[Structure] {
        &self.structures
    }

    /// The structure with this id, by identity (not position), the same way
    /// as [`Self::lane`].
    pub fn structure(&self, id: StructureId) -> Option<&Structure> {
        match self.structures.get(id.0) {
            Some(structure) if structure.id == id => Some(structure),
            _ => self.structures.iter().find(|s| s.id == id),
        }
    }

    /// Every structure over `lane`, with the part of the lane it covers. Use
    /// it to find whether a lane runs through a tunnel or over a bridge, and
    /// where.
    pub fn structures_over(&self, lane: LaneId) -> impl Iterator<Item = (&Structure, &Coverage)> {
        self.structures.iter().flat_map(move |s| {
            s.lanes
                .iter()
                .filter(move |c| c.lane == lane)
                .map(move |c| (s, c))
        })
    }

    /// Every lane, in the order the importer emitted them. Positions are not
    /// ids. Look a specific lane up with [`RoadNetwork::lane`].
    pub fn lanes(&self) -> &[Lane] {
        &self.lanes
    }

    /// Every object placed along the roads, in the order the importer emitted
    /// them.
    pub fn objects(&self) -> &[Object] {
        &self.objects
    }

    /// The object with this id, by identity (not position), the same way as
    /// [`Self::lane`].
    pub fn object(&self, id: ObjectId) -> Option<&Object> {
        match self.objects.get(id.0) {
            Some(object) if object.id == id => Some(object),
            _ => self.objects.iter().find(|o| o.id == id),
        }
    }

    /// The lane with this id, by identity (not position), so ids stay valid
    /// however an importer assigns them.
    ///
    /// Importers hand out ids sequentially, so the id is almost always its own
    /// index, so try that first and verify, falling back to a scan when it is
    /// not. The fallback keeps the by-identity contract. The fast path keeps
    /// the router off an O(lanes) probe per Dijkstra pop.
    pub fn lane(&self, id: LaneId) -> Option<&Lane> {
        match self.lanes.get(id.0) {
            Some(lane) if lane.id == id => Some(lane),
            _ => self.lanes.iter().find(|l| l.id == id),
        }
    }

    /// Every lane of kind [`LaneType::Driving`]. Narrower than
    /// [`LaneType::is_drivable`], and narrower still than [`Self::lanes`],
    /// which also yields sidewalks, medians, and the rest of the
    /// cross-section.
    pub fn driving_lanes(&self) -> impl Iterator<Item = &Lane> {
        self.lanes.iter().filter(|l| l.kind == LaneType::Driving)
    }

    /// The lowest point of any lane centerline (Z-up, metres), so how far down
    /// the road legitimately reaches. `None` if the network has no lanes. Used
    /// to set an off-map fall floor relative to the terrain, so a map that dips
    /// well below zero (a valley, an underpass) isn't mistaken for freefall.
    pub fn min_elevation(&self) -> Option<f32> {
        self.lanes
            .iter()
            .flat_map(|l| l.center.points())
            .map(|p| p.z)
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// The lanes reachable by driving off `id`'s exit end (its `successors`).
    pub fn successors(&self, id: LaneId) -> impl Iterator<Item = &Lane> {
        self.lane(id)
            .into_iter()
            .flat_map(|l| l.successors.iter())
            .filter_map(|s| self.lane(*s))
    }

    /// The driving lane whose centerline is nearest `point`, with the
    /// projection onto it. That is the lane a body is in, and its lane-keeping
    /// error.
    ///
    /// Answered through the ground-plane index built in [`RoadNetwork::new`],
    /// so the cost tracks the local lane density rather than the size of the
    /// map.
    ///
    /// Nearest is by full 3D distance, but the index prunes in XY only. That
    /// is sound, because a horizontal distance is never more than the 3D one,
    /// so pruning on it can only keep candidates, never drop a winner. It is
    /// also what keeps both of two stacked roads, a bridge over a road,
    /// candidates for a point between them.
    pub fn nearest_lane(&self, point: Point) -> Option<(LaneId, Projection)> {
        let index = &self.index;
        index.grid.nearest(point.x, point.y, |item, best| {
            let i = item as usize;
            // The footprint is a lower bound on the distance to the
            // centerline, so a span whose box already loses needs no
            // projection. That is most of them, and all the repeats of a
            // span that crosses several cells.
            if index.bounds[i].dist2(point.x, point.y) > best {
                return None;
            }
            let (lane, segments) = &index.spans[i];
            let lane = &self.lanes[*lane];
            let projection = lane.center.project_segments(point, segments.clone());
            Some((
                (point - projection.point).length_squared(),
                (lane.id, projection),
            ))
        })
    }

    /// The road surface nearest `point`: project onto the nearest driving lane,
    /// then sample it. The entry point for draping a body onto the road.
    /// `None` if there are no driving lanes. NB: on a banked
    /// multi-lane road, adjacent lanes differ in height, so this can step
    /// vertically as the nearest lane flips at a lane boundary.
    pub fn sample_near(&self, point: Point) -> Option<RoadSample> {
        let (id, proj) = self.nearest_lane(point)?;
        self.lane(id).map(|lane| lane.sample_at(proj.s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coords::Vector;

    fn lane(id: usize, points: &[[f32; 3]]) -> Lane {
        Lane {
            id: LaneId(id),
            kind: LaneType::Driving,
            direction: Direction::Forward,
            center: Polyline::new(points.iter().map(|p| Point::from_array(*p)).collect()),
            width: 3.5,
            widths: Vec::new(),
            bank: Vec::new(),
            successors: Vec::new(),
            predecessors: Vec::new(),
            neighbors: Vec::new(),
        }
    }

    #[test]
    fn nearest_lane_picks_the_closer_centerline() {
        let net = RoadNetwork::new(vec![
            lane(0, &[[0.0, 2.0, 0.0], [10.0, 2.0, 0.0]]),
            lane(1, &[[0.0, -2.0, 0.0], [10.0, -2.0, 0.0]]),
        ]);
        let (id, proj) = net.nearest_lane(Point::new(5.0, 1.5, 0.0)).expect("a lane");
        assert_eq!(id, LaneId(0));
        assert!((proj.point - Point::new(5.0, 2.0, 0.0)).length() < 1e-4);
    }

    #[test]
    fn nearest_lane_is_none_when_empty() {
        assert!(RoadNetwork::default().nearest_lane(Point::ORIGIN).is_none());
    }

    #[test]
    fn sample_at_reads_bank_and_tilts_the_up_normal() {
        let mut banked = lane(0, &[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]]);
        banked.bank = vec![0.2, 0.2];
        let s = banked.sample_at(5.0);
        assert!((s.bank - 0.2).abs() < 1e-5, "bank {}", s.bank);
        assert!(
            s.heading.abs_diff_eq(Vector::X, 1e-4),
            "heading {:?}",
            s.heading
        );
        // The up-normal leans off vertical but still points up.
        assert!(s.up.z < 1.0 && s.up.z > 0.9, "up {:?}", s.up);
        assert!((s.up - Vector::Z).length() > 0.05, "up should tilt");

        // A flat lane samples bank 0 and a vertical up-normal.
        let flat = lane(1, &[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]]);
        let fs = flat.sample_at(5.0);
        assert_eq!(fs.bank, 0.0);
        assert!(fs.up.abs_diff_eq(Vector::Z, 1e-5), "flat up {:?}", fs.up);
    }

    #[test]
    fn sample_near_samples_the_nearest_lane() {
        let mut banked = lane(0, &[[0.0, 2.0, 0.0], [10.0, 2.0, 0.0]]);
        banked.bank = vec![0.1, 0.1];
        let flat = lane(1, &[[0.0, -2.0, 0.0], [10.0, -2.0, 0.0]]);
        let net = RoadNetwork::new(vec![banked, flat]);
        // Nearer the banked lane -> its bank.
        let a = net
            .sample_near(Point::new(5.0, 1.8, 0.0))
            .expect("a sample");
        assert!((a.bank - 0.1).abs() < 1e-5, "bank {}", a.bank);
        // Nearer the flat lane -> bank 0.
        let b = net
            .sample_near(Point::new(5.0, -1.8, 0.0))
            .expect("a sample");
        assert_eq!(b.bank, 0.0);
    }

    #[test]
    fn sample_near_is_none_when_empty() {
        assert!(RoadNetwork::default().sample_near(Point::ORIGIN).is_none());
    }

    #[test]
    fn sample_at_up_follows_stored_tangent_not_travel() {
        // A Backward lane and a Forward lane with the SAME centerline and bank
        // must sample identically: sample_at rolls about the stored tangent, so
        // travel direction never enters and `up` agrees with the baked heights.
        let pts = &[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]];
        let mut fwd = lane(0, pts);
        fwd.bank = vec![0.2, 0.2];
        let mut bwd = lane(1, pts);
        bwd.direction = Direction::Backward;
        bwd.bank = vec![0.2, 0.2];

        let f = fwd.sample_at(5.0);
        let b = bwd.sample_at(5.0);
        assert_eq!(f.up, b.up, "up must not depend on travel direction");
        assert_eq!(f.bank, b.bank);
        assert!(f.heading.abs_diff_eq(b.heading, 1e-6));
        // Raised edge is the +offset (left of the stored tangent): for heading
        // +X, left is +Y, so the up-normal leans toward -Y.
        assert!(
            b.up.y < -0.05,
            "up should lean away from the raised +Y edge: {:?}",
            b.up
        );
    }

    // --- curved + banked sample_at -------------------------------------------

    // On a lane that TURNS and is banked, `heading` tracks the curve, `bank`
    // interpolates between vertices, and `up` stays unit, tilted, and orthogonal
    // to the heading at every station.
    #[test]
    fn sample_at_on_a_curved_banked_lane() {
        // Two segments: +X for 10 m, then turning toward +Y. Bank ramps 0 -> 0.2.
        let mut l = lane(0, &[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [20.0, 10.0, 0.0]]);
        l.bank = vec![0.0, 0.1, 0.2];
        let seg1 = 10.0_f32;
        let seg2 = (100.0_f32 + 100.0).sqrt(); // sqrt(200)

        // Heading at the very start is the first segment direction (+X).
        assert!(
            l.sample_at(0.0).heading.abs_diff_eq(Vector::X, 1e-5),
            "start heading {:?}",
            l.sample_at(0.0).heading
        );
        // Heading at the very end is the last segment direction (+X+Y / sqrt2).
        let end_dir = Vector::new(1.0, 1.0, 0.0).normalize_or_zero();
        assert!(
            l.sample_at(seg1 + seg2).heading.abs_diff_eq(end_dir, 1e-4),
            "end heading {:?} want {end_dir:?}",
            l.sample_at(seg1 + seg2).heading
        );

        // Bank reads the profile: 0.1 at the interior vertex, 0.2 at the end,
        // and the midpoint of the ramped second segment is halfway (0.15).
        assert!((l.sample_at(seg1).bank - 0.1).abs() < 1e-5);
        assert!((l.sample_at(seg1 + seg2).bank - 0.2).abs() < 1e-5);
        assert!(
            (l.sample_at(seg1 + seg2 * 0.5).bank - 0.15).abs() < 1e-5,
            "mid-seg2 bank {}",
            l.sample_at(seg1 + seg2 * 0.5).bank
        );

        // At every station up is unit, orthogonal to heading, and (where banked)
        // tilted off vertical.
        for s in [0.0, 3.0, seg1, seg1 + 4.0, seg1 + seg2] {
            let rs = l.sample_at(s);
            assert!((rs.up.length() - 1.0).abs() < 1e-5, "up not unit @ {s}");
            assert!(rs.up.dot(rs.heading).abs() < 1e-6, "up.heading != 0 @ {s}");
            assert!(rs.up.z > 0.9, "up.z too low @ {s}: {}", rs.up.z);
            if rs.bank.abs() > 1e-3 {
                assert!(
                    (rs.up - Vector::Z).length() > 0.02,
                    "up should tilt where banked @ {s}: {:?}",
                    rs.up
                );
            }
        }
    }

    // Negative bank in sample_at leans the up-normal the opposite way from
    // positive bank (raised edge flips sides).
    #[test]
    fn sample_at_negative_bank_flips_the_up_normal() {
        let mut pos = lane(0, &[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]]);
        pos.bank = vec![0.25, 0.25];
        let mut neg = lane(1, &[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]]);
        neg.bank = vec![-0.25, -0.25];
        let p = pos.sample_at(5.0);
        let n = neg.sample_at(5.0);
        // Heading +X: +bank leans up toward -Y, -bank toward +Y.
        assert!(p.up.y < -0.05, "+bank up {:?}", p.up);
        assert!(n.up.y > 0.05, "-bank up {:?}", n.up);
        assert!((p.up.y + n.up.y).abs() < 1e-6, "should be mirrored in y");
        assert!((p.up.z - n.up.z).abs() < 1e-6, "same height component");
    }

    // The documented lane-boundary vertical step: two adjacent banked lanes sit
    // at different heights, and sample_near returns the *nearest* lane's height
    // and bank, stepping as the nearest lane flips across the boundary.
    #[test]
    fn sample_near_steps_at_a_banked_lane_boundary() {
        // Lane A raised (+y side), lane B lowered (-y side); each carries its own
        // bank. The reference-line pivot makes their centerlines differ in z.
        let mut a = lane(0, &[[0.0, 2.0, 0.3], [10.0, 2.0, 0.3]]);
        a.bank = vec![0.1, 0.1];
        let mut b = lane(1, &[[0.0, -2.0, -0.3], [10.0, -2.0, -0.3]]);
        b.bank = vec![-0.15, -0.15];
        let net = RoadNetwork::new(vec![a, b]);

        // Just on A's side of the midline -> A's height and bank.
        let sa = net
            .sample_near(Point::new(5.0, 0.1, 0.0))
            .expect("sample A");
        assert!((sa.bank - 0.1).abs() < 1e-5, "A bank {}", sa.bank);
        assert!((sa.point.z - 0.3).abs() < 1e-5, "A height {}", sa.point.z);
        // Just on B's side -> B's height and bank.
        let sb = net
            .sample_near(Point::new(5.0, -0.1, 0.0))
            .expect("sample B");
        assert!((sb.bank + 0.15).abs() < 1e-5, "B bank {}", sb.bank);
        assert!((sb.point.z + 0.3).abs() < 1e-5, "B height {}", sb.point.z);
        // The seam is a real vertical step, not a blend.
        assert!(
            (sa.point.z - sb.point.z).abs() > 0.5,
            "expected a vertical step across the boundary: {} vs {}",
            sa.point.z,
            sb.point.z
        );
    }

    // sample_at at a vertex reads exactly the bank stored there, and sampling the
    // same lane twice is bit-identical (deterministic, no hidden state).
    #[test]
    fn sample_at_is_consistent_and_reads_vertex_bank_exactly() {
        let mut l = lane(0, &[[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [4.0, 0.0, 6.0]]);
        l.bank = vec![0.05, 0.12, -0.08];
        // Arc length at each vertex: 0, 4, 10.
        for (s, want) in [(0.0, 0.05), (4.0, 0.12), (10.0, -0.08)] {
            assert!(
                (l.sample_at(s).bank - want).abs() < 1e-6,
                "vertex bank @ {s} = {}, want {want}",
                l.sample_at(s).bank
            );
        }
        // Two identical calls yield an identical sample.
        assert_eq!(l.sample_at(3.3), l.sample_at(3.3));
        assert_eq!(l.sample_at(7.0), l.sample_at(7.0));
    }

    #[test]
    fn min_elevation_is_the_lowest_centerline_point() {
        // A network that dips to z=-40 (a deep valley) reports -40, not 0.
        let net = RoadNetwork::new(vec![
            lane(0, &[[0.0, 0.0, 5.0], [10.0, 0.0, 2.0]]),
            lane(1, &[[0.0, 0.0, -40.0], [10.0, 0.0, -12.0]]),
        ]);
        assert_eq!(net.min_elevation(), Some(-40.0));
        assert_eq!(RoadNetwork::default().min_elevation(), None);
    }

    #[test]
    fn lane_lookup_is_by_id_not_position() {
        // Ids need not equal vec positions. An importer may assign arbitrary
        // ones. Position 0 holds id 17, position 1 holds id 4.
        let net = RoadNetwork::new(vec![
            Lane {
                id: LaneId(17),
                ..lane(0, &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]])
            },
            Lane {
                id: LaneId(4),
                ..lane(1, &[[0.0, 5.0, 0.0], [1.0, 5.0, 0.0]])
            },
        ]);
        assert_eq!(net.lane(LaneId(17)).map(|l| l.id), Some(LaneId(17)));
        assert_eq!(net.lane(LaneId(4)).map(|l| l.id), Some(LaneId(4)));
        assert!(net.lane(LaneId(0)).is_none()); // position 0, but not id 0
                                                // nearest_lane's returned id round-trips through lane().
        let (id, _) = net.nearest_lane(Point::new(0.5, 0.0, 0.0)).unwrap();
        assert!(net.lane(id).is_some());
    }

    // --- bank_at sampling ----------------------------------------------------

    // A flat lane (empty bank) reads 0 everywhere and never panics, including at
    // and past the ends and below zero.
    #[test]
    fn bank_at_of_a_flat_lane_is_zero_and_never_panics() {
        let l = lane(0, &[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [4.0, 0.0, 0.0]]);
        assert!(l.bank.is_empty());
        for s in [-10.0, -0.0, 0.0, 1.0, 2.0, 4.0, 4.0001, 1000.0] {
            assert_eq!(l.bank_at(s), 0.0, "flat bank_at({s})");
        }
    }

    // A non-empty profile interpolates linearly between vertices and clamps past
    // both ends. Centerline at x = 0, 2, 4 (two 2 m segments); bank = 0, 0.1,
    // 0.2, so bank_at grows linearly with s and flattens outside [0, 4].
    #[test]
    fn bank_at_interpolates_between_vertices_and_clamps() {
        let l = Lane {
            bank: vec![0.0, 0.1, 0.2],
            ..lane(0, &[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [4.0, 0.0, 0.0]])
        };
        // Exactly on vertices.
        assert!((l.bank_at(0.0) - 0.0).abs() < 1e-6, "{}", l.bank_at(0.0));
        assert!((l.bank_at(2.0) - 0.1).abs() < 1e-6, "{}", l.bank_at(2.0));
        assert!((l.bank_at(4.0) - 0.2).abs() < 1e-6, "{}", l.bank_at(4.0));
        // Midway through each segment -> the midpoint value.
        assert!((l.bank_at(1.0) - 0.05).abs() < 1e-6, "{}", l.bank_at(1.0));
        assert!((l.bank_at(3.0) - 0.15).abs() < 1e-6, "{}", l.bank_at(3.0));
        // Past the far end clamps to the last vertex; below zero to the first.
        assert!(
            (l.bank_at(100.0) - 0.2).abs() < 1e-6,
            "{}",
            l.bank_at(100.0)
        );
        assert!(
            (l.bank_at(-100.0) - 0.0).abs() < 1e-6,
            "{}",
            l.bank_at(-100.0)
        );
        // Exactly at length and just past it must not panic and stay clamped.
        let len = l.center.length();
        assert!((l.bank_at(len) - 0.2).abs() < 1e-6);
        assert!((l.bank_at(len + 5.0) - 0.2).abs() < 1e-6);
    }

    // The stored sign is preserved (a negative bank stays negative through
    // interpolation and clamping).
    #[test]
    fn bank_at_preserves_a_negative_profile() {
        let l = Lane {
            bank: vec![-0.2, -0.1, 0.0],
            ..lane(0, &[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [4.0, 0.0, 0.0]])
        };
        assert!((l.bank_at(0.0) + 0.2).abs() < 1e-6, "{}", l.bank_at(0.0));
        assert!((l.bank_at(1.0) + 0.15).abs() < 1e-6, "{}", l.bank_at(1.0));
        assert!((l.bank_at(-5.0) + 0.2).abs() < 1e-6, "clamp low");
    }
}
