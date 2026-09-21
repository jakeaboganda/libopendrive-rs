//! Smoke test: the importer survives a real, tool-exported OpenDRIVE map.
//!
//! `real_roads.xodr` is CARLA's Town07 (MIT-licensed; header preserved in the
//! file) -- 234 roads of lines + arcs + `laneOffset` + many lane sections, no
//! spirals. It pins the "loads without panicking, produces finite geometry"
//! bar for real files, and is what multiple lane sections were built for.

use libopendrive::load_file;

const REAL: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/town07.xodr");
const SPIRAL: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/spiral.xodr");
const PARAM: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/e6mini.xodr");

#[test]
fn real_map_loads_with_finite_geometry() {
    let net = load_file(REAL).expect("real OpenDRIVE map should load");
    assert!(net.driving_lanes().count() > 0, "no driving lanes imported");
    for lane in net.lanes() {
        let pts = lane.center.points();
        assert!(pts.len() >= 2, "lane {:?} has < 2 points", lane.id);
        assert!(
            pts.iter().all(|p| p.is_finite()),
            "lane {:?} has a non-finite point",
            lane.id
        );
        let len = lane.center.length();
        assert!(
            len.is_finite() && len > 0.0,
            "lane {:?} length {len}",
            lane.id
        );
    }
}

// A 100 m line then a 300 m clothoid (curvStart 0 -> curvEnd -0.02), so the
// heading swings by ~-3 rad. Every lane follows the reference, so a correctly
// imported spiral leaves the end heading far from the initial +X. With the
// spiral skipped the road stays straight (start == end) and this fails.
#[test]
fn spiral_road_actually_curves() {
    let net = load_file(SPIRAL).expect("spiral map should load");
    let lane = net
        .lanes()
        .iter()
        .max_by(|a, b| a.center.length().total_cmp(&b.center.length()))
        .expect("a lane");
    let start = lane.center.pose_at(0.0).heading;
    let end = lane.center.pose_at(lane.center.length()).heading;
    assert!(
        start.dot(end) < 0.0,
        "road did not curve: start {start:?} end {end:?} dot {}",
        start.dot(end)
    );
}

// A real, junction-rich map (Town07: 234 roads, 31 junctions) should resolve
// into a populated connectivity graph -- most lanes lead somewhere, and every
// successor is a real lane in the network.
#[test]
fn real_map_builds_a_connected_graph() {
    let net = load_file(REAL).expect("load");
    let total = net.driving_lanes().count();
    let with_succ = net
        .driving_lanes()
        .filter(|l| !l.successors.is_empty())
        .count();
    assert!(
        with_succ * 2 > total,
        "only {with_succ}/{total} lanes have a successor"
    );
    // Every successor id resolves to a real lane (no dangling references).
    for lane in net.lanes() {
        for s in &lane.successors {
            assert!(net.lane(*s).is_some(), "dangling successor {s:?}");
        }
    }
}

// esmini e6mini.xodr: a real highway built from paramPoly3 (+ line) geometry.
// With paramPoly3 skipped, its curved segments would be dropped; this pins that
// the real file imports into finite lanes.
#[test]
fn param_poly3_map_loads_with_finite_geometry() {
    let net = load_file(PARAM).expect("paramPoly3 map should load");
    assert!(net.driving_lanes().count() > 0, "no driving lanes");
    for lane in net.lanes() {
        assert!(
            lane.center.points().iter().all(|p| p.is_finite()),
            "lane {:?} has a non-finite point",
            lane.id
        );
    }
}
