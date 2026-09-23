//! Contiguous lane sections must meet flush. A lane section's end tangent is
//! the true curve tangent there, not its last chord, so the next section - which
//! starts from the same station - derives the same rib direction and the two
//! strips join without a V-shaped gap.

use libopendrive::{load_file_with_provenance, LaneId, LaneProvenance, Mesh, Point};

/// The mesh rib (left vertex, right vertex) at one end of a lane's strip.
fn rib(mesh: &Mesh, lane: LaneId, at_end: bool) -> Option<(Point, Point)> {
    let span = mesh.lanes.iter().find(|s| s.lane == lane)?;
    let (l, r) = if at_end {
        (
            span.vertices.end as usize - 2,
            span.vertices.end as usize - 1,
        )
    } else {
        (
            span.vertices.start as usize,
            span.vertices.start as usize + 1,
        )
    };
    Some((mesh.vertices[l], mesh.vertices[r]))
}

/// Geometrically contiguous lane pairs: same road, same OpenDRIVE lane id,
/// consecutive sections. Such a pair shares a station, so it shares a point.
fn joints(prov: &[LaneProvenance]) -> Vec<(LaneId, LaneId)> {
    let mut out = Vec::new();
    for a in prov {
        for b in prov {
            if a.road_id == b.road_id && a.od_id == b.od_id && b.section == a.section + 1 {
                out.push((a.lane, b.lane));
            }
        }
    }
    out
}

#[test]
fn section_joints_share_a_rib_direction() {
    let (net, prov) = load_file_with_provenance("tests/data/town07.xodr").unwrap();
    let mesh = net.surface_mesh();

    let mut checked = 0;
    for (a, b) in joints(&prov) {
        let (Some((a_l, a_r)), Some((b_l, b_r))) = (rib(&mesh, a, true), rib(&mesh, b, false))
        else {
            continue; // one side isn't a tessellated driving lane
        };
        // Direction only: a lane whose width steps at the joint legitimately
        // has a shorter rib, but the two ribs must still be parallel.
        let ends = (a_l - a_r).normalize_or_zero();
        let starts = (b_l - b_r).normalize_or_zero();
        checked += 1;
        assert!(
            ends.dot(starts) > 1.0 - 1e-5,
            "rib direction turns at the {a:?} -> {b:?} joint: {ends:?} then {starts:?}"
        );
    }
    assert!(checked > 300, "expected many section joints, got {checked}");
}

#[test]
fn equal_width_section_joints_have_no_gap() {
    let (net, prov) = load_file_with_provenance("tests/data/town07.xodr").unwrap();
    let mesh = net.surface_mesh();

    let mut checked = 0;
    for (a, b) in joints(&prov) {
        let (Some(la), Some(lb)) = (net.lane(a), net.lane(b)) else {
            continue;
        };
        if (la.width - lb.width).abs() > 1e-6 {
            continue; // a real width step, not a seam
        }
        let (Some((a_l, a_r)), Some((b_l, b_r))) = (rib(&mesh, a, true), rib(&mesh, b, false))
        else {
            continue;
        };
        let gap = (a_l - b_l).length().max((a_r - b_r).length());
        checked += 1;
        assert!(
            gap < 1e-4,
            "{gap} m seam between {a:?} and {b:?} at equal width {}",
            la.width
        );
    }
    assert!(
        checked > 300,
        "expected many equal-width joints, got {checked}"
    );
}
