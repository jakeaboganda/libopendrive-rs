//! The baked road-network model: lanes, their connectivity graph, and the
//! queries over both. Format-agnostic -- nothing here knows OpenDRIVE.

use glam::Vec3;

use crate::geometry::{Polyline, Projection, RoadSample};

/// An opaque lane identifier. **Not** a vector index into `RoadNetwork.lanes` --
/// an importer may assign arbitrary ids (e.g. from OpenDRIVE lane keys), so
/// look lanes up with [`RoadNetwork::lane`], never by position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LaneId(pub usize);

/// What a lane is for. Only driving lanes exist today; shoulders, sidewalks,
/// etc. slot in here as the importer grows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneKind {
    Driving,
}

/// Travel direction of a lane relative to its geometry's start→end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Backward,
}

/// One lane: a drivable strip described by its centerline and width. A plan
/// is laid down `center`; a vehicle drives it.
#[derive(Debug, Clone, PartialEq)]
pub struct Lane {
    pub id: LaneId,
    pub kind: LaneKind,
    pub direction: Direction,
    /// Lane centerline, Y-up, meters.
    pub center: Polyline,
    /// Constant lane width (per-vertex widths can come later).
    pub width: f32,
    /// Per-centerline-vertex superelevation angle (radians, signed), parallel to
    /// `center.points()`. Positive raises the **+offset** edge -- the left-hand
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
    /// Lanes that drive into this lane -- the reverse of `successors`.
    pub predecessors: Vec<LaneId>,
    /// Adjacent same-section, same-direction lanes you can change into (lateral
    /// lane-change edges). Empty if there's no neighbor to change to.
    pub neighbors: Vec<LaneId>,
}

/// The "compiled map": everything a consumer needs, baked and format-agnostic.
///
/// Immutable once built. The lane list is private so the network can derive
/// lookup structures from it in [`RoadNetwork::new`] without any way for them
/// to go stale -- a map that changed under its own index would answer
/// `nearest_lane` with a lane that is no longer there.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RoadNetwork {
    lanes: Vec<Lane>,
}

impl Lane {
    /// Superelevation angle (radians, signed) at arc length `s`, interpolated
    /// between vertices; zero everywhere on a flat lane. Positive raises the
    /// left edge -- see [`Lane::bank`].
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

    /// The road surface at arc length `s` along this lane: the banked centerline
    /// point, its stored-tangent heading, the bank angle, and the surface
    /// up-normal. What draping a body onto the (possibly canted) lane needs.
    pub fn sample_at(&self, s: f32) -> RoadSample {
        let pose = self.center.pose_at(s);
        RoadSample::new(pose.position, pose.heading, self.bank_at(s))
    }
}

impl RoadNetwork {
    /// Bake a lane list into a network.
    pub fn new(lanes: Vec<Lane>) -> Self {
        Self { lanes }
    }

    /// Every lane, in the order the importer emitted them. Positions are not
    /// ids -- look a specific lane up with [`RoadNetwork::lane`].
    pub fn lanes(&self) -> &[Lane] {
        &self.lanes
    }

    /// The lane with this id, by identity (not position), so ids stay valid
    /// however an importer assigns them.
    ///
    /// Importers hand out ids sequentially, so the id is almost always its own
    /// index -- try that first and verify, falling back to a scan when it is
    /// not. The fallback keeps the by-identity contract; the fast path keeps
    /// the router off an O(lanes) probe per Dijkstra pop.
    pub fn lane(&self, id: LaneId) -> Option<&Lane> {
        match self.lanes.get(id.0) {
            Some(lane) if lane.id == id => Some(lane),
            _ => self.lanes.iter().find(|l| l.id == id),
        }
    }

    pub fn driving_lanes(&self) -> impl Iterator<Item = &Lane> {
        self.lanes.iter().filter(|l| l.kind == LaneKind::Driving)
    }

    /// The lowest point of any lane centerline (Y-up, metres) -- how far down
    /// the road legitimately reaches. `None` if the network has no lanes. Used
    /// to set an off-map fall floor relative to the terrain, so a map that dips
    /// well below zero (a valley, an underpass) isn't mistaken for freefall.
    pub fn min_elevation(&self) -> Option<f32> {
        self.lanes
            .iter()
            .flat_map(|l| l.center.points())
            .map(|p| p.y)
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
    /// projection onto it -- the lane a body is in, and its lane-keeping
    /// error.
    pub fn nearest_lane(&self, point: Vec3) -> Option<(LaneId, Projection)> {
        self.driving_lanes()
            .map(|lane| (lane.id, lane.center.project(point)))
            .min_by(|(_, a), (_, b)| {
                let da = (point - a.point).length_squared();
                let db = (point - b.point).length_squared();
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// The road surface nearest `point`: project onto the nearest driving lane,
    /// then sample it. The entry point for draping a body onto the road.
    /// `None` if there are no driving lanes. NB: on a banked
    /// multi-lane road, adjacent lanes differ in height, so this can step
    /// vertically as the nearest lane flips at a lane boundary.
    pub fn sample_near(&self, point: Vec3) -> Option<RoadSample> {
        let (id, proj) = self.nearest_lane(point)?;
        self.lane(id).map(|lane| lane.sample_at(proj.s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lane(id: usize, points: &[[f32; 3]]) -> Lane {
        Lane {
            id: LaneId(id),
            kind: LaneKind::Driving,
            direction: Direction::Forward,
            center: Polyline::new(points.iter().map(|p| Vec3::from_array(*p)).collect()),
            width: 3.5,
            bank: Vec::new(),
            successors: Vec::new(),
            predecessors: Vec::new(),
            neighbors: Vec::new(),
        }
    }

    #[test]
    fn nearest_lane_picks_the_closer_centerline() {
        let net = RoadNetwork::new(vec![
            lane(0, &[[0.0, 0.0, 2.0], [10.0, 0.0, 2.0]]),
            lane(1, &[[0.0, 0.0, -2.0], [10.0, 0.0, -2.0]]),
        ]);
        let (id, proj) = net.nearest_lane(Vec3::new(5.0, 0.0, 1.5)).expect("a lane");
        assert_eq!(id, LaneId(0));
        assert!((proj.point - Vec3::new(5.0, 0.0, 2.0)).length() < 1e-4);
    }

    #[test]
    fn nearest_lane_is_none_when_empty() {
        assert!(RoadNetwork::default().nearest_lane(Vec3::ZERO).is_none());
    }

    #[test]
    fn sample_at_reads_bank_and_tilts_the_up_normal() {
        let mut banked = lane(0, &[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]]);
        banked.bank = vec![0.2, 0.2];
        let s = banked.sample_at(5.0);
        assert!((s.bank - 0.2).abs() < 1e-5, "bank {}", s.bank);
        assert!(
            s.heading.abs_diff_eq(Vec3::X, 1e-4),
            "heading {:?}",
            s.heading
        );
        // The up-normal leans off vertical but still points up.
        assert!(s.up.y < 1.0 && s.up.y > 0.9, "up {:?}", s.up);
        assert!((s.up - Vec3::Y).length() > 0.05, "up should tilt");

        // A flat lane samples bank 0 and a vertical up-normal.
        let flat = lane(1, &[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]]);
        let fs = flat.sample_at(5.0);
        assert_eq!(fs.bank, 0.0);
        assert!(fs.up.abs_diff_eq(Vec3::Y, 1e-5), "flat up {:?}", fs.up);
    }

    #[test]
    fn sample_near_samples_the_nearest_lane() {
        let mut banked = lane(0, &[[0.0, 0.0, 2.0], [10.0, 0.0, 2.0]]);
        banked.bank = vec![0.1, 0.1];
        let flat = lane(1, &[[0.0, 0.0, -2.0], [10.0, 0.0, -2.0]]);
        let net = RoadNetwork::new(vec![banked, flat]);
        // Nearer the banked lane -> its bank.
        let a = net.sample_near(Vec3::new(5.0, 0.0, 1.8)).expect("a sample");
        assert!((a.bank - 0.1).abs() < 1e-5, "bank {}", a.bank);
        // Nearer the flat lane -> bank 0.
        let b = net
            .sample_near(Vec3::new(5.0, 0.0, -1.8))
            .expect("a sample");
        assert_eq!(b.bank, 0.0);
    }

    #[test]
    fn sample_near_is_none_when_empty() {
        assert!(RoadNetwork::default().sample_near(Vec3::ZERO).is_none());
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
        // +X, left is -Z, so the up-normal leans toward +Z.
        assert!(
            b.up.z > 0.05,
            "up should lean off the raised -Z edge: {:?}",
            b.up
        );
    }

    // --- curved + banked sample_at -------------------------------------------

    // On a lane that TURNS and is banked, `heading` tracks the curve, `bank`
    // interpolates between vertices, and `up` stays unit, tilted, and orthogonal
    // to the heading at every station.
    #[test]
    fn sample_at_on_a_curved_banked_lane() {
        // Two segments: +X for 10 m, then turning toward +Z. Bank ramps 0 -> 0.2.
        let mut l = lane(0, &[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [20.0, 0.0, 10.0]]);
        l.bank = vec![0.0, 0.1, 0.2];
        let seg1 = 10.0_f32;
        let seg2 = (100.0_f32 + 100.0).sqrt(); // sqrt(200)

        // Heading at the very start is the first segment direction (+X).
        assert!(
            l.sample_at(0.0).heading.abs_diff_eq(Vec3::X, 1e-5),
            "start heading {:?}",
            l.sample_at(0.0).heading
        );
        // Heading at the very end is the last segment direction (+X+Z / sqrt2).
        let end_dir = Vec3::new(1.0, 0.0, 1.0).normalize();
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
            assert!(rs.up.y > 0.9, "up.y too low @ {s}: {}", rs.up.y);
            if rs.bank.abs() > 1e-3 {
                assert!(
                    (rs.up - Vec3::Y).length() > 0.02,
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
        // Heading +X: +bank leans up toward +Z, -bank toward -Z.
        assert!(p.up.z > 0.05, "+bank up {:?}", p.up);
        assert!(n.up.z < -0.05, "-bank up {:?}", n.up);
        assert!((p.up.z + n.up.z).abs() < 1e-6, "should be mirrored in z");
        assert!((p.up.y - n.up.y).abs() < 1e-6, "same height component");
    }

    // The documented lane-boundary vertical step: two adjacent banked lanes sit
    // at different heights, and sample_near returns the *nearest* lane's height
    // and bank -- stepping as the nearest lane flips across the boundary.
    #[test]
    fn sample_near_steps_at_a_banked_lane_boundary() {
        // Lane A raised (+z side), lane B lowered (-z side); each carries its own
        // bank. The reference-line pivot makes their centerlines differ in y.
        let mut a = lane(0, &[[0.0, 0.3, 2.0], [10.0, 0.3, 2.0]]);
        a.bank = vec![0.1, 0.1];
        let mut b = lane(1, &[[0.0, -0.3, -2.0], [10.0, -0.3, -2.0]]);
        b.bank = vec![-0.15, -0.15];
        let net = RoadNetwork::new(vec![a, b]);

        // Just on A's side of the midline -> A's height and bank.
        let sa = net.sample_near(Vec3::new(5.0, 0.0, 0.1)).expect("sample A");
        assert!((sa.bank - 0.1).abs() < 1e-5, "A bank {}", sa.bank);
        assert!((sa.point.y - 0.3).abs() < 1e-5, "A height {}", sa.point.y);
        // Just on B's side -> B's height and bank.
        let sb = net
            .sample_near(Vec3::new(5.0, 0.0, -0.1))
            .expect("sample B");
        assert!((sb.bank + 0.15).abs() < 1e-5, "B bank {}", sb.bank);
        assert!((sb.point.y + 0.3).abs() < 1e-5, "B height {}", sb.point.y);
        // The seam is a real vertical step, not a blend.
        assert!(
            (sa.point.y - sb.point.y).abs() > 0.5,
            "expected a vertical step across the boundary: {} vs {}",
            sa.point.y,
            sb.point.y
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
        // A network that dips to y=-40 (a deep valley) reports -40, not 0.
        let net = RoadNetwork::new(vec![
            lane(0, &[[0.0, 5.0, 0.0], [10.0, 2.0, 0.0]]),
            lane(1, &[[0.0, -40.0, 0.0], [10.0, -12.0, 0.0]]),
        ]);
        assert_eq!(net.min_elevation(), Some(-40.0));
        assert_eq!(RoadNetwork::default().min_elevation(), None);
    }

    #[test]
    fn lane_lookup_is_by_id_not_position() {
        // Ids need not equal vec positions -- an importer may assign arbitrary
        // ones. Position 0 holds id 17, position 1 holds id 4.
        let net = RoadNetwork::new(vec![
            Lane {
                id: LaneId(17),
                ..lane(0, &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]])
            },
            Lane {
                id: LaneId(4),
                ..lane(1, &[[0.0, 0.0, 5.0], [1.0, 0.0, 5.0]])
            },
        ]);
        assert_eq!(net.lane(LaneId(17)).map(|l| l.id), Some(LaneId(17)));
        assert_eq!(net.lane(LaneId(4)).map(|l| l.id), Some(LaneId(4)));
        assert!(net.lane(LaneId(0)).is_none()); // position 0, but not id 0
                                                // nearest_lane's returned id round-trips through lane().
        let (id, _) = net.nearest_lane(Vec3::new(0.5, 0.0, 0.0)).unwrap();
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
    // 0.2 -- so bank_at grows linearly with s and flattens outside [0, 4].
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
