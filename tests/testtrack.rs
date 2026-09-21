//! The purpose-built wheel/tire test track imports as the road it describes.
//!
//! `testtrack.xodr` exists so vehicle behaviour is exercised deliberately
//! rather than by whatever a city map happens to contain. That only works if
//! the sections it documents actually survive import -- a straight really
//! straight, a tight corner really tight, a crest that really rises and falls.
//! Otherwise a vehicle-tuning session is reading a road that isn't there.

use libopendrive::load_file;

const TRACK: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/testtrack.xodr");

/// Curvature at arc length `s`, from the change in heading over a short span.
fn curvature(lane: &libopendrive::Lane, s: f32) -> f32 {
    const SPAN: f32 = 4.0;
    let (a, b) = (
        lane.center.pose_at(s - SPAN * 0.5).heading,
        lane.center.pose_at(s + SPAN * 0.5).heading,
    );
    // Signed angle between the two headings, in the ground plane.
    let cross = a.z * b.x - a.x * b.z;
    let dot = a.x * b.x + a.z * b.z;
    (cross.atan2(dot) / SPAN).abs()
}

#[test]
fn the_test_track_has_the_sections_it_claims() {
    let net = load_file(TRACK).expect("the test track loads");
    assert_eq!(net.driving_lanes().count(), 2, "one lane each direction");

    let lane = net
        .driving_lanes()
        .max_by(|a, b| a.center.length().total_cmp(&b.center.length()))
        .expect("a driving lane");
    let length = lane.center.length();
    assert!(
        (length - 1053.0).abs() < 40.0,
        "track baked to {length:.1} m, expected ~1053"
    );

    // The straight really is straight, and the tight corner really is tight.
    // R=25 is the sharpest thing on the track outside the spiral's end.
    let straight = curvature(lane, 150.0);
    assert!(straight < 1e-3, "the opening straight curves at {straight}");
    let tight = curvature(lane, 764.0);
    assert!(
        tight > 1.0 / 40.0,
        "the R25 corner came out at radius {:.0} m",
        1.0 / tight.max(1e-6)
    );

    // The spiral tightens: curvature must grow along it, not sit constant.
    let (entry, exit) = (curvature(lane, 860.0), curvature(lane, 940.0));
    assert!(
        exit > entry * 1.5,
        "the spiral did not tighten: {entry:.5} -> {exit:.5} per metre"
    );

    // It climbs and comes back down -- the crest and dip are the point of the
    // elevation profile, so a flat import would be a silent loss.
    let heights: Vec<f32> = lane.center.points().iter().map(|p| p.y).collect();
    let low = heights.iter().copied().fold(f32::INFINITY, f32::min);
    let high = heights.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    assert!(
        high - low > 3.0,
        "the track is nearly flat: {low:.2} m to {high:.2} m"
    );
    let crest = lane.center.point_at(405.0).y;
    assert!(
        crest > lane.center.point_at(300.0).y + 2.0,
        "no crest: {crest:.2} m against the climb's start"
    );
    assert!(
        crest > lane.center.point_at(470.0).y + 1.0,
        "the crest does not fall away into the dip"
    );

    assert!(lane.center.points().iter().all(|p| p.is_finite()));
}

#[test]
fn the_test_track_curves_are_super_elevated() {
    let net = load_file(TRACK).expect("the test track loads");
    let lane = net
        .driving_lanes()
        .max_by(|a, b| a.center.length().total_cmp(&b.center.length()))
        .expect("a driving lane");
    let len = lane.center.length();

    // The opening straight (before any curve, so no arc-length drift) is dead
    // flat -- superelevation is confined to the curves.
    assert!(
        lane.bank_at(150.0).abs() < 1e-3,
        "opening straight is banked: {}",
        lane.bank_at(150.0)
    );

    // Scan the whole lane for the extreme bank each way. Proper banking raises
    // the outer edge of these left-hand turns, which is negative superelevation,
    // so every sample is <= 0 and the deepest reaches the spiral's ~0.20 rad.
    let (mut deepest, mut highest) = (0.0f32, 0.0f32);
    let mut s = 0.0;
    while s <= len {
        let b = lane.bank_at(s);
        deepest = deepest.min(b);
        highest = highest.max(b);
        s += 1.0;
    }
    assert!(
        (-0.21..=-0.19).contains(&deepest),
        "deepest bank {deepest} should approach -0.20 (the spiral peak)"
    );
    assert!(
        highest < 1e-3,
        "found adverse (positive) bank {highest}; every turn should bank the same way"
    );

    // The banked track still tessellates to a valid collider (the whole point of
    // reading superelevation: before it, a banked road could not import).
    net.surface_mesh()
        .validate()
        .expect("the banked test track tessellates");
}

// --- Per-curve banking, flat straights, the baked mesh cant, and both-lane
// coverage. `the_test_track_curves_are_super_elevated` above scans the whole
// lane for one extremum each way; these pin each curve individually and check
// the geometry.

/// Deepest (most negative) bank over `[lo, hi]` lane arc length, scanned finely
/// so lane-vs-road arc-length drift can't miss the peak.
fn deepest_bank(lane: &libopendrive::Lane, lo: f32, hi: f32) -> f32 {
    let (mut best, mut s) = (0.0f32, lo);
    while s <= hi {
        best = best.min(lane.bank_at(s));
        s += 0.25;
    }
    best
}

/// Largest |bank| over `[lo, hi]` -- for asserting a stretch reads flat.
fn worst_abs_bank(lane: &libopendrive::Lane, lo: f32, hi: f32) -> f32 {
    let (mut m, mut s) = (0.0f32, lo);
    while s <= hi {
        m = m.max(lane.bank_at(s).abs());
        s += 0.5;
    }
    m
}

fn longest_lane(net: &libopendrive::RoadNetwork) -> &libopendrive::Lane {
    net.driving_lanes()
        .max_by(|a, b| a.center.length().total_cmp(&b.center.length()))
        .expect("a driving lane")
}

#[test]
fn each_left_curve_banks_toward_its_outer_edge_at_its_own_peak() {
    let net = load_file(TRACK).expect("the test track loads");
    let lane = longest_lane(&net);

    // All three curves turn left, so proper banking is negative superelevation.
    // Windows are wide enough to swallow lane-vs-road arc-length drift; the
    // deepest sample in each is the authored plateau/peak.
    let r60 = deepest_bank(lane, 610.0, 690.0);
    let r25 = deepest_bank(lane, 748.0, 782.0);
    let spiral = deepest_bank(lane, 850.0, 955.0);

    assert!(
        (-0.105..=-0.095).contains(&r60),
        "R=60 sweeper peaked at {r60}, expected ~-0.10"
    );
    assert!(
        (-0.185..=-0.175).contains(&r25),
        "R=25 corner peaked at {r25}, expected ~-0.18"
    );
    assert!(
        (-0.205..=-0.185).contains(&spiral),
        "spiral peaked at {spiral}, expected ~-0.20"
    );

    // Distinguish the curves: the corner out-banks the sweeper, the spiral is
    // deepest of all -- a check that the profile isn't a single value smeared
    // across every curve.
    assert!(
        spiral < r25 && r25 < r60,
        "banking should deepen R60 -> R25 -> spiral, got {r60} / {r25} / {spiral}"
    );

    // No adverse (positive) bank anywhere inside a curve -- every turn cants the
    // same way, never against the corner.
    for (name, lo, hi) in [
        ("R60", 610.0, 690.0),
        ("R25", 748.0, 782.0),
        ("spiral", 850.0, 955.0),
    ] {
        let (mut hi_bank, mut s) = (f32::NEG_INFINITY, lo);
        while s <= hi {
            hi_bank = hi_bank.max(lane.bank_at(s));
            s += 0.25;
        }
        assert!(
            hi_bank < 1e-3,
            "the {name} curve has an adverse (positive) bank {hi_bank}"
        );
    }
}

#[test]
fn the_straights_between_the_curves_stay_flat() {
    let net = load_file(TRACK).expect("the test track loads");
    // Check both lanes: a flat straight must read flat whichever way it's driven.
    for lane in net.driving_lanes() {
        // Interior windows, held clear of the ramp-in/out at each curve's ends
        // so lane-vs-road arc-length drift can't leak curve bank into them.
        for (name, lo, hi) in [
            ("opening", 20.0, 590.0),   // before the R60 sweeper
            ("settle", 705.0, 738.0),   // R60 -> R25
            ("mid", 790.0, 828.0),      // R25 -> spiral
            ("run-out", 965.0, 1030.0), // after the spiral
        ] {
            let m = worst_abs_bank(lane, lo, hi);
            assert!(
                m < 3e-3,
                "the {name} straight on lane {:?} is banked: |bank| up to {m}",
                lane.id
            );
        }
    }
}

#[test]
fn the_cant_is_baked_into_the_real_surface_mesh() {
    let net = load_file(TRACK).expect("the test track loads");
    let lane = longest_lane(&net);

    // Apex of the R=25 corner: the lane point at its deepest bank there.
    let (mut apex_s, mut best, mut s) = (760.0f32, 0.0f32, 748.0f32);
    while s <= 782.0 {
        let b = lane.bank_at(s);
        if b < best {
            best = b;
            apex_s = s;
        }
        s += 0.25;
    }
    let apex = lane.center.point_at(apex_s);

    let mesh = net.surface_mesh();
    // Rib pairs are pushed [left, right]; find the pair whose left vertex is
    // nearest the apex and confirm the RIGHT (outer, for a left turn) vertex
    // rides physically higher -- the cant is in the collider/viz geometry, not
    // just the `bank` scalar.
    let pair = mesh
        .vertices
        .chunks_exact(2)
        .min_by(|a, b| {
            (a[0] - apex)
                .length_squared()
                .total_cmp(&(b[0] - apex).length_squared())
        })
        .expect("the mesh has rib pairs");
    let (left, right) = (pair[0], pair[1]);
    assert!(
        (left - apex).length() < 2.5,
        "no mesh rib near the R25 apex: nearest left vertex {left:?} vs apex {apex:?}"
    );
    assert!(
        right.y - left.y > 0.2,
        "the outer edge does not ride higher at the apex: left.y {:.3}, right.y {:.3}",
        left.y,
        right.y
    );

    // Global invariant: across the whole mesh no rib is canted the wrong way
    // (right below left) -- every banked section raises its outer/right edge,
    // and the deepest section lifts it by the full ~2*half*sin(0.2) ~ 0.68 m.
    let (mut min_d, mut max_d) = (f32::INFINITY, f32::NEG_INFINITY);
    for p in mesh.vertices.chunks_exact(2) {
        let d = p[1].y - p[0].y;
        min_d = min_d.min(d);
        max_d = max_d.max(d);
    }
    assert!(
        min_d > -1e-3,
        "a mesh rib banks the wrong way (right below left): min delta {min_d}"
    );
    assert!(
        max_d > 0.6,
        "the deepest mesh cant is too shallow: max right-left delta {max_d}"
    );
}

#[test]
fn both_lanes_carry_the_road_level_banking() {
    let net = load_file(TRACK).expect("the test track loads");
    let (mut saw_forward, mut saw_backward) = (false, false);
    for lane in net.driving_lanes() {
        // The stored bank profile is parallel to the centerline...
        assert!(
            !lane.bank.is_empty(),
            "lane {:?} has no bank profile",
            lane.id
        );
        assert_eq!(
            lane.bank.len(),
            lane.center.points().len(),
            "lane {:?} bank profile is not parallel to its centerline",
            lane.id
        );
        // ...and both directions reach the deep spiral bank: superelevation is a
        // road-level property, applied to the left (Backward) and right
        // (Forward) lane alike, not just the one the scan above happens to pick.
        let deepest = deepest_bank(lane, 0.0, lane.center.length());
        assert!(
            deepest < -0.15,
            "lane {:?} ({:?}) barely banks: deepest {deepest}",
            lane.id,
            lane.direction
        );
        match lane.direction {
            libopendrive::Direction::Forward => saw_forward = true,
            libopendrive::Direction::Backward => saw_backward = true,
        }
    }
    assert!(
        saw_forward && saw_backward,
        "expected one Forward and one Backward driving lane"
    );
}
