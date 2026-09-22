//! Edge cases for `Mesh::height_at`, the vertical-ray surface sampler that
//! drapes a body onto the meshed road.
//!
//! These pin the corners the inline unit tests don't: a query exactly on a
//! shared edge/vertex, dead-centre of a quad, just outside, a degenerate/empty
//! mesh, overlapping (stacked) surfaces, and the returned normal being a unit
//! up-vector for either winding order.
//!
//! Companion to the inline tests in `src/mesh.rs`.

use libopendrive::{Mesh, Point, Vector};

/// A single flat quad at height `z` spanning x∈[0,2], y∈[0,2], split on the
/// (0,0)-(2,2) diagonal. `flip` reverses the winding of both triangles.
fn flat_quad(z: f32, flip: bool) -> Mesh {
    let vertices = vec![
        Point::new(0.0, 0.0, z), // 0
        Point::new(2.0, 0.0, z), // 1
        Point::new(2.0, 2.0, z), // 2
        Point::new(0.0, 2.0, z), // 3
    ];
    let indices = if flip {
        vec![0, 2, 1, 0, 3, 2]
    } else {
        vec![0, 1, 2, 0, 2, 3]
    };
    Mesh {
        vertices,
        normals: vec![Vector::Z; 4],
        indices,
        ..Default::default()
    }
}

#[test]
fn dead_centre_of_a_quad_resolves() {
    let mesh = flat_quad(3.0, false);
    // (1,1) is the quad centre -- it lies on the shared diagonal, so it must be
    // claimed by (at least) one triangle, not fall through the split.
    let (z, n) = mesh.height_at(1.0, 1.0).expect("centre of the quad");
    assert!((z - 3.0).abs() < 1e-5, "z {z}");
    assert!(n.abs_diff_eq(Vector::Z, 1e-6), "n {n:?}");
}

#[test]
fn a_point_on_a_shared_edge_and_on_a_vertex_resolves() {
    let mesh = flat_quad(1.0, false);
    // A point squarely on the shared diagonal edge (not the midpoint), inside
    // the barycentric tolerance of both triangles.
    assert!(
        mesh.height_at(0.5, 0.5).is_some(),
        "a point on the shared edge must resolve, not fall through the crack"
    );
    // Exactly on a shared vertex.
    assert!(
        mesh.height_at(2.0, 2.0).is_some(),
        "a shared vertex must resolve"
    );
    // On an outer edge midpoint.
    assert!(
        mesh.height_at(1.0, 0.0).is_some(),
        "outer edge must resolve"
    );
}

#[test]
fn a_point_just_outside_returns_none() {
    let mesh = flat_quad(0.0, false);
    // Comfortably clear of the quad footprint on every side.
    assert!(mesh.height_at(2.5, 1.0).is_none(), "past +x edge");
    assert!(mesh.height_at(-0.5, 1.0).is_none(), "past -x edge");
    assert!(mesh.height_at(1.0, 2.5).is_none(), "past +y edge");
    assert!(mesh.height_at(1.0, -0.5).is_none(), "past -y edge");
}

#[test]
fn a_degenerate_or_empty_mesh_returns_none() {
    // Empty: no triangles at all.
    assert!(Mesh::default().height_at(0.0, 0.0).is_none());

    // Degenerate: a triangle with zero XY footprint (all three vertices on a
    // vertical line) has an edge-on projection and must be skipped, not
    // divide-by-zero into a bogus hit.
    let edge_on = Mesh {
        vertices: vec![
            Point::new(1.0, 1.0, 0.0),
            Point::new(1.0, 1.0, 5.0),
            Point::new(1.0, 1.0, 2.0),
        ],
        normals: vec![Vector::Z; 3],
        indices: vec![0, 1, 2],
        ..Default::default()
    };
    assert!(edge_on.height_at(1.0, 1.0).is_none());
}

#[test]
fn overlapping_surfaces_return_the_higher_one() {
    // Two stacked quads over the same XY footprint (a bridge over a road): the
    // sampler must return the surface you'd stand on -- the higher.
    let mut low = flat_quad(0.0, false);
    let high = flat_quad(5.0, false);
    let base = low.vertices.len() as u32;
    low.vertices.extend(high.vertices);
    low.normals.extend(high.normals);
    low.indices.extend(high.indices.iter().map(|i| i + base));

    let (z, _) = low.height_at(1.0, 0.5).expect("both quads cover the point");
    assert!(
        (z - 5.0).abs() < 1e-5,
        "should pick the higher surface, got {z}"
    );

    // Order-independence: stack them the other way and still get the higher.
    let mut high_first = flat_quad(5.0, false);
    let low2 = flat_quad(0.0, false);
    let base = high_first.vertices.len() as u32;
    high_first.vertices.extend(low2.vertices);
    high_first.normals.extend(low2.normals);
    high_first
        .indices
        .extend(low2.indices.iter().map(|i| i + base));
    let (z, _) = high_first.height_at(1.0, 0.5).expect("covered");
    assert!(
        (z - 5.0).abs() < 1e-5,
        "higher regardless of order, got {z}"
    );
}

#[test]
fn the_returned_normal_is_a_unit_up_vector_for_both_windings() {
    for flip in [false, true] {
        let mesh = flat_quad(2.0, flip);
        let (_, n) = mesh.height_at(1.5, 0.5).expect("on the quad");
        assert!(
            (n.length() - 1.0).abs() < 1e-6,
            "normal {n:?} not unit (flip={flip})"
        );
        assert!(n.z > 0.0, "normal {n:?} must point up (flip={flip})");
        // A flat quad's normal is exactly +Z whichever way it is wound.
        assert!(
            n.abs_diff_eq(Vector::Z, 1e-6),
            "flat normal {n:?} should be +Z (flip={flip})"
        );
    }
}

#[test]
fn a_tilted_surface_returns_its_interpolated_leaning_normal_for_both_windings() {
    // height_at returns the mesh's own (interpolated) vertex normals, not a flat
    // per-facet normal -- so a body draped on it doesn't snap at facet edges. A
    // surface tilted about the X axis carries a leaning up-normal at every vertex;
    // the query must return that lean, unit and upward, whichever way it is wound.
    let vertices = vec![
        Point::new(0.0, 0.0, 0.0),
        Point::new(2.0, 0.0, 0.0),
        Point::new(2.0, 2.0, 1.0),
        Point::new(0.0, 2.0, 1.0),
    ];
    // The surface's true up-normal (perpendicular to the plane, pointing up).
    let up = (vertices[1] - vertices[0])
        .cross(vertices[3] - vertices[0])
        .normalize_or_zero();
    let up = if up.z < 0.0 { -up } else { up };
    assert!(up.z < 0.999, "the test surface should actually tilt");
    for flip in [false, true] {
        let indices = if flip {
            vec![0, 2, 1, 0, 3, 2]
        } else {
            vec![0, 1, 2, 0, 2, 3]
        };
        let mesh = Mesh {
            vertices: vertices.clone(),
            normals: vec![up; 4],
            indices,
            ..Default::default()
        };
        let (_, n) = mesh.height_at(1.0, 1.0).expect("on the tilted surface");
        assert!(
            (n.length() - 1.0).abs() < 1e-6,
            "tilted normal {n:?} not unit (flip={flip})"
        );
        assert!(n.z > 0.0, "tilted normal {n:?} must point up (flip={flip})");
        assert!(
            n.abs_diff_eq(up, 1e-5),
            "should return the interpolated vertex normal {up:?}, got {n:?} (flip={flip})"
        );
    }
}

#[test]
fn an_extreme_query_returns_none_rather_than_a_nan_height() {
    // The barycentric arithmetic overflows to infinity out here, and every
    // comparison against the resulting NaN is false -- so an outside-the-
    // triangle test written as `l < -eps` waves it through and reports a NaN
    // height, which lands in whatever is being draped onto the road.
    let mesh = flat_quad(1.0, false);
    for bad in [f32::MAX, f32::MIN, 1.0e30, -1.0e30, f32::INFINITY, f32::NAN] {
        for (x, y) in [(bad, 1.0), (1.0, bad), (bad, bad)] {
            let hit = mesh.height_at(x, y);
            assert!(
                hit.is_none_or(|(z, n)| z.is_finite() && n.is_finite()),
                "height_at({x}, {y}) returned {hit:?}"
            );
        }
    }
}
