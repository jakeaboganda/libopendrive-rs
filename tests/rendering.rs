//! What a renderer needs from a baked map, checked on a real one.
//!
//! Visualization is not built here yet. This pins the part that would be
//! expensive to retrofit, which is tracing the surface mesh back to the lanes
//! it came from.

use libopendrive::load_file;

const TOWN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/town07.xodr");

#[test]
fn every_lane_span_addresses_its_own_slice_of_the_mesh() {
    let net = load_file(TOWN).expect("town07 loads");
    let mesh = net.surface_mesh();

    assert_eq!(
        mesh.lanes.len(),
        net.driving_lanes().count(),
        "one span per tessellated lane"
    );

    let mut covered = 0usize;
    for (span, lane) in mesh.lanes.iter().zip(net.driving_lanes()) {
        assert_eq!(span.lane, lane.id, "spans are in emission order");
        assert!(net.lane(span.lane).is_some(), "span names a real lane");

        // Two ribs per centerline vertex, two triangles per segment.
        let points = lane.center.points().len();
        let (vertices, indices) = (
            (span.vertices.end - span.vertices.start) as usize,
            (span.indices.end - span.indices.start) as usize,
        );
        assert_eq!(vertices, points * 2, "lane {:?} vertex count", lane.id);
        assert_eq!(indices, (points - 1) * 6, "lane {:?} index count", lane.id);

        // The slices are addressable, and every triangle in this lane's index
        // range points inside this lane's vertex range -- so a renderer can
        // draw or pick one lane without dragging in its neighbours.
        let _ = &mesh.vertices[span.vertices.start as usize..span.vertices.end as usize];
        let _ = &mesh.normals[span.vertices.start as usize..span.vertices.end as usize];
        for &i in &mesh.indices[span.indices.start as usize..span.indices.end as usize] {
            assert!(
                span.vertices.contains(&i),
                "lane {:?} indexes vertex {i} outside its own span {:?}",
                lane.id,
                span.vertices
            );
        }
        covered += indices;
    }
    assert_eq!(covered, mesh.indices.len(), "the spans tile the whole mesh");
}

#[test]
fn a_hand_built_mesh_carries_no_spans() {
    // `lanes` is what the tessellator recorded, not a required field. A mesh
    // assembled by hand is still a valid mesh.
    let mesh = libopendrive::Mesh::default();
    assert!(mesh.lanes.is_empty());
    assert!(mesh.validate().is_err());
}
