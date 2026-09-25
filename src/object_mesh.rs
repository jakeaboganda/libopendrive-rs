//! Tessellating objects into triangles.

use std::ops::Range;

use crate::coords::{Point, Vector};
use crate::mesh::Mesh;
use crate::network::RoadNetwork;
use crate::object::{orient, Corner, Extent, ObjectId, Section, Shape};

/// Sides on a tessellated cylinder.
const CYLINDER_SEGMENTS: usize = 16;

/// Twice the area, in square metres, below which a triangle has none worth
/// drawing. A post given a height and no footprint is all such triangles.
const MIN_AREA: f32 = 1e-6;

/// The slice of a [`Mesh`] belonging to one object: a half-open range into
/// `vertices` (and, in step, `normals`) and one into `indices`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ObjectSpan {
    /// The object this slice was tessellated from.
    pub object: ObjectId,
    /// Its range in `Mesh::vertices` and `Mesh::normals`.
    pub vertices: Range<u32>,
    /// Its range in `Mesh::indices`.
    pub indices: Range<u32>,
}

impl RoadNetwork {
    /// Tessellate every object into one mesh. [`Mesh::objects`] names the
    /// object behind each slice.
    ///
    /// Every face has its own vertices, carrying that face's normal, so edges
    /// stay sharp. Faces are wound to face outward: away from a solid's
    /// middle, up from a lid, down from a floor. An open outline has no
    /// inside, so its walls keep the order of its corners.
    ///
    /// - A box is six faces and a cylinder is a prism of 16 sides.
    /// - An outline is a wall along each edge, from the corners' bases to their
    ///   tops. A closed one also has a lid over its tops and, if it has any
    ///   height, a floor under its bases.
    /// - A sweep is a wall up each side, a top, a floor and a cap at each end.
    ///
    /// A face with no area is left out, so an object with no volume has no
    /// span: a solid with no extent, or a post given a height and no
    /// footprint. Where a shape is flat, its two faces coincide and only one
    /// is kept: the lid of anything with no height, the +axis face of a box
    /// with one zero dimension, and the left wall of a sweep with no width.
    /// Draw those double-sided.
    ///
    /// A lid is triangulated in plan, so a closed outline must not cross
    /// itself, and one standing on its edge gets no lid.
    pub fn object_mesh(&self) -> Mesh {
        let mut mesh = Mesh::default();
        for object in self.objects() {
            let vertices = mesh.vertices.len() as u32;
            let indices = mesh.indices.len() as u32;
            match &object.shape {
                Shape::Solid {
                    position,
                    heading,
                    pitch,
                    roll,
                    extent: Some(extent),
                } => {
                    let frame = |local: [f32; 3]| {
                        let [u, v, z] = orient(
                            f64::from(*heading),
                            f64::from(*pitch),
                            f64::from(*roll),
                            local.map(f64::from),
                        );
                        Vector::new(u as f32, v as f32, z as f32)
                    };
                    match *extent {
                        Extent::Box {
                            length,
                            width,
                            height,
                        } => cuboid(&mut mesh, *position, frame, [length, width, height]),
                        Extent::Cylinder { radius, height } => {
                            cylinder(&mut mesh, *position, frame, radius, height)
                        }
                    }
                }
                Shape::Solid { extent: None, .. } => {}
                Shape::Outline { corners, closed } => outline(&mut mesh, corners, *closed),
                Shape::Sweep { sections } => sweep(&mut mesh, sections),
            }
            if mesh.indices.len() as u32 > indices {
                mesh.objects.push(ObjectSpan {
                    object: object.id,
                    vertices: vertices..mesh.vertices.len() as u32,
                    indices: indices..mesh.indices.len() as u32,
                });
            }
        }
        mesh
    }
}

/// A box centred on `origin` in plan and rising from it. `frame` turns an
/// offset in the box's own frame into the network's.
fn cuboid(mesh: &mut Mesh, origin: Point, frame: impl Fn([f32; 3]) -> Vector, size: [f32; 3]) {
    // `unit` runs 0..1 along each axis, centred across and rising up.
    let at = |unit: [f32; 3]| {
        origin
            + frame([
                (unit[0] - 0.5) * size[0],
                (unit[1] - 0.5) * size[1],
                unit[2] * size[2],
            ])
    };
    for axis in 0..3 {
        let (b, c) = ((axis + 1) % 3, (axis + 2) % 3);
        let mut direction = [0.0; 3];
        direction[axis] = 1.0;
        let out = frame(direction);
        // The + face, then the - face unless the box has no depth here and
        // it lies on the + one.
        for (side, out) in [(1.0, out), (0.0, -out)] {
            if side == 0.0 && size[axis] == 0.0 {
                continue;
            }
            let corners = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]].map(|[p, q]| {
                let mut unit = [0.0; 3];
                (unit[axis], unit[b], unit[c]) = (side, p, q);
                at(unit)
            });
            face(mesh, &corners, &fan(4), out);
        }
    }
}

/// An upright cylinder centred on `origin` and rising from it.
fn cylinder(
    mesh: &mut Mesh,
    origin: Point,
    frame: impl Fn([f32; 3]) -> Vector,
    radius: f32,
    height: f32,
) {
    let angle = |k: usize| k as f32 * std::f32::consts::TAU / CYLINDER_SEGMENTS as f32;
    let ring = |z: f32| -> Vec<Point> {
        (0..CYLINDER_SEGMENTS)
            .map(|k| {
                let (sin, cos) = angle(k).sin_cos();
                origin + frame([radius * cos, radius * sin, z])
            })
            .collect()
    };
    let (bottom, top) = (ring(0.0), ring(height));
    for k in 0..CYLINDER_SEGMENTS {
        let j = (k + 1) % CYLINDER_SEGMENTS;
        let (sin, cos) = (angle(k) + angle(1) / 2.0).sin_cos();
        let out = frame([cos, sin, 0.0]);
        face(mesh, &[bottom[k], bottom[j], top[j], top[k]], &fan(4), out);
    }
    let up = frame([0.0, 0.0, 1.0]);
    face(mesh, &top, &fan(CYLINDER_SEGMENTS), up);
    if height > 0.0 {
        face(mesh, &bottom, &fan(CYLINDER_SEGMENTS), -up);
    }
}

fn outline(mesh: &mut Mesh, corners: &[Corner], closed: bool) {
    let n = corners.len();
    let bases: Vec<Point> = corners.iter().map(|c| c.base).collect();
    let tops: Vec<Point> = corners.iter().map(|c| c.top).collect();
    let counter_clockwise = plan_area(&bases) >= 0.0;
    let edges = if closed { n } else { n - 1 };
    for i in 0..edges {
        let j = (i + 1) % n;
        let along = bases[j] - bases[i];
        let out = match (closed, counter_clockwise) {
            (false, _) => Vector::ZERO,
            (true, true) => Vector::new(along.y, -along.x, 0.0),
            (true, false) => Vector::new(-along.y, along.x, 0.0),
        };
        face(mesh, &[bases[i], bases[j], tops[j], tops[i]], &fan(4), out);
    }
    if closed {
        let plan: Vec<[f32; 2]> = tops.iter().map(|p| [p.x, p.y]).collect();
        let triangles = triangulate(&plan);
        face(mesh, &tops, &triangles, Vector::Z);
        if bases != tops {
            face(mesh, &bases, &triangles, -Vector::Z);
        }
    }
}

fn sweep(mesh: &mut Mesh, sections: &[Section]) {
    let thin = |s: &Section| s.left == s.right;
    let flat = |s: &Section| s.left.base == s.left.top && s.right.base == s.right.top;
    for pair in sections.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let along = b.left.base - a.left.base;
        let left = Vector::new(-along.y, along.x, 0.0);
        let (al, ar, bl, br) = (a.left, a.right, b.left, b.right);
        face(mesh, &[al.base, bl.base, bl.top, al.top], &fan(4), left);
        if thin(a) && thin(b) {
            continue;
        }
        face(mesh, &[ar.base, br.base, br.top, ar.top], &fan(4), -left);
        face(mesh, &[al.top, bl.top, br.top, ar.top], &fan(4), Vector::Z);
        if !(flat(a) && flat(b)) {
            face(
                mesh,
                &[al.base, bl.base, br.base, ar.base],
                &fan(4),
                -Vector::Z,
            );
        }
    }
    let (Some(first), Some(last)) = (sections.first(), sections.last()) else {
        return;
    };
    let along = last.left.base - first.left.base;
    for (s, out) in [(first, -along), (last, along)] {
        face(
            mesh,
            &[s.left.base, s.right.base, s.right.top, s.left.top],
            &fan(4),
            out,
        );
    }
}

/// The triangles of a convex polygon of `n` corners, fanned from the first.
fn fan(n: usize) -> Vec<[usize; 3]> {
    (1..n.saturating_sub(1)).map(|i| [0, i, i + 1]).collect()
}

/// One flat face: `points`, joined by `triangles`, each turned to face
/// `out`. An `out` of zero keeps the triangles as given. Triangles with no
/// area are dropped, and a face left with none adds nothing.
fn face(mesh: &mut Mesh, points: &[Point], triangles: &[[usize; 3]], out: Vector) {
    let mut kept = Vec::with_capacity(triangles.len());
    let mut sum = Vector::ZERO;
    for &[a, b, c] in triangles {
        let n = (points[b] - points[a]).cross(points[c] - points[a]);
        if n.length() < MIN_AREA {
            continue;
        }
        if n.dot(out) < 0.0 {
            kept.push([a, c, b]);
            sum = sum - n;
        } else {
            kept.push([a, b, c]);
            sum = sum + n;
        }
    }
    if kept.is_empty() {
        return;
    }
    let normal = sum.normalize_or(Vector::Z);
    let first = mesh.vertices.len() as u32;
    mesh.vertices.extend_from_slice(points);
    mesh.normals
        .extend(std::iter::repeat_n(normal, points.len()));
    mesh.indices
        .extend(kept.iter().flatten().map(|&i| first + i as u32));
}

/// Twice the signed area of `points` in plan: positive if they run
/// counter-clockwise seen from above.
fn plan_area(points: &[Point]) -> f32 {
    let n = points.len();
    (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum()
}

/// Triangles covering a simple polygon, by ear clipping. Wound
/// counter-clockwise whichever way the polygon runs. A polygon with no area
/// gets none, and one that crosses itself gets as many as clip cleanly.
fn triangulate(polygon: &[[f32; 2]]) -> Vec<[usize; 3]> {
    let turn = |a: usize, b: usize, c: usize| {
        let (a, b, c) = (polygon[a], polygon[b], polygon[c]);
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    let n = polygon.len();
    let area: f32 = (0..n)
        .map(|i| {
            let (a, b) = (polygon[i], polygon[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    if n < 3 || area.abs() < MIN_AREA {
        return Vec::new();
    }
    let mut ring: Vec<usize> = (0..n).collect();
    if area < 0.0 {
        ring.reverse();
    }
    let mut triangles = Vec::with_capacity(n - 2);
    while ring.len() > 3 {
        let m = ring.len();
        let corner = |i: usize| (ring[(i + m - 1) % m], ring[i], ring[(i + 1) % m]);
        let ear = (0..m).find(|&i| {
            let (a, b, c) = corner(i);
            turn(a, b, c) > 0.0
                && ring.iter().all(|&p| {
                    p == a
                        || p == b
                        || p == c
                        || turn(a, b, p) < 0.0
                        || turn(b, c, p) < 0.0
                        || turn(c, a, p) < 0.0
                })
        });
        if let Some(i) = ear {
            let (a, b, c) = corner(i);
            triangles.push([a, b, c]);
            ring.remove(i);
            continue;
        }
        // No ear: a corner on a straight run can hold the rest up, so drop
        // one. Without one, the polygon crosses itself.
        let straight = (0..m).find(|&i| {
            let (a, b, c) = corner(i);
            turn(a, b, c).abs() < MIN_AREA
        });
        match straight {
            Some(i) => {
                ring.remove(i);
            }
            None => return triangles,
        }
    }
    if turn(ring[0], ring[1], ring[2]) > 0.0 {
        triangles.push([ring[0], ring[1], ring[2]]);
    }
    triangles
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(polygon: &[[f32; 2]], triangles: &[[usize; 3]]) -> f32 {
        triangles
            .iter()
            .map(|&[a, b, c]| {
                let (a, b, c) = (polygon[a], polygon[b], polygon[c]);
                ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])) / 2.0
            })
            .sum()
    }

    #[test]
    fn ear_clipping_covers_a_concave_polygon_either_way_round() {
        // An L of area 3, which a fan from its first corner would get wrong.
        let l = [[0., 0.], [2., 0.], [2., 1.], [1., 1.], [1., 2.], [0., 2.]];
        let mut reversed = l;
        reversed.reverse();
        for polygon in [l, reversed] {
            let triangles = triangulate(&polygon);
            assert_eq!(triangles.len(), 4);
            assert!((area(&polygon, &triangles) - 3.0).abs() < 1e-6);
        }
    }

    #[test]
    fn ear_clipping_steps_over_a_corner_on_a_straight_edge() {
        let polygon = [[0., 0.], [1., 0.], [2., 0.], [2., 1.], [0., 1.]];
        let triangles = triangulate(&polygon);
        assert!((area(&polygon, &triangles) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn a_polygon_with_no_area_has_no_triangles() {
        assert!(triangulate(&[[0., 0.], [1., 0.], [2., 0.]]).is_empty());
    }
}
