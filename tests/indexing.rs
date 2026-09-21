//! The lane index must answer exactly what a full scan answers.
//!
//! `nearest_lane` prunes candidates by a ground-plane grid. Pruning is where
//! this kind of optimisation goes wrong, silently, on the one query that
//! mattered. So it is checked against the brute-force answer on a real city
//! export, which has the stacked roads, dead ground, and far-off-map queries
//! a hand-written fixture would not.

use glam::Vec3;
use libopendrive::{load_file, LaneId, Projection, RoadNetwork};

const TOWN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/town07.xodr");

/// What `nearest_lane` did before the index: project onto every driving lane
/// and keep the closest in 3D, first one winning a tie.
fn scan_nearest(net: &RoadNetwork, point: Vec3) -> Option<(LaneId, Projection)> {
    net.driving_lanes()
        .map(|lane| (lane.id, lane.center.project(point)))
        .min_by(|(_, a), (_, b)| {
            let d = |p: &Projection| (point - p.point).length_squared();
            d(a).total_cmp(&d(b))
        })
}

/// Query points covering the cases the grid has to get right: on the road,
/// beside it, high above and far below it, out past the map, and at the
/// coordinate extremes.
fn probes(net: &RoadNetwork) -> Vec<Vec3> {
    let lanes: Vec<_> = net.driving_lanes().collect();
    let mut out = Vec::new();
    for i in 0..48 {
        let lane = lanes[i * lanes.len() / 48];
        let on = lane.center.point_at(lane.center.length() * 0.37);
        out.push(on);
        out.push(on + Vec3::new(7.5, 0.0, -7.5)); // off to one side
        out.push(on + Vec3::new(0.0, 60.0, 0.0)); // well above the surface
        out.push(on + Vec3::new(0.0, -60.0, 0.0)); // well below it
        out.push(on + Vec3::new(400.0, 0.0, 250.0)); // out past the map
    }
    out.extend([
        Vec3::ZERO,
        Vec3::new(1.0e9, 0.0, -1.0e9),
        Vec3::new(-1.0e9, 1.0e9, 0.0),
        Vec3::new(f32::MAX, 0.0, f32::MIN),
    ]);
    out
}

#[test]
fn the_lane_index_agrees_with_a_full_scan() {
    let net = load_file(TOWN).expect("town07 loads");
    for point in probes(&net) {
        let indexed = net.nearest_lane(point);
        let scanned = scan_nearest(&net, point);
        assert_eq!(
            indexed.map(|(id, _)| id),
            scanned.map(|(id, _)| id),
            "different lane at {point:?}"
        );
        assert_eq!(
            indexed.map(|(_, p)| p.point),
            scanned.map(|(_, p)| p.point),
            "different projection at {point:?}"
        );
    }
}

#[test]
fn a_non_finite_query_returns_rather_than_panicking() {
    // A NaN reaches here from a diverged integrator, not from a file. It has
    // no nearest anything; it must not take the process down on the way to
    // saying so.
    let net = load_file(TOWN).expect("town07 loads");
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let _ = net.nearest_lane(Vec3::new(bad, 0.0, 0.0));
        let _ = net.nearest_lane(Vec3::new(0.0, bad, 0.0));
        let _ = net.nearest_lane(Vec3::splat(bad));
        let _ = net.sample_near(Vec3::new(0.0, 0.0, bad));
    }
}

#[test]
fn an_empty_network_indexes_and_answers_nothing() {
    let empty = RoadNetwork::default();
    assert!(empty.nearest_lane(Vec3::ZERO).is_none());
    assert!(empty.sample_near(Vec3::ZERO).is_none());
    assert!(empty.route(Vec3::ZERO, Vec3::X).is_none());
}
