//! Performance guardrails on the imported city map.
//!
//! The bound is the point, not the number. Routing is typically answered
//! inline on a simulation tick, so a request that takes long enough stalls
//! every body in the scene -- one query becoming everyone's dropped frame.

use std::time::Instant;

use libopendrive::load_file;

const TOWN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/town07.xodr");

/// One physics tick at 64 Hz. A routing request has to fit inside a tick with
/// room to spare, or the caller feels it.
const TICK: f64 = 1.0 / 64.0;

#[test]
fn routing_across_town07_stays_under_its_budget() {
    let net = load_file(TOWN).expect("load town07");
    let lanes: Vec<_> = net.driving_lanes().collect();
    assert!(
        lanes.len() > 500,
        "town07 imported only {} driving lanes; the budget means nothing on a \
         small map",
        lanes.len()
    );

    // Spread the sample across the map so the routes are long ones.
    let picks: Vec<glam::Vec3> = (0..16)
        .map(|i| {
            let lane = lanes[i * lanes.len() / 16];
            lane.center.point_at(lane.center.length() * 0.5)
        })
        .collect();

    let mut worst = 0.0_f64;
    let mut routed = 0;
    for from in &picks {
        for to in &picks {
            let start = Instant::now();
            let route = net.route(*from, *to);
            let elapsed = start.elapsed().as_secs_f64();
            worst = worst.max(elapsed);
            if route.is_some() {
                routed += 1;
            }
        }
    }

    assert!(routed > 0, "no pair in the sample routed at all");
    // Deliberately loose: a shared CI box is slow and this must not flake.
    // A regression to the O(lanes)-per-pop lookup blows past it by orders of
    // magnitude, which is what the guardrail is for.
    assert!(
        worst < TICK,
        "the slowest route took {worst:.4}s, past the {TICK:.4}s tick budget"
    );
    println!("worst route over town07: {worst:.6}s across {routed} routed pairs");
}
