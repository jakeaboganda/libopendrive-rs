//! Where the lanes the file links do not meet: a road shorter or longer than
//! its reference line, and a lane link across a gap.

use super::links::LaneMeta;
use super::Warning;
use crate::coords::{Point, Vector};
use crate::road::Road;
use crate::{Direction, Lane};

/// How far, in metres, a road's `length` may differ from where its
/// `<planView>` ends before it counts as a different length.
pub(super) const LENGTH_TOLERANCE: f64 = 0.01;

/// How far apart, in metres, a lane's exit and its successor's entry may be
/// before the link counts as crossing a gap. Across the test corpus a link
/// that meets is under 1 cm apart, and under 9 cm on maps converted from
/// OpenStreetMap. Each file fault the ASAM examples have is at least 0.9 m.
const GAP_TOLERANCE: f32 = 0.1;

/// A warning if `road`'s `length` is more than [`LENGTH_TOLERANCE`] from
/// where its `<planView>` ends: the last `<geometry>`'s `s` plus its
/// `length`.
pub(super) fn road_length(road: &Road) -> Option<Warning> {
    let last = road.geoms.last()?;
    let plan_view = last.s + last.length;
    ((road.length - plan_view).abs() > LENGTH_TOLERANCE).then(|| Warning::RoadLengthMismatch {
        road_id: road.od_id.clone(),
        length: road.length,
        plan_view,
    })
}

/// A warning for each link between `lanes` whose lanes are more than
/// [`GAP_TOLERANCE`] apart where one leaves off and the next begins, in lane
/// order. `metas` names each lane's road, section and `<lane id>`, in step
/// with `lanes`.
///
/// The ends are compared across the lane, from border to border, not at the
/// centerline. A lane that splits in two, or hands over to a lane opening
/// out of nothing beside it, meets its successor along its border.
pub(super) fn link_gaps(lanes: &[Lane], metas: &[LaneMeta]) -> Vec<Warning> {
    let position = |id: crate::LaneId| match lanes.get(id.0) {
        Some(l) if l.id == id => Some(id.0),
        _ => lanes.iter().position(|l| l.id == id),
    };
    let mut out = Vec::new();
    for (lane, from) in lanes.iter().zip(metas) {
        let exit = across(lane, lane.direction == Direction::Forward);
        for &next in &lane.successors {
            let Some(i) = position(next) else {
                continue;
            };
            let (to, into) = (&lanes[i], &metas[i]);
            let gap = distance(exit, across(to, to.direction == Direction::Backward));
            if gap <= GAP_TOLERANCE {
                continue;
            }
            out.push(Warning::LinkGap {
                road_id: from.road.clone(),
                section: from.section,
                lane: from.od_id,
                to_road_id: into.road.clone(),
                to_section: into.section,
                to_lane: into.od_id,
                gap: f64::from(gap),
            });
        }
    }
    out
}

/// The line across `lane` from border to border at the end of its
/// centerline, or at its start.
fn across(lane: &Lane, end: bool) -> [Point; 2] {
    let points = lane.center.points();
    let (i, s) = if end {
        (points.len() - 1, lane.center.length())
    } else {
        (0, 0.0)
    };
    let tangent = lane.center.tangents()[i];
    let half = lane.width_at(s) / 2.0;
    let bank = lane.bank_at(s);
    let left = Vector::new(-tangent.y, tangent.x, 0.0).normalize_or_zero();
    let reach = left * (half * bank.cos()) + Vector::Z * (half * bank.sin());
    [points[i] + reach, points[i] - reach]
}

/// The shortest distance between two line segments, either of which may be a
/// point.
fn distance([p, q]: [Point; 2], [r, s]: [Point; 2]) -> f32 {
    let (d1, d2, w) = (q - p, s - r, p - r);
    let (a, e, f) = (d1.dot(d1), d2.dot(d2), d2.dot(w));
    let (u, v) = if a <= f32::EPSILON && e <= f32::EPSILON {
        (0.0, 0.0)
    } else if a <= f32::EPSILON {
        (0.0, (f / e).clamp(0.0, 1.0))
    } else if e <= f32::EPSILON {
        ((-d1.dot(w) / a).clamp(0.0, 1.0), 0.0)
    } else {
        let (b, c) = (d1.dot(d2), d1.dot(w));
        let denom = a * e - b * b;
        let u = if denom > f32::EPSILON * a * e {
            ((b * f - c * e) / denom).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let v = (b * u + f) / e;
        if v < 0.0 {
            ((-c / a).clamp(0.0, 1.0), 0.0)
        } else if v > 1.0 {
            (((b - c) / a).clamp(0.0, 1.0), 1.0)
        } else {
            (u, v)
        }
    };
    ((p + d1 * u) - (r + d2 * v)).length()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(a: [f32; 3], b: [f32; 3]) -> [Point; 2] {
        [Point::from_array(a), Point::from_array(b)]
    }

    #[test]
    fn segments_that_cross_touch_and_parallel_ones_are_their_spacing_apart() {
        let crossing = distance(
            seg([0., -1., 0.], [0., 1., 0.]),
            seg([-1., 0., 0.], [1., 0., 0.]),
        );
        assert!(crossing < 1e-6);
        let parallel = distance(
            seg([0., 0., 0.], [0., 2., 0.]),
            seg([3., 1., 0.], [3., 4., 0.]),
        );
        assert!((parallel - 3.0).abs() < 1e-6);
        let past_the_end = distance(
            seg([0., 0., 0.], [0., 1., 0.]),
            seg([0., 3., 0.], [0., 5., 0.]),
        );
        assert!((past_the_end - 2.0).abs() < 1e-6);
        let above = distance(
            seg([0., -1., 0.], [0., 1., 0.]),
            seg([-1., 0., 1.], [1., 0., 1.]),
        );
        assert!((above - 1.0).abs() < 1e-6);
        let point = distance(
            seg([0., 0., 0.], [0., 0., 0.]),
            seg([1., -1., 0.], [1., 1., 0.]),
        );
        assert!((point - 1.0).abs() < 1e-6);
        let end_on_point = distance(
            seg([0., -3.5, 0.], [0., -7., 0.]),
            seg([0., -7., 0.], [0., -7., 0.]),
        );
        assert!(end_on_point < 1e-6);
    }
}
