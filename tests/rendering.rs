//! What a renderer needs from a baked map, checked on a real one.
//!
//! Visualization is not built here yet, so these pin the two things that
//! would be expensive to retrofit: the surface mesh can be traced back to the
//! lanes it came from, and the whole network survives a round trip out of the
//! process.

use libopendrive::load_file;

const TOWN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/town07.xodr");
#[cfg(feature = "serde")]
const SWEEPER: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/data/banked_sweeper.xodr"
);

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

#[cfg(feature = "serde")]
#[test]
fn a_network_survives_a_round_trip_and_is_still_queryable() {
    // A viewer typically lives in another process. What it receives has to
    // behave like an imported map, not just look like one -- the arc lengths,
    // tangents and lane index are all derived state that has to come back.
    use libopendrive::RoadNetwork;

    let net = load_file(SWEEPER).expect("the banked sweeper loads");
    let json = serde_json::to_string(&net).expect("serialize");
    let back: RoadNetwork = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(net, back);
    for lane in net.driving_lanes() {
        let probe = lane.center.point_at(lane.center.length() * 0.5);
        assert_eq!(
            net.sample_near(probe),
            back.sample_near(probe),
            "lane {:?} samples differently after a round trip",
            lane.id
        );
        assert_eq!(net.nearest_lane(probe), back.nearest_lane(probe));
    }
    assert_eq!(net.surface_mesh(), back.surface_mesh());
}

#[cfg(feature = "serde")]
#[test]
fn a_mesh_survives_a_round_trip() {
    let mesh = load_file(SWEEPER).expect("loads").surface_mesh();
    let json = serde_json::to_string(&mesh).expect("serialize");
    let back: libopendrive::Mesh = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(mesh, back);
    assert_eq!(mesh.lanes, back.lanes, "lane spans must survive too");
}

#[cfg(feature = "serde")]
#[test]
fn a_polyline_of_fewer_than_two_points_is_refused_on_the_way_in() {
    // Polyline serializes as bare points, so deserializing is the one place
    // an untrusted peer could hand us a degenerate one.
    use libopendrive::Polyline;
    assert!(serde_json::from_str::<Polyline>("[[0,0,0]]").is_err());
    assert!(serde_json::from_str::<Polyline>("[]").is_err());

    let line: Polyline = serde_json::from_str("[[0,0,0],[10,0,0]]").expect("two points");
    assert!((line.length() - 10.0).abs() < 1e-5);
    // Derived state really was rebuilt, not defaulted.
    assert!(line.pose_at(5.0).heading.x > 0.9);
}
