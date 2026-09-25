//! Tunnels and bridges cover the lanes the road says they do, over the
//! stretch it says.
//!
//! `objects.xodr` (see `objects.py`) has a tunnel over the last 25 m of road
//! 0, a 100 m arc of curvature 0.005 climbing at 2 %, and a bridge under lane
//! -1 of road 1, a flat 60 m straight. A lane on the arc is shorter or longer
//! than the reference line, so how far along the lane the tunnel starts is
//! not its `s`.

use libopendrive::{
    load_file, load_file_with_provenance, Coverage, LaneId, StructureId, StructureKind,
};

const OBJECTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/objects.xodr");

const CURVATURE: f64 = 0.005;

/// The lane of road `road` with OpenDRIVE id `od_id`.
fn lane(road: &str, od_id: i32) -> LaneId {
    let (_, prov) = load_file_with_provenance(OBJECTS).expect("objects.xodr loads");
    prov.lanes
        .iter()
        .find(|p| p.road_id == road && p.od_id == od_id)
        .map(|p| p.lane)
        .expect("lane")
}

/// Metres along a lane of road 0, offset `t` from the reference line, at
/// station `s`: its plan length shrinks by `1 - curvature t` round the bend,
/// and it climbs 2 m in every 100.
fn along_arc(s: f64, t: f64) -> f32 {
    let plan = 1.0 - CURVATURE * t;
    (s * (plan * plan + 0.02 * 0.02).sqrt()) as f32
}

fn assert_covers(got: &Coverage, lane: LaneId, from: f32, to: f32) {
    assert_eq!(got.lane, lane);
    assert!(
        (got.from - from).abs() < 1e-2 && (got.to - to).abs() < 1e-2,
        "{got:?}: expected {from}..{to}"
    );
}

#[test]
fn a_tunnel_covers_every_lane_over_its_stretch_of_road() {
    let net = load_file(OBJECTS).expect("objects.xodr loads");
    let tunnel = &net.structures()[0];
    assert_eq!(tunnel.name, "Hill");
    assert_eq!(
        tunnel.kind,
        StructureKind::Tunnel {
            kind: "standard".into(),
            lighting: Some(0.8),
            daylight: Some(0.1)
        }
    );
    // No <validity>, so both lanes, from s = 75 to the road's end at 100.
    let [left, right] = &tunnel.lanes[..] else {
        panic!("two lanes: {:?}", tunnel.lanes);
    };
    assert_covers(
        left,
        lane("0", 1),
        along_arc(75.0, 1.5),
        along_arc(100.0, 1.5),
    );
    assert_covers(
        right,
        lane("0", -1),
        along_arc(75.0, -1.5),
        along_arc(100.0, -1.5),
    );
    // Which is where the lane ends.
    let end = net.lane(lane("0", 1)).unwrap().center.length();
    assert!((left.to - end).abs() < 1e-3);
}

#[test]
fn a_bridge_covers_only_the_lanes_its_validity_names() {
    let net = load_file(OBJECTS).expect("objects.xodr loads");
    let bridge = &net.structures()[1];
    assert_eq!(bridge.name, "Creek");
    assert_eq!(
        bridge.kind,
        StructureKind::Bridge {
            kind: "concrete".into()
        }
    );
    let [only] = &bridge.lanes[..] else {
        panic!("one lane: {:?}", bridge.lanes);
    };
    assert_covers(only, lane("1", -1), 40.0, 55.0);
}

#[test]
fn a_lane_knows_which_structures_are_over_it() {
    let net = load_file(OBJECTS).expect("objects.xodr loads");
    let names = |road, od_id| {
        net.structures_over(lane(road, od_id))
            .map(|(s, _)| s.name.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(names("0", 1), ["Hill"]);
    assert_eq!(names("1", -1), ["Creek"]);
    assert!(names("1", 1).is_empty());
}

#[test]
fn provenance_names_each_structures_road_id_and_stretch() {
    let (net, prov) = load_file_with_provenance(OBJECTS).expect("objects.xodr loads");
    assert_eq!(prov.structures.len(), net.structures().len());
    let records: Vec<_> = prov
        .structures
        .iter()
        .map(|p| {
            (
                p.structure,
                p.road_id.as_str(),
                p.od_id.as_str(),
                p.s,
                p.length,
            )
        })
        .collect();
    assert_eq!(
        records,
        [
            (StructureId(0), "0", "20", 75.0, 25.0),
            (StructureId(1), "1", "21", 40.0, 15.0),
        ]
    );
    for s in net.structures() {
        assert_eq!(net.structure(s.id), Some(s));
    }
}
