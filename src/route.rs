//! Routing over the baked lane graph: a shortest lane path, sampled into a
//! drivable sequence of positions.
//!
//! Routes over both **longitudinal** (successor) and **lane-change** (lateral
//! neighbor) edges. Dijkstra with a flat lane-change penalty, then centerline
//! sampling in travel order (a lane change drives a short way, then hops to the
//! neighbor beside it for a path tracker to smooth). Speed is intentionally
//! absent: the route is geometry, and the caller stamps its own speed onto the
//! waypoints.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use crate::coords::Point;

use crate::network::{Direction, Lane, LaneId, RoadNetwork};

/// Waypoint spacing (meters) when sampling a route's lane centerlines.
const ROUTE_STEP: f32 = 2.0;
/// Cost (meters-equivalent) charged for a lane change, so the router prefers
/// staying in lane but will change when needed to reach the goal.
const LANE_CHANGE_COST: f32 = 30.0;
/// How far to drive along a lane before a lane change, so the transition isn't
/// an instantaneous sideways hop (the path tracker smooths the remaining jump).
const LANE_CHANGE_DIST: f32 = 10.0;

impl RoadNetwork {
    /// A drivable sequence of positions from `from` to `to` along the lane
    /// graph, or `None` if unreachable. Snaps each endpoint to its nearest
    /// driving lane, finds the shortest lane path over successor + lane-change
    /// edges, and samples the lane centerlines (in travel order) into points.
    pub fn route(&self, from: Point, to: Point) -> Option<Vec<Point>> {
        let (start, start_proj) = self.nearest_lane(from)?;
        let (goal, goal_proj) = self.nearest_lane(to)?;
        let path = self.lane_path(start, goal)?;
        Some(self.sample_route(&path, start_proj.s, goal_proj.s))
    }

    /// Shortest lane path `start -> goal` (Dijkstra): successor edges cost the
    /// traversed lane's length, lane-change edges a flat penalty. `None` if
    /// unreachable.
    fn lane_path(&self, start: LaneId, goal: LaneId) -> Option<Vec<LaneId>> {
        let mut dist: HashMap<LaneId, f32> = HashMap::from([(start, 0.0)]);
        let mut prev: HashMap<LaneId, LaneId> = HashMap::new();
        let mut heap = BinaryHeap::from([State {
            cost: 0.0,
            lane: start,
        }]);
        while let Some(State { cost, lane }) = heap.pop() {
            if lane == goal {
                return Some(reconstruct(&prev, start, goal));
            }
            if cost > *dist.get(&lane).unwrap_or(&f32::INFINITY) {
                continue; // a stale, longer entry
            }
            let Some(l) = self.lane(lane) else { continue };
            // Longitudinal edges cost the traversed lane's length; lane-change
            // edges cost a flat penalty.
            let step = l.center.length();
            let edges = (l.successors.iter().map(|&n| (n, cost + step)))
                .chain(l.neighbors.iter().map(|&n| (n, cost + LANE_CHANGE_COST)));
            for (next, nd) in edges {
                if nd < *dist.get(&next).unwrap_or(&f32::INFINITY) {
                    dist.insert(next, nd);
                    prev.insert(next, lane);
                    heap.push(State {
                        cost: nd,
                        lane: next,
                    });
                }
            }
        }
        None
    }

    /// Sample the lane path into points: each lane's centerline traversed in its
    /// travel direction, from the first lane's entry projection to the last
    /// lane's exit projection.
    fn sample_route(&self, path: &[LaneId], start_s: f32, goal_s: f32) -> Vec<Point> {
        let last = path.len().saturating_sub(1);
        let mut pts: Vec<Point> = Vec::new();
        // The handoff point between lanes; the next lane starts where this maps
        // onto it (so a lane change enters the neighbor beside where we left).
        let mut cursor: Option<Point> = None;
        for (i, &lid) in path.iter().enumerate() {
            let Some(lane) = self.lane(lid) else { continue };
            let len = lane.center.length();
            // Travel order: a forward lane runs along its polyline, a backward
            // one against it.
            let (travel_start, travel_end) = match lane.direction {
                Direction::Forward => (0.0, len),
                Direction::Backward => (len, 0.0),
            };
            let entry = match (i, cursor) {
                (0, _) => start_s.clamp(0.0, len),
                (_, Some(c)) => lane.center.project(c).s,
                _ => travel_start,
            };
            // Whether we leave this lane by a lane change (a neighbor, not a
            // successor): if so, drive only a short way before switching.
            let changes_off = path
                .get(i + 1)
                .is_some_and(|n| lane.neighbors.contains(n) && !lane.successors.contains(n));
            let exit = if i == last {
                goal_s.clamp(0.0, len)
            } else if changes_off {
                advance(entry, travel_end, LANE_CHANGE_DIST)
            } else {
                travel_end
            };
            sample_segment(lane, entry, exit, &mut pts);
            cursor = Some(lane.center.point_at(exit));
        }
        pts
    }
}

/// Move `dist` from `from` toward `toward`, without overshooting `toward`.
fn advance(from: f32, toward: f32, dist: f32) -> f32 {
    let step = (toward - from).signum() * dist;
    (from + step).clamp(from.min(toward), from.max(toward))
}

/// Append points along `lane`'s centerline from arc length `entry` to `exit`
/// (either direction), spaced ~`ROUTE_STEP`, deduping the join with prior lanes.
fn sample_segment(lane: &Lane, entry: f32, exit: f32, pts: &mut Vec<Point>) {
    let span = exit - entry;
    if span.abs() < 1e-4 {
        push_dedup(pts, lane.center.point_at(entry));
        return;
    }
    let steps = (span.abs() / ROUTE_STEP).ceil().max(1.0) as usize;
    for k in 0..=steps {
        let s = entry + span * (k as f32 / steps as f32);
        push_dedup(pts, lane.center.point_at(s));
    }
}

fn push_dedup(pts: &mut Vec<Point>, p: Point) {
    if pts.last().is_none_or(|q| q.distance_squared_to(p) > 1e-6) {
        pts.push(p);
    }
}

fn reconstruct(prev: &HashMap<LaneId, LaneId>, start: LaneId, goal: LaneId) -> Vec<LaneId> {
    let mut path = vec![goal];
    let mut cur = goal;
    while cur != start {
        cur = prev[&cur];
        path.push(cur);
    }
    path.reverse();
    path
}

/// A Dijkstra frontier entry, ordered as a min-heap on cost (ties by lane id,
/// for deterministic paths).
struct State {
    cost: f32,
    lane: LaneId,
}
impl PartialEq for State {
    fn eq(&self, o: &Self) -> bool {
        self.cost == o.cost && self.lane == o.lane
    }
}
impl Eq for State {}
impl Ord for State {
    fn cmp(&self, o: &Self) -> Ordering {
        o.cost
            .total_cmp(&self.cost)
            .then_with(|| self.lane.0.cmp(&o.lane.0))
    }
}
impl PartialOrd for State {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Polyline;
    use crate::network::LaneKind;

    fn lane(id: usize, pts: &[[f32; 3]], dir: Direction, succ: &[usize], nbrs: &[usize]) -> Lane {
        Lane {
            id: LaneId(id),
            kind: LaneKind::Driving,
            direction: dir,
            center: Polyline::new(pts.iter().map(|p| Point::from_array(*p)).collect()),
            width: 3.5,
            bank: Vec::new(),
            successors: succ.iter().map(|&s| LaneId(s)).collect(),
            predecessors: Vec::new(),
            neighbors: nbrs.iter().map(|&s| LaneId(s)).collect(),
        }
    }

    #[test]
    fn same_lane_route() {
        let net = RoadNetwork::new(vec![lane(
            0,
            &[[0.0, 0.0, 0.0], [20.0, 0.0, 0.0]],
            Direction::Forward,
            &[],
            &[],
        )]);
        let path = net
            .route(Point::new(2.0, 0.0, 0.0), Point::new(15.0, 0.0, 0.0))
            .expect("route");
        assert!(path.first().unwrap().x < 4.0, "starts near x=2");
        assert!(path.last().unwrap().x > 13.0, "ends near x=15");
        for w in path.windows(2) {
            assert!(w[1].x >= w[0].x - 0.01, "monotonic forward");
        }
    }

    #[test]
    fn two_lane_successor_route() {
        let net = RoadNetwork::new(vec![
            lane(
                0,
                &[[0.0, 0.0, 0.0], [20.0, 0.0, 0.0]],
                Direction::Forward,
                &[1],
                &[],
            ),
            lane(
                1,
                &[[20.0, 0.0, 0.0], [40.0, 0.0, 0.0]],
                Direction::Forward,
                &[],
                &[],
            ),
        ]);
        let path = net
            .route(Point::new(2.0, 0.0, 0.0), Point::new(38.0, 0.0, 0.0))
            .expect("route");
        assert!(path.first().unwrap().x < 4.0);
        assert!(path.last().unwrap().x > 36.0);
        assert!(path.iter().any(|p| p.x > 25.0), "crosses into lane B");
    }

    #[test]
    fn no_route_when_disconnected() {
        let net = RoadNetwork::new(vec![
            lane(
                0,
                &[[0.0, 0.0, 0.0], [20.0, 0.0, 0.0]],
                Direction::Forward,
                &[],
                &[],
            ),
            lane(
                1,
                &[[100.0, 0.0, 0.0], [120.0, 0.0, 0.0]],
                Direction::Forward,
                &[],
                &[],
            ),
        ]);
        assert!(net
            .route(Point::new(2.0, 0.0, 0.0), Point::new(110.0, 0.0, 0.0))
            .is_none());
    }

    /// Two separate two-lane strips, 500 m apart: nothing links them.
    fn two_components() -> RoadNetwork {
        RoadNetwork::new(vec![
            lane(
                0,
                &[[0.0, 0.0, 0.0], [20.0, 0.0, 0.0]],
                Direction::Forward,
                &[1],
                &[],
            ),
            lane(
                1,
                &[[20.0, 0.0, 0.0], [40.0, 0.0, 0.0]],
                Direction::Forward,
                &[],
                &[],
            ),
            lane(
                2,
                &[[500.0, 0.0, 0.0], [520.0, 0.0, 0.0]],
                Direction::Forward,
                &[3],
                &[],
            ),
            lane(
                3,
                &[[520.0, 0.0, 0.0], [540.0, 0.0, 0.0]],
                Direction::Forward,
                &[],
                &[],
            ),
        ])
    }

    #[test]
    fn a_route_between_disconnected_components_is_none() {
        // `None` means "no path", and the caller decides what to do about it.
        // Anything else, an empty plan or a straight line through the void,
        // would read as a route the vehicle could drive.
        let net = two_components();
        assert!(net
            .route(Point::new(2.0, 0.0, 0.0), Point::new(538.0, 0.0, 0.0))
            .is_none());
        // Unreachable in both directions, not just one.
        assert!(net
            .route(Point::new(502.0, 0.0, 0.0), Point::new(38.0, 0.0, 0.0))
            .is_none());
        // And each component still routes within itself, so the graph is sound
        // and it really is the gap that stopped it.
        assert!(net
            .route(Point::new(2.0, 0.0, 0.0), Point::new(38.0, 0.0, 0.0))
            .is_some());
        assert!(net
            .route(Point::new(502.0, 0.0, 0.0), Point::new(538.0, 0.0, 0.0))
            .is_some());
    }

    #[test]
    fn a_cyclic_lane_graph_terminates() {
        // A roundabout is a cycle, and every real map has them. Dijkstra
        // handles this by construction; the point is that nothing here walks
        // the graph naively, on the map where a hang costs the caller its
        // whole tick.
        let net = RoadNetwork::new(vec![
            lane(
                0,
                &[[0.0, 0.0, 0.0], [20.0, 0.0, 0.0]],
                Direction::Forward,
                &[1],
                &[],
            ),
            lane(
                1,
                &[[20.0, 0.0, 0.0], [20.0, 0.0, 20.0]],
                Direction::Forward,
                &[2],
                &[],
            ),
            lane(
                2,
                &[[20.0, 0.0, 20.0], [0.0, 0.0, 0.0]],
                Direction::Forward,
                &[0],
                &[],
            ),
            // Hangs off the ring but leads back nowhere.
            lane(
                3,
                &[[100.0, 0.0, 0.0], [120.0, 0.0, 0.0]],
                Direction::Forward,
                &[0],
                &[],
            ),
        ]);
        assert!(net
            .route(Point::new(1.0, 0.0, 0.0), Point::new(20.0, 0.0, 18.0))
            .is_some());
        // All the way round, back to where it started.
        assert!(net
            .route(Point::new(1.0, 0.0, 0.0), Point::new(19.0, 0.0, 0.0))
            .is_some());
        // Into the one-way spur: unreachable, and the cycle must not spin.
        assert!(net
            .route(Point::new(1.0, 0.0, 0.0), Point::new(118.0, 0.0, 0.0))
            .is_none());
    }

    #[test]
    fn routing_the_same_pair_twice_gives_an_identical_path() {
        // Two equal-cost routes to the same goal. The `total_cmp` + lane-id
        // tie-break in `State`'s `Ord` is what makes the choice between them
        // stable; without it the winner would follow heap/hash order, and two
        // identical requests could disagree.
        let net = RoadNetwork::new(vec![
            lane(
                0,
                &[[0.0, 0.0, 0.0], [20.0, 0.0, 0.0]],
                Direction::Forward,
                &[1, 2],
                &[],
            ),
            lane(
                1,
                &[[20.0, 0.0, 0.0], [40.0, 0.0, 5.0]],
                Direction::Forward,
                &[3],
                &[],
            ),
            lane(
                2,
                &[[20.0, 0.0, 0.0], [40.0, 0.0, -5.0]],
                Direction::Forward,
                &[3],
                &[],
            ),
            lane(
                3,
                &[[40.0, 0.0, 0.0], [60.0, 0.0, 0.0]],
                Direction::Forward,
                &[],
                &[],
            ),
        ]);
        let first = net
            .route(Point::new(1.0, 0.0, 0.0), Point::new(58.0, 0.0, 0.0))
            .expect("route");
        for _ in 0..20 {
            let again = net
                .route(Point::new(1.0, 0.0, 0.0), Point::new(58.0, 0.0, 0.0))
                .expect("route");
            assert_eq!(first, again, "the same request produced a different path");
        }
    }

    #[test]
    fn a_route_never_contains_a_non_finite_point() {
        // Waypoints go straight out as a plan, and a non-finite one reaches
        // whatever applies forces from there.
        let net = two_components();
        let route = net
            .route(Point::new(2.0, 0.0, 0.0), Point::new(38.0, 0.0, 0.0))
            .expect("route");
        assert!(route.iter().all(|p| p.is_finite()));

        // Including from a caller asking about somewhere absurd: the endpoints
        // snap to the nearest lane, so a far-away request is still answered in
        // real coordinates.
        let far = net.route(
            Point::new(1.0e30, 0.0, -1.0e30),
            Point::new(-1.0e30, 0.0, 1.0e30),
        );
        assert!(far.is_none_or(|r| r.iter().all(|p| p.is_finite())));

        // The built-in road, sampled end to end.
        let demo = crate::fixtures::demo_road();
        let (first, last) = (
            demo.lanes()[0].center.point_at(0.0),
            demo.lanes()[0]
                .center
                .point_at(demo.lanes()[0].center.length()),
        );
        if let Some(route) = demo.route(first, last) {
            assert!(route.iter().all(|p| p.is_finite()));
        }
    }

    #[test]
    fn route_changes_lanes_to_reach_the_exit() {
        // Lane A (z=0) is a dead end; its neighbor B (z=-3.5) leads to C. The
        // goal is on C, so the route must change A -> B, then drive B -> C.
        let net = RoadNetwork::new(vec![
            lane(
                0,
                &[[0.0, 0.0, 0.0], [20.0, 0.0, 0.0]],
                Direction::Forward,
                &[],
                &[1],
            ),
            lane(
                1,
                &[[0.0, 0.0, -3.5], [20.0, 0.0, -3.5]],
                Direction::Forward,
                &[2],
                &[0],
            ),
            lane(
                2,
                &[[20.0, 0.0, -3.5], [40.0, 0.0, -3.5]],
                Direction::Forward,
                &[],
                &[],
            ),
        ]);
        let path = net
            .route(Point::new(0.0, 0.0, 0.0), Point::new(38.0, 0.0, -3.5))
            .expect("route");
        assert!(
            path.iter().any(|p| p.z.abs() < 0.5),
            "starts on lane A (z~0)"
        );
        assert!(
            path.iter().any(|p| (p.z + 3.5).abs() < 0.5),
            "moves onto the z=-3.5 lanes"
        );
        assert!(path.last().unwrap().x > 36.0, "reaches C");
    }
}
