use glam::{Quat, Vec3};

/// A position plus a horizontal heading along a lane. Y is up (elevation).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Pose {
    pub position: Vec3,
    /// Unit tangent in the XZ (ground) plane -- the direction of travel.
    pub heading: Vec3,
}

/// The road surface at one station: where a body sits and how it is oriented on
/// a (possibly canted) road, plus the bank angle a vehicle model consumes. This
/// is what draping a body onto the road needs -- see [`crate::Lane::sample_at`]
/// and [`crate::RoadNetwork::sample_near`].
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RoadSample {
    /// Centerline surface point (Y-up, m) -- already at the banked height.
    pub point: Vec3,
    /// Unit tangent in the XZ plane. The centerline's *stored* (geometry)
    /// direction, which for a `Backward` lane opposes travel; it is the frame
    /// `bank`/`up` are defined in.
    pub heading: Vec3,
    /// Superelevation (rad, signed; positive raises the +offset / left edge).
    pub bank: f32,
    /// Surface up-normal: +Y rolled about `heading` by `bank`. Lateral cant
    /// only -- since `heading` is horizontal, `up.dot(heading) == 0`, so this
    /// carries no fore-aft **grade** pitch. Exact on a level road; on a graded
    /// lane it omits the small pitch component. Read the meshed vertices via
    /// [`Mesh::height_at`](crate::Mesh::height_at) if you need the grade too.
    pub up: Vec3,
}

impl RoadSample {
    /// Build a sample, deriving the up-normal by rolling +Y about the horizontal
    /// `heading` by `bank`. `heading` must be the centerline's stored tangent so
    /// the roll frame agrees with how `bank` is defined (positive raises the
    /// left-hand-normal edge).
    pub fn new(point: Vec3, heading: Vec3, bank: f32) -> Self {
        let up = (Quat::from_axis_angle(heading, bank) * Vec3::Y).normalize_or(Vec3::Y);
        Self {
            point,
            heading,
            bank,
            up,
        }
    }
}

/// The result of projecting a world point onto a polyline.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Projection {
    /// Arc length of the nearest point along the polyline.
    pub s: f32,
    /// The nearest point on the polyline itself.
    pub point: Vec3,
    /// Signed lateral distance from the polyline, in the ground plane.
    /// Positive is to the **left** of travel; negative is to the right.
    pub offset: f32,
}

/// A polyline in 3D (Y-up, meters), queried by arc length. This is the baked
/// form every curve reduces to: an importer samples clothoids/arcs into points;
/// consumers only ever see the points. At least two points.
///
/// Serializes as its points alone -- the cumulative lengths and tangents are
/// derived, so sending them would be both wasteful and a way to receive a
/// polyline whose cached state disagrees with its geometry.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(into = "Vec<Vec3>", try_from = "Vec<Vec3>")
)]
pub struct Polyline {
    points: Vec<Vec3>,
    /// Cumulative arc length at each point; `cumulative[0] == 0`.
    cumulative: Vec<f32>,
    /// Per-vertex unit horizontal tangent -- the angle bisector at interior
    /// vertices, the lone segment direction at the ends. Interpolating these
    /// gives a heading that's continuous across vertices (no per-segment step),
    /// and their normals give a consistent lateral offset for lanes/meshes.
    tangents: Vec<Vec3>,
}

impl Polyline {
    /// Build a polyline, or `None` if given fewer than two points. Importers
    /// baking **external** map data (which may be malformed) must use this and
    /// surface the error, rather than crash -- see [`Polyline::new`].
    pub fn try_new(points: Vec<Vec3>) -> Option<Self> {
        if points.len() < 2 {
            return None;
        }
        let mut cumulative = Vec::with_capacity(points.len());
        let mut acc = 0.0;
        cumulative.push(0.0);
        for pair in points.windows(2) {
            acc += (pair[1] - pair[0]).length();
            cumulative.push(acc);
        }
        let tangents = vertex_tangents(&points);
        Some(Self {
            points,
            cumulative,
            tangents,
        })
    }

    /// Build from trusted, in-code geometry. Panics on fewer than two points --
    /// that's a construction bug, not a runtime condition. Importers handling
    /// external files use [`Polyline::try_new`] instead.
    pub fn new(points: Vec<Vec3>) -> Self {
        Self::try_new(points).expect("a polyline needs at least two points")
    }

    pub fn points(&self) -> &[Vec3] {
        &self.points
    }

    /// Per-vertex horizontal unit tangents (see the field docs).
    pub(crate) fn tangents(&self) -> &[Vec3] {
        &self.tangents
    }

    pub fn length(&self) -> f32 {
        self.cumulative.last().copied().unwrap_or(0.0)
    }

    /// The segment index containing arc length `s` (clamped to a valid segment).
    fn segment(&self, s: f32) -> usize {
        let last = self.points.len() - 2;
        self.cumulative
            .partition_point(|&c| c <= s)
            .saturating_sub(1)
            .min(last)
    }

    /// Fractional position `t` within segment `i` for arc length `s`.
    fn local_t(&self, i: usize, s: f32) -> f32 {
        let seg_len = self.cumulative[i + 1] - self.cumulative[i];
        if seg_len > 1e-6 {
            (s - self.cumulative[i]) / seg_len
        } else {
            0.0
        }
    }

    /// The segment index containing arc length `s`, and the fractional position
    /// `t` in `[0, 1]` within it, both clamped to a valid segment. The one place
    /// arc length becomes a `(vertex i, vertex i+1, t)` lerp -- shared by every
    /// by-arc-length sampler (position, heading, and a lane's per-vertex bank),
    /// so they can't disagree about where `s` lands.
    pub(crate) fn locate(&self, s: f32) -> (usize, f32) {
        let s = s.clamp(0.0, self.length());
        let i = self.segment(s);
        (i, self.local_t(i, s))
    }

    /// Position at arc length `s` (clamped to `[0, length]`).
    pub fn point_at(&self, s: f32) -> Vec3 {
        let (i, t) = self.locate(s);
        self.points[i].lerp(self.points[i + 1], t)
    }

    /// Position and horizontal heading at arc length `s`. The heading
    /// interpolates the per-vertex tangents, so it's continuous across vertices
    /// (a path-tracking controller sees no per-segment step).
    pub fn pose_at(&self, s: f32) -> Pose {
        let (i, t) = self.locate(s);
        Pose {
            position: self.points[i].lerp(self.points[i + 1], t),
            heading: self.tangents[i]
                .lerp(self.tangents[i + 1], t)
                .normalize_or_zero(),
        }
    }

    /// Nearest point on the polyline to `point`, with its arc length and signed
    /// lateral offset. Nearest is by full 3D distance; the offset is measured in
    /// the ground plane against the containing segment's direction.
    pub fn project(&self, point: Vec3) -> Projection {
        let mut best = Projection {
            s: 0.0,
            point: self.points[0],
            offset: 0.0,
        };
        let mut best_dist = f32::INFINITY;
        for i in 0..self.points.len() - 1 {
            let a = self.points[i];
            let ab = self.points[i + 1] - a;
            let len2 = ab.length_squared();
            let t = if len2 > 1e-9 {
                ((point - a).dot(ab) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let closest = a + ab * t;
            let dist = (point - closest).length_squared();
            if dist < best_dist {
                best_dist = dist;
                let heading = horizontal(ab).normalize_or_zero();
                best = Projection {
                    s: self.cumulative[i] + ab.length() * t,
                    point: closest,
                    offset: horizontal(point - closest).dot(left_normal(heading)),
                };
            }
        }
        best
    }
}

impl From<Polyline> for Vec<Vec3> {
    fn from(line: Polyline) -> Self {
        line.points
    }
}

/// Why a list of points is not a polyline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a polyline needs at least two points")]
pub struct TooFewPoints;

impl TryFrom<Vec<Vec3>> for Polyline {
    type Error = TooFewPoints;

    fn try_from(points: Vec<Vec3>) -> Result<Self, Self::Error> {
        Self::try_new(points).ok_or(TooFewPoints)
    }
}

/// Drop a vector onto the XZ ground plane.
fn horizontal(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

/// Per-vertex unit horizontal tangents: the bisector of the adjacent segment
/// directions at interior vertices, the single segment direction at the ends.
fn vertex_tangents(points: &[Vec3]) -> Vec<Vec3> {
    let n = points.len();
    (0..n)
        .map(|i| {
            let incoming = if i > 0 {
                horizontal(points[i] - points[i - 1]).normalize_or_zero()
            } else {
                Vec3::ZERO
            };
            let outgoing = if i + 1 < n {
                horizontal(points[i + 1] - points[i]).normalize_or_zero()
            } else {
                Vec3::ZERO
            };
            (incoming + outgoing).normalize_or_zero()
        })
        .collect()
}

/// Unit left-hand normal of a horizontal `heading` (about +Y up). For heading
/// +X this is −Z. Zero if the heading has no horizontal extent.
pub(crate) fn left_normal(heading: Vec3) -> Vec3 {
    Vec3::Y.cross(horizontal(heading)).normalize_or_zero()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(points: &[[f32; 3]]) -> Polyline {
        Polyline::new(points.iter().map(|p| Vec3::from_array(*p)).collect())
    }

    #[test]
    fn try_new_rejects_too_few_points() {
        assert!(Polyline::try_new(vec![Vec3::ZERO]).is_none());
        assert!(Polyline::try_new(vec![]).is_none());
        assert!(Polyline::try_new(vec![Vec3::ZERO, Vec3::X]).is_some());
    }

    #[test]
    fn length_sums_the_segments() {
        let l = line(&[[0.0, 0.0, 0.0], [3.0, 0.0, 0.0], [3.0, 0.0, 4.0]]);
        assert!((l.length() - 7.0).abs() < 1e-5);
    }

    #[test]
    fn point_at_interpolates_and_clamps() {
        let l = line(&[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]]);
        assert!((l.point_at(5.0) - Vec3::new(5.0, 0.0, 0.0)).length() < 1e-5);
        // Past the end clamps to the last point.
        assert!((l.point_at(100.0) - Vec3::new(10.0, 0.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn pose_heading_exact_at_ends_and_continuous_across_a_vertex() {
        let l = line(&[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 0.0, 10.0]]);
        // Endpoints resolve to the exact segment directions.
        assert!(l.pose_at(0.0).heading.abs_diff_eq(Vec3::X, 1e-5));
        assert!(l.pose_at(l.length()).heading.abs_diff_eq(Vec3::Z, 1e-5));
        // No per-segment jump: heading just before and after the vertex agree.
        let before = l.pose_at(10.0 - 0.01).heading;
        let after = l.pose_at(10.0 + 0.01).heading;
        assert!((before - after).length() < 0.02, "{before:?} vs {after:?}");
    }

    #[test]
    fn project_gives_arc_length_and_signed_offset() {
        let l = line(&[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]]);
        // A point to the left of +X travel (toward −Z) is a positive offset.
        let left = l.project(Vec3::new(5.0, 0.0, -3.0));
        assert!((left.s - 5.0).abs() < 1e-4);
        assert!((left.offset - 3.0).abs() < 1e-4, "offset {}", left.offset);
        // A point to the right (+Z) is negative.
        let right = l.project(Vec3::new(5.0, 0.0, 3.0));
        assert!((right.offset + 3.0).abs() < 1e-4, "offset {}", right.offset);
    }

    #[test]
    fn project_arc_length_spans_multiple_segments() {
        let l = line(&[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 0.0, 10.0]]);
        // A point beside the second segment lands past the first segment's end.
        let p = l.project(Vec3::new(12.0, 0.0, 6.0));
        assert!((p.s - 16.0).abs() < 1e-4, "s {}", p.s);
    }

    #[test]
    fn left_normal_of_plus_x_is_minus_z() {
        assert!(left_normal(Vec3::X).abs_diff_eq(Vec3::new(0.0, 0.0, -1.0), 1e-5));
    }

    // --- RoadSample::new invariants ------------------------------------------

    // For any bank angle the up-normal stays unit length and orthogonal to the
    // horizontal heading (the roll axis), and at bank 0 it is *exactly* +Y.
    #[test]
    fn road_sample_up_is_unit_orthogonal_and_plumb_at_zero() {
        let headings = [
            Vec3::X,
            Vec3::Z,
            Vec3::new(1.0, 0.0, 1.0).normalize(),
            Vec3::new(-2.0, 0.0, 1.0).normalize(),
        ];
        for h in headings {
            for bank in [0.0_f32, 0.2, -0.2, 1.5, -1.5] {
                let s = RoadSample::new(Vec3::new(3.0, 1.0, -4.0), h, bank);
                // Unit length.
                assert!(
                    (s.up.length() - 1.0).abs() < 1e-5,
                    "up not unit for heading {h:?} bank {bank}: len {}",
                    s.up.length()
                );
                // Orthogonal to heading (lateral cant only; no fore-aft grade).
                assert!(
                    s.up.dot(h).abs() < 1e-6,
                    "up.heading = {} for heading {h:?} bank {bank}",
                    s.up.dot(h)
                );
                // Fields are stored verbatim.
                assert_eq!(s.point, Vec3::new(3.0, 1.0, -4.0));
                assert_eq!(s.heading, h);
                assert_eq!(s.bank, bank);
            }
            // At bank 0 the up-normal is exactly +Y (not just approximately).
            assert_eq!(
                RoadSample::new(Vec3::ZERO, h, 0.0).up,
                Vec3::Y,
                "bank 0 up must be exactly +Y for heading {h:?}"
            );
        }
    }

    // Sign: for heading +X, +bank leans the up-normal toward +Z (the left edge
    // rises), and -bank leans it toward -Z. The two are mirror images.
    #[test]
    fn road_sample_up_sign_flips_with_bank_sign() {
        let pos = RoadSample::new(Vec3::ZERO, Vec3::X, 0.3);
        let neg = RoadSample::new(Vec3::ZERO, Vec3::X, -0.3);
        assert!(pos.up.z > 0.05, "+bank should lean +Z: {:?}", pos.up);
        assert!(neg.up.z < -0.05, "-bank should lean -Z: {:?}", neg.up);
        // Mirror across the XZ->Y plane: same y, opposite z.
        assert!((pos.up.y - neg.up.y).abs() < 1e-6);
        assert!((pos.up.z + neg.up.z).abs() < 1e-6);
    }

    #[test]
    fn locate_finds_segment_and_fraction() {
        let l = line(&[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 0.0, 10.0]]);
        // Mid first segment.
        assert_eq!(l.locate(5.0), (0, 0.5));
        // Mid second segment (arc length 15 of 20).
        let (i, t) = l.locate(15.0);
        assert_eq!(i, 1);
        assert!((t - 0.5).abs() < 1e-5, "t {t}");
        // Clamps below and above the polyline.
        assert_eq!(l.locate(-3.0), (0, 0.0));
        let (i, t) = l.locate(100.0);
        assert_eq!(i, 1);
        assert!((t - 1.0).abs() < 1e-5, "t {t}");
    }
}
