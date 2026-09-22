//! The importer's behavior on files it should not trust.
//!
//! `.xodr` files come from outside your program, exported by another tool or
//! handed over by whoever wants their map driven on. Everything downstream
//! (the lane polylines, the surface trimesh a physics collider is built from,
//! the routing graph) treats the imported network as sound, so this is the
//! boundary where that has to be made true.

use libopendrive::{load_file, load_str};

const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/");
/// The four full maps in `tests/data`: a hand-authored demo, an esmini
/// highway, a purpose-built test track, and a city export.
const FULL_MAPS: [&str; 4] = [
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/demo.xodr"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/e6mini.xodr"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/town07.xodr"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/testtrack.xodr"),
];

fn fixture(name: &str) -> libopendrive::RoadNetwork {
    load_file(format!("{DATA}{name}")).unwrap_or_else(|e| panic!("loading {name}: {e}"))
}

#[test]
fn malformed_xml_is_an_error_not_a_panic() {
    for junk in [
        "",
        "not xml at all",
        "<OpenDRIVE>",                                 // unclosed
        "<OpenDRIVE></OpenDRIVE>",                     // well-formed, empty
        "<OpenDRIVE><road/></OpenDRIVE>",              // a road with nothing in it
        "\u{feff}<?xml version=\"1.0\"?><OpenDRIVE/>", // BOM + empty
    ] {
        assert!(
            load_str(junk).is_err(),
            "expected an error for {junk:?}, got a network"
        );
    }
    assert!(load_file(format!("{DATA}does_not_exist.xodr")).is_err());
}

#[test]
fn a_road_with_no_geometry_is_skipped_rather_than_panicking() {
    // One unusable road must not cost the other 233 in a city export.
    let net = fixture("no_geometry.xodr");
    assert_eq!(net.driving_lanes().count(), 1, "the good road was lost");
    for lane in net.lanes() {
        assert!(lane.center.points().iter().all(|p| p.is_finite()));
    }
}

#[test]
fn a_zero_length_lane_is_dropped_not_baked_into_a_degenerate_polyline() {
    let net = fixture("zero_length.xodr");
    assert_eq!(
        net.driving_lanes().count(),
        1,
        "the degenerate road survived"
    );
    for lane in net.lanes() {
        assert!(
            lane.center.points().len() >= 2,
            "lane {:?} baked to a single point",
            lane.id
        );
        let length = lane.center.length();
        assert!(
            length.is_finite() && length > 0.0,
            "lane {:?} has length {length}",
            lane.id
        );
    }
}

#[test]
fn a_non_finite_coordinate_never_reaches_the_road_network() {
    // Rust's float parser accepts "NaN" and turns an out-of-range exponent
    // into infinity, so an XML attribute carries either straight through
    // unless the importer refuses it. A NaN vertex is also what makes the
    // road's trimesh collider fail to build.
    let net = fixture("non_finite.xodr");
    for lane in net.lanes() {
        for point in lane.center.points() {
            assert!(
                point.is_finite(),
                "lane {:?} carries a non-finite point {point:?}",
                lane.id
            );
        }
        assert!(lane.width.is_finite() && lane.width > 0.0);
    }
    // And the sound road in the same file still imports.
    assert!(net.driving_lanes().count() >= 1, "the good road was lost");
}

#[test]
fn a_lane_link_to_a_nonexistent_lane_is_dropped_at_import() {
    let net = fixture("dangling_link.xodr");
    for lane in net.lanes() {
        for successor in &lane.successors {
            assert!(
                net.lane(*successor).is_some(),
                "lane {:?} has dangling successor {successor:?}",
                lane.id
            );
        }
        for predecessor in &lane.predecessors {
            assert!(net.lane(*predecessor).is_some());
        }
    }
}

#[test]
fn every_referenced_lane_id_exists() {
    // The invariant the router walks on, checked across every full map:
    // successors, predecessors, and lane-change neighbours all resolve.
    for path in FULL_MAPS {
        let net = load_file(path).unwrap_or_else(|e| panic!("loading {path}: {e}"));
        assert!(
            net.driving_lanes().count() > 0,
            "{path} has no driving lanes"
        );
        for lane in net.lanes() {
            for (kind, ids) in [
                ("successor", &lane.successors),
                ("predecessor", &lane.predecessors),
                ("neighbor", &lane.neighbors),
            ] {
                for id in ids {
                    assert!(
                        net.lane(*id).is_some(),
                        "{path}: lane {:?} has dangling {kind} {id:?}",
                        lane.id
                    );
                }
            }
        }
    }
}

#[test]
fn every_full_map_tessellates_into_a_valid_trimesh() {
    // A consumer builds one static trimesh collider from this exact mesh,
    // usually during scene setup, with nowhere to report a failure. An
    // imported map's vertices trace back to an outside file, so "the mesh is
    // generated geometry we control" only holds if this does.
    for path in FULL_MAPS {
        let net = load_file(path).unwrap_or_else(|e| panic!("loading {path}: {e}"));
        let mesh = net.surface_mesh();
        mesh.validate()
            .unwrap_or_else(|e| panic!("{path} does not tessellate: {e}"));
        assert!(
            mesh.normals.len() == mesh.vertices.len(),
            "{path}: normals and vertices disagree"
        );
    }
}
