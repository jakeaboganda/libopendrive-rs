use std::ops::Range;

use glam::{Quat, Vec3};

use crate::geometry::left_normal;
use crate::grid::{Aabb, Grid};
use crate::network::{LaneId, RoadNetwork};

/// A triangle mesh (Z-up, meters): per-vertex positions and up-normals, plus
/// triangle indices. The road surface, shared by the physics collider (which
/// needs only positions) and a renderer (which needs normals for lighting).
///
/// Deliberately a plain data type with public fields and no engine types in
/// sight: uploading it is a matter of copying three slices.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Mesh {
    /// Vertex positions (Z-up, metres).
    pub vertices: Vec<Vec3>,
    /// Per-vertex up-normals, parallel to `vertices`.
    pub normals: Vec<Vec3>,
    /// Triangle vertex indices, three per triangle.
    pub indices: Vec<u32>,
    /// Which lane each part of the mesh came from, in emission order.
    ///
    /// [`RoadNetwork::surface_mesh`] merges every lane into one buffer, which
    /// is what a collider and a single draw call want. This is the way back:
    /// it is what lets a renderer pick the lane under the cursor, give one
    /// lane its own material, or name the lane a degenerate triangle came
    /// from. Empty on a mesh built by hand.
    pub lanes: Vec<LaneSpan>,
}

/// The slice of a [`Mesh`] belonging to one lane: a half-open range into
/// `vertices` (and, in step, `normals`) and one into `indices`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LaneSpan {
    /// The lane this slice was tessellated from.
    pub lane: LaneId,
    /// Its range in `Mesh::vertices` and `Mesh::normals`.
    pub vertices: Range<u32>,
    /// Its range in `Mesh::indices`.
    pub indices: Range<u32>,
}

/// Why a mesh cannot be turned into a physics trimesh.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MeshError {
    /// No triangles at all.
    #[error("mesh has no triangles")]
    Empty,
    /// A vertex carries a `NaN` or an infinity.
    #[error("vertex {0} is not finite")]
    NonFiniteVertex(usize),
    /// A triangle names a vertex that does not exist.
    #[error("triangle {0} indexes vertex {1}, past the {2} vertices present")]
    IndexOutOfRange(usize, u32, usize),
    /// A triangle repeats a vertex, so it has no area.
    #[error("triangle {0} is degenerate (it repeats a vertex)")]
    DegenerateTriangle(usize),
}

impl Mesh {
    /// Whether this mesh can back a physics collider.
    ///
    /// Trimesh builders answer the same question, but typically by failing
    /// deep inside scene setup, where there is nothing to do but panic.
    /// Checking here lets an untrusted map be rejected at *load*, with the
    /// file named, which is what a bad `.xodr` deserves.
    pub fn validate(&self) -> Result<(), MeshError> {
        if self.indices.is_empty() {
            return Err(MeshError::Empty);
        }
        if let Some(i) = self.vertices.iter().position(|v| !v.is_finite()) {
            return Err(MeshError::NonFiniteVertex(i));
        }
        for (t, triangle) in self.indices.chunks_exact(3).enumerate() {
            for &index in triangle {
                if index as usize >= self.vertices.len() {
                    return Err(MeshError::IndexOutOfRange(t, index, self.vertices.len()));
                }
            }
            if triangle[0] == triangle[1]
                || triangle[1] == triangle[2]
                || triangle[0] == triangle[2]
            {
                return Err(MeshError::DegenerateTriangle(t));
            }
        }
        Ok(())
    }

    /// The surface height (world Z) and up-normal directly under `(x, y)`, by a
    /// vertical ray against the triangles. `None` if no triangle covers the point
    /// (off the road). Where triangles overlap (a bridge over a road) it returns
    /// the highest -- the surface you would be standing on.
    ///
    /// Height is the *actual* baked surface, faceted between vertices -- what the
    /// collider and viewer use, so a body draped against it sits on the road that
    /// is drawn. The normal, though, is barycentric-interpolated from the smooth
    /// per-vertex normals, not the flat triangle normal: a body oriented to the
    /// flat normal snaps as each wheel crosses a facet edge (visible attitude
    /// vibration), while the interpolated normal varies continuously.
    ///
    /// Scans every triangle. For more than a query or two, build a
    /// [`Mesh::sampler`] instead -- it answers the same thing off an index.
    pub fn height_at(&self, x: f32, y: f32) -> Option<(f32, Vec3)> {
        self.highest(0..self.indices.len() / 3, x, y)
    }

    /// An index over this mesh's triangles, for repeated [`Mesh::height_at`]
    /// queries.
    ///
    /// Building it is linear in the triangle count; each query afterwards
    /// touches only the triangles over the point rather than all of them. Two
    /// or three queries and it has paid for itself, and a per-wheel road
    /// conform on a city map does thousands per tick.
    pub fn sampler(&self) -> MeshSampler<'_> {
        MeshSampler::new(self)
    }

    /// The highest of `triangles` covering `(x, y)`, with its normal.
    fn highest(
        &self,
        triangles: impl IntoIterator<Item = usize>,
        x: f32,
        y: f32,
    ) -> Option<(f32, Vec3)> {
        let mut best: Option<(f32, Vec3)> = None;
        for t in triangles {
            let tri = &self.indices[t * 3..t * 3 + 3];
            let (ia, ib, ic) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
            let (a, b, c) = (self.vertices[ia], self.vertices[ib], self.vertices[ic]);
            // Barycentric coords of (x,y) in the triangle's XY projection.
            let det = (b.y - c.y) * (a.x - c.x) + (c.x - b.x) * (a.y - c.y);
            if det.abs() < 1e-9 {
                continue; // edge-on triangle: no XY footprint
            }
            let l1 = ((b.y - c.y) * (x - c.x) + (c.x - b.x) * (y - c.y)) / det;
            let l2 = ((c.y - a.y) * (x - c.x) + (a.x - c.x) * (y - c.y)) / det;
            let l3 = 1.0 - l1 - l2;
            // Reject outside-the-triangle, and non-finite with it. A query far
            // enough out overflows the barycentric arithmetic to infinity, and
            // every comparison against the resulting NaN is false -- so the
            // bounds test alone would wave it through and report a NaN height.
            if !(l1 >= -1e-4 && l2 >= -1e-4 && l3 >= -1e-4) {
                continue;
            }
            let height = l1 * a.z + l2 * b.z + l3 * c.z;
            if best.is_some_and(|(best_height, _)| height <= best_height) {
                continue;
            }
            // Smooth (interpolated vertex) normal, so a draped body doesn't snap
            // at facet edges. Fall back to the flat normal if this mesh carries no
            // per-vertex normals.
            let n = if self.normals.len() == self.vertices.len() {
                (l1 * self.normals[ia] + l2 * self.normals[ib] + l3 * self.normals[ic])
                    .normalize_or(Vec3::Z)
            } else {
                (b - a).cross(c - a).normalize_or(Vec3::Z)
            };
            let n = if n.z < 0.0 { -n } else { n }; // face up regardless of winding
            best = Some((height, n));
        }
        best
    }
}

/// A mesh plus a ground-plane index over its triangles. Borrows the mesh, so
/// it cannot outlive or diverge from the geometry it describes.
///
/// Answers exactly what [`Mesh::height_at`] answers, including which surface
/// wins where triangles stack.
#[derive(Debug, Clone)]
pub struct MeshSampler<'a> {
    mesh: &'a Mesh,
    grid: Grid,
}

impl<'a> MeshSampler<'a> {
    fn new(mesh: &'a Mesh) -> Self {
        let bounds: Vec<Aabb> = mesh
            .indices
            .chunks_exact(3)
            .filter_map(|tri| {
                Aabb::around(
                    tri.iter()
                        .filter_map(|&i| mesh.vertices.get(i as usize).map(|v| (v.x, v.y))),
                )
            })
            .collect();
        Self {
            mesh,
            grid: Grid::build(&bounds),
        }
    }

    /// The mesh this samples.
    pub fn mesh(&self) -> &'a Mesh {
        self.mesh
    }

    /// The surface height (world Z) and up-normal directly under `(x, y)` --
    /// see [`Mesh::height_at`], which this answers identically.
    pub fn height_at(&self, x: f32, y: f32) -> Option<(f32, Vec3)> {
        // A triangle covering the point overlaps the cell holding it, so the
        // one cell is the whole candidate set.
        let candidates = self.grid.at(x, y);
        self.mesh
            .highest(candidates.iter().map(|&t| t as usize), x, y)
    }
}

impl RoadNetwork {
    /// Tessellate every driving lane into one surface mesh-- a quad strip per
    /// lane, each rib offset +/-width/2 from the centerline along the per-vertex
    /// cross axis, carrying the centerline's elevation. On a superelevated lane
    /// the cross axis is tilted about the tangent by the local `bank`, so the
    /// outer edge of a banked curve rides above the inner one. Winding is
    /// consistent (triangles face up).
    ///
    /// Note: ribs use the vertex bisector normal, which keeps width consistent
    /// across vertices but does not guard against self-intersection on curves
    /// tighter than the half-width. An importer contract to enforce at bake.
    pub fn surface_mesh(&self) -> Mesh {
        let mut mesh = Mesh::default();
        for lane in self.driving_lanes() {
            let points = lane.center.points();
            let tangents = lane.center.tangents();
            let half = lane.width * 0.5;
            let base = mesh.vertices.len() as u32;
            let first_index = mesh.indices.len() as u32;
            for i in 0..points.len() {
                let along = tangents[i];
                // Cross axis, rolled about the (stored) tangent by the local
                // bank: +bank raises the left rib. Flat lanes (empty bank) leave
                // it the horizontal left normal, so the mesh is unchanged.
                let bank = lane.bank.get(i).copied().unwrap_or(0.0);
                let lateral = Quat::from_axis_angle(along, bank) * left_normal(along);
                // Surface up-normal: along × lateral is +Z for a flat road, and
                // tilts with grade/bank.
                let up = along.cross(lateral).normalize_or(Vec3::Z);
                mesh.vertices.push(points[i] + lateral * half);
                mesh.normals.push(up);
                mesh.vertices.push(points[i] - lateral * half);
                mesh.normals.push(up);
            }
            // Two triangles per segment, over the [left, right] rib pairs.
            for i in 0..points.len() as u32 - 1 {
                let l0 = base + i * 2;
                let (r0, l1, r1) = (l0 + 1, l0 + 2, l0 + 3);
                mesh.indices.extend_from_slice(&[l0, r0, r1, l0, r1, l1]);
            }
            mesh.lanes.push(LaneSpan {
                lane: lane.id,
                vertices: base..mesh.vertices.len() as u32,
                indices: first_index..mesh.indices.len() as u32,
            });
        }
        mesh
    }
}

#[cfg(test)]
mod tests {
    use crate::fixtures::demo_road;
    use crate::{Direction, Lane, LaneId, LaneKind, Mesh, Polyline, RoadNetwork};
    use glam::Vec3;

    #[test]
    fn one_straight_lane_tessellates_to_two_quads() {
        let net = RoadNetwork::new(vec![Lane {
            id: LaneId(0),
            kind: LaneKind::Driving,
            direction: Direction::Forward,
            center: Polyline::new(vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(5.0, 0.0, 0.0),
                Vec3::new(10.0, 0.0, 0.0),
            ]),
            width: 4.0,
            bank: Vec::new(),
            successors: Vec::new(),
            predecessors: Vec::new(),
            neighbors: Vec::new(),
        }]);
        let mesh = net.surface_mesh();
        // 3 ribs × 2 vertices; 2 segments × 2 triangles × 3 indices.
        assert_eq!(mesh.vertices.len(), 6);
        assert_eq!(mesh.normals.len(), 6);
        assert_eq!(mesh.indices.len(), 12);
        // The first rib straddles the centerline by +/-half-width in Y.
        assert!((mesh.vertices[0].y - mesh.vertices[1].y).abs() > 3.9);
        // A flat road faces straight up.
        assert!(mesh.normals[0].abs_diff_eq(Vec3::Z, 1e-5));
    }

    #[test]
    fn every_shipped_map_tessellates_into_a_valid_trimesh() {
        // The hand-authored road here; imported maps are covered by the
        // integration tests. A consumer builds one static trimesh collider
        // from exactly this mesh, so a degenerate tessellation is a panic at
        // scene setup.
        demo_road()
            .surface_mesh()
            .validate()
            .expect("the built-in demo road tessellates");
    }

    #[test]
    fn validate_rejects_what_a_collider_cannot_build() {
        use crate::MeshError;
        assert_eq!(Mesh::default().validate(), Err(MeshError::Empty));

        let sound = Mesh {
            vertices: vec![Vec3::ZERO, Vec3::X, Vec3::Y],
            normals: vec![Vec3::Z; 3],
            indices: vec![0, 1, 2],
            ..Default::default()
        };
        assert_eq!(sound.validate(), Ok(()));

        let mut nan = sound.clone();
        nan.vertices[1].x = f32::NAN;
        assert_eq!(nan.validate(), Err(MeshError::NonFiniteVertex(1)));

        let mut past_end = sound.clone();
        past_end.indices = vec![0, 1, 7];
        assert_eq!(
            past_end.validate(),
            Err(MeshError::IndexOutOfRange(0, 7, 3))
        );

        let mut degenerate = sound.clone();
        degenerate.indices = vec![0, 1, 1];
        assert_eq!(degenerate.validate(), Err(MeshError::DegenerateTriangle(0)));
    }

    #[test]
    fn a_banked_lane_tilts_its_ribs() {
        // A straight lane heading +X, canted a constant 0.2 rad: +bank raises
        // the left rib, so the left (first) vertex of each pair rides above the
        // right, and the up-normal leans off vertical.
        let net = RoadNetwork::new(vec![Lane {
            bank: vec![0.2; 3],
            ..Lane {
                id: LaneId(0),
                kind: LaneKind::Driving,
                direction: Direction::Forward,
                center: Polyline::new(vec![
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(5.0, 0.0, 0.0),
                    Vec3::new(10.0, 0.0, 0.0),
                ]),
                width: 4.0,
                bank: Vec::new(),
                successors: Vec::new(),
                predecessors: Vec::new(),
                neighbors: Vec::new(),
            }
        }]);
        let mesh = net.surface_mesh();
        mesh.validate().expect("a banked lane is a valid trimesh");
        // Left (index 0) above right (index 1) of the first rib pair.
        assert!(
            mesh.vertices[0].z - mesh.vertices[1].z > 0.5,
            "outer/left {} should ride above inner/right {}",
            mesh.vertices[0].z,
            mesh.vertices[1].z
        );
        // The up-normal is tilted but still points up.
        assert!(mesh.normals[0].z < 0.99 && mesh.normals[0].z > 0.9);
        assert!(
            (mesh.normals[0] - Vec3::Z).length() > 0.05,
            "normal should tilt"
        );
    }

    // Build a one-lane network from centerline points and a per-vertex bank.
    fn banked_net(points: Vec<Vec3>, bank: Vec<f32>, width: f32) -> RoadNetwork {
        RoadNetwork::new(vec![Lane {
            id: LaneId(0),
            kind: LaneKind::Driving,
            direction: Direction::Forward,
            center: Polyline::new(points),
            width,
            bank,
            successors: Vec::new(),
            predecessors: Vec::new(),
            neighbors: Vec::new(),
        }])
    }

    // A CURVED banked lane: a right-hand arc (curving toward -Y), so the left
    // edge is the OUTER edge. With a constant positive bank the outer edge must
    // ride above the inner one all the way round, every normal still points up,
    // and the mesh is a valid trimesh.
    #[test]
    fn a_banked_curve_lifts_the_outer_edge_and_stays_valid() {
        let r = 20.0_f32;
        // Circle centered at (0,-r,0): point = (r sinθ, r cosθ - r, 0). θ=0 -> +X
        // heading, y falls -> a right turn, so left_normal (+Y at start) is outer.
        let points: Vec<Vec3> = (0..=8)
            .map(|k| {
                let th = (k as f32) * (std::f32::consts::FRAC_PI_4 / 8.0);
                Vec3::new(r * th.sin(), r * th.cos() - r, 0.0)
            })
            .collect();
        let net = banked_net(points.clone(), vec![0.15; points.len()], 5.0);
        let mesh = net.surface_mesh();
        mesh.validate().expect("a banked curve tessellates");

        // Every normal still points generally up.
        assert!(
            mesh.normals.iter().all(|n| n.z > 0.9),
            "some normal fell below 0.9: {:?}",
            mesh.normals.iter().map(|n| n.z).fold(1.0_f32, f32::min)
        );
        // Outer (left, even index) rib rides above the inner (right, odd) rib at
        // every rib pair -- the physically-correct banked-curve profile.
        for i in 0..points.len() {
            let outer = mesh.vertices[2 * i].z;
            let inner = mesh.vertices[2 * i + 1].z;
            assert!(
                outer - inner > 0.5,
                "rib {i}: outer {outer} should ride above inner {inner}"
            );
        }
        // The normals are genuinely tilted, not vertical.
        assert!(mesh.normals.iter().any(|n| (n.z - 1.0).abs() > 0.005));
    }

    // Negative bank rolls the surface the other way: the RIGHT rib rides above
    // the left, and the normal leans toward +Y -- the mirror of positive bank.
    #[test]
    fn negative_bank_raises_the_opposite_rib() {
        let pts = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(5.0, 0.0, 0.0),
            Vec3::new(10.0, 0.0, 0.0),
        ];
        let neg = banked_net(pts.clone(), vec![-0.2; 3], 4.0).surface_mesh();
        let pos = banked_net(pts, vec![0.2; 3], 4.0).surface_mesh();
        neg.validate().expect("valid trimesh");
        // Right (index 1) above left (index 0) for negative bank.
        assert!(
            neg.vertices[1].z - neg.vertices[0].z > 0.5,
            "negative bank should raise the right rib: left {} right {}",
            neg.vertices[0].z,
            neg.vertices[1].z
        );
        // Exact mirror of the positive-bank mesh.
        assert!((neg.vertices[0].z + pos.vertices[0].z).abs() < 1e-5);
        assert!((neg.vertices[1].z + pos.vertices[1].z).abs() < 1e-5);
        // Normal leans toward +Y (positive bank leans -Y).
        assert!(neg.normals[0].y > 0.05, "neg normal {:?}", neg.normals[0]);
        assert!(pos.normals[0].y < -0.05, "pos normal {:?}", pos.normals[0]);
        assert!(neg.normals.iter().all(|n| n.z > 0.9));
    }

    // The two independent consumers of `bank` must agree: at each centerline
    // vertex the mesh's per-vertex up-normal equals the up-normal
    // `Lane::sample_at` derives at that vertex's arc length. Both roll +Z about
    // the same stored tangent by the same bank, so they cannot diverge.
    #[test]
    fn mesh_normal_matches_sample_at_up_at_each_vertex() {
        let pts = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(8.0, 0.0, 0.0),
            Vec3::new(16.0, 6.0, 0.0),
            Vec3::new(20.0, 16.0, 0.0),
        ];
        let bank = vec![0.05, 0.12, 0.18, 0.1];
        let net = banked_net(pts.clone(), bank, 5.0);
        let mesh = net.surface_mesh();
        let lane = &net.lanes()[0];
        // Cumulative arc length at each vertex.
        let mut s = 0.0_f32;
        for i in 0..pts.len() {
            if i > 0 {
                s += (pts[i] - pts[i - 1]).length();
            }
            let mesh_up = mesh.normals[2 * i]; // left and right share the vertex normal
            let sampled_up = lane.sample_at(s).up;
            assert!(
                mesh_up.abs_diff_eq(sampled_up, 1e-5),
                "vertex {i} (s={s}): mesh normal {mesh_up:?} != sample_at up {sampled_up:?}"
            );
        }
    }

    #[test]
    fn height_at_reads_the_surface_under_a_point() {
        // One flat 4 m-wide lane heading +X, from x=0 to x=10 at z=0.
        let net = RoadNetwork::new(vec![Lane {
            id: LaneId(0),
            kind: LaneKind::Driving,
            direction: Direction::Forward,
            center: Polyline::new(vec![Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0)]),
            width: 4.0,
            bank: Vec::new(),
            successors: Vec::new(),
            predecessors: Vec::new(),
            neighbors: Vec::new(),
        }]);
        let mesh = net.surface_mesh();
        // On the lane: z = 0, normal up.
        let (z, n) = mesh.height_at(5.0, 1.0).expect("on the lane");
        assert!(z.abs() < 1e-5, "z {z}");
        assert!(n.abs_diff_eq(Vec3::Z, 1e-5), "n {n:?}");
        // Off the lane (beyond the half-width): nothing under the point.
        assert!(mesh.height_at(5.0, 10.0).is_none());
    }

    #[test]
    fn height_at_normal_is_continuous_across_facets() {
        // The banked oval's curves are faceted (a polyline of segments). A body
        // oriented to the flat per-triangle normal snaps as it crosses each facet
        // edge -- visible attitude vibration. height_at interpolates the smooth
        // vertex normals, so the normal must vary continuously as the sample point
        // walks across facet boundaries, not jump.
        let mesh = crate::fixtures::banked_oval().surface_mesh();
        // Walk a fine line along a banked curve (the right curve: centre x=+35).
        let (mut prev, mut worst) = (None, 0.0f32);
        let mut s = 0.0;
        while s < std::f32::consts::PI {
            let x = 35.0 + 26.0 * s.cos();
            let y = -26.0 * s.sin();
            if let Some((_, n)) = mesh.height_at(x, y) {
                if let Some(p) = prev {
                    worst = worst.max((n as Vec3).angle_between(p).to_degrees());
                }
                prev = Some(n);
            }
            s += 0.02; // ~0.5 m steps, several per 2.5 m facet
        }
        // Interpolated normals drift by a fraction of a degree per step; the flat
        // per-facet normal would jump several degrees at each edge.
        assert!(
            worst < 1.5,
            "normal jumps {worst:.2} deg across a facet -- not interpolated?"
        );
    }

    #[test]
    fn height_at_follows_a_banked_cross_section() {
        // A banked lane: the surface height varies across the width, and the
        // normal tilts -- height_at reports the faceted surface, not a plane.
        let net = RoadNetwork::new(vec![Lane {
            id: LaneId(0),
            kind: LaneKind::Driving,
            direction: Direction::Forward,
            center: Polyline::new(vec![Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0)]),
            width: 6.0,
            bank: vec![0.2, 0.2],
            successors: Vec::new(),
            predecessors: Vec::new(),
            neighbors: Vec::new(),
        }]);
        let mesh = net.surface_mesh();
        // -y is to the right of +X travel (negative offset); the left (+bank)
        // edge is toward +y and rides higher.
        let (left, _) = mesh.height_at(5.0, 2.0).expect("left of centre");
        let (right, _) = mesh.height_at(5.0, -2.0).expect("right of centre");
        assert!(
            left > right + 0.3,
            "left {left} should ride above right {right}"
        );
        let (_, n) = mesh.height_at(5.0, 0.0).expect("centre");
        assert!(n.z < 0.999 && n.z > 0.9, "normal should tilt: {n:?}");
    }

    #[test]
    fn demo_road_mesh_is_non_degenerate_and_faces_up() {
        let mesh = demo_road().surface_mesh();
        assert!(!mesh.vertices.is_empty());
        assert_eq!(mesh.normals.len(), mesh.vertices.len());
        assert_eq!(mesh.indices.len() % 3, 0);
        assert!(mesh
            .indices
            .iter()
            .all(|&i| (i as usize) < mesh.vertices.len()));
        // The graded road tilts slightly but every normal still points upward.
        assert!(mesh.normals.iter().all(|n| n.z > 0.9));
    }
}
