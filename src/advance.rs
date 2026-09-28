//! Moving a lane position along the lanes: a distance down the lane graph, and
//! across to the lane beside it.

use crate::network::{Direction, LaneId, RoadNetwork};
use crate::road::{station_at, LanePosition};

/// Where one branch of [`RoadNetwork::advance`] ended.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Advance {
    /// The branch went the whole distance, to here.
    Reached(LanePosition),
    /// The branch ran out of lanes: the lane it was on ends with nothing
    /// after it. `at` is the end of that lane, and `short` how far short of
    /// the distance it stopped, in metres.
    DeadEnd {
        /// Where the lane ends.
        at: LanePosition,
        /// The distance left, in metres.
        short: f64,
    },
}

impl Advance {
    /// Where the branch ended, whether it went the whole distance or not.
    pub fn position(&self) -> LanePosition {
        match *self {
            Self::Reached(at) | Self::DeadEnd { at, .. } => at,
        }
    }
}

/// One branch in flight: the lane it is on, how far along its centerline,
/// and the distance still to go.
struct Branch {
    lane: LaneId,
    arc: f32,
    remaining: f64,
}

impl RoadNetwork {
    /// Every place `distance` metres on from `from` along the lanes, one per
    /// branch, as CARLA's `Waypoint::GetNext` gives them. The caller picks.
    ///
    /// A positive distance goes the way the lane's traffic runs, onto its
    /// [`successors`](crate::Lane::successors), and a negative one back
    /// against it, onto its predecessors. The distance is measured along the
    /// lanes' centerlines, so it is how far a vehicle on them travels.
    /// esmini's `MoveAlongS` and CARLA step by road `s` instead, which is
    /// shorter on the outside of a bend. The offset keeps its side of the
    /// traffic, so it flips sign onto a road that runs the other way.
    ///
    /// A branch that reaches a lane with nothing after it stops there, as an
    /// [`Advance::DeadEnd`]. Branches that meet again give the same place
    /// once. Every fork on the way splits the branches, so a long distance
    /// through a city can give many: step in short distances. Empty if `from`
    /// is not on a lane of the network's roads.
    pub fn advance(&self, from: LanePosition, distance: f64) -> Vec<Advance> {
        let (Some(lane), Some(arc)) = (self.lane(from.lane), self.centerline_s(from)) else {
            return Vec::new();
        };
        let ahead = distance >= 0.0;
        let side = from.offset * travel(lane.direction);
        let mut out: Vec<Advance> = Vec::new();
        let mut todo = vec![Branch {
            lane: lane.id,
            arc,
            remaining: distance.abs(),
        }];
        while let Some(Branch {
            lane,
            arc,
            remaining,
        }) = todo.pop()
        {
            let Some(lane) = self.lane(lane) else {
                continue;
            };
            let length = lane.center.length();
            let up = (lane.direction == Direction::Forward) == ahead;
            let room = f64::from(if up { length - arc } else { arc });
            let next = if ahead {
                &lane.successors
            } else {
                &lane.predecessors
            };
            if remaining > room && !next.is_empty() {
                todo.extend(next.iter().rev().filter_map(|&id| {
                    let next = self.lane(id)?;
                    let up = (next.direction == Direction::Forward) == ahead;
                    Some(Branch {
                        lane: id,
                        arc: if up { 0.0 } else { next.center.length() },
                        remaining: remaining - room,
                    })
                }));
                continue;
            }
            let moved = remaining.min(room) as f32;
            let arc = if up { arc + moved } else { arc - moved };
            let Some(at) = self.on_centerline(lane.id, arc, side * travel(lane.direction)) else {
                continue;
            };
            let end = if remaining > room {
                Advance::DeadEnd {
                    at,
                    short: remaining - room,
                }
            } else {
                Advance::Reached(at)
            };
            if !out.contains(&end) {
                out.push(end);
            }
        }
        out
    }

    /// The lane beside `at`'s on the left of its traffic, at the same `s`
    /// and at its center, or `None` if there is none. The lane of any type,
    /// running either way, so the left of a lane beside the center line is
    /// the first lane of the other side. Whether traffic may change into it
    /// is the caller's to decide.
    pub fn left_of(&self, at: LanePosition) -> Option<LanePosition> {
        self.beside(at, 1.0)
    }

    /// The lane beside `at`'s on the right of its traffic, as
    /// [`Self::left_of`] finds the left.
    pub fn right_of(&self, at: LanePosition) -> Option<LanePosition> {
        self.beside(at, -1.0)
    }

    /// The lane beside `at`'s on the `side` of its traffic, 1.0 for the left.
    fn beside(&self, at: LanePosition, side: f64) -> Option<LanePosition> {
        let lane = self.lane(at.lane)?;
        let on = self.road_lane(at.lane)?;
        let section = self.road(on.road)?.section(on.section)?;
        if !(section.start..=section.end).contains(&at.s) {
            return None;
        }
        let mut across: Vec<(i32, LaneId)> = section.lanes.clone();
        across.sort_by_key(|&(od_id, _)| std::cmp::Reverse(od_id));
        let here = across.iter().position(|&(od_id, _)| od_id == on.od_id)?;
        let there = if side * travel(lane.direction) > 0.0 {
            here.checked_sub(1)?
        } else {
            here + 1
        };
        let &(_, id) = across.get(there)?;
        Some(LanePosition {
            lane: id,
            s: at.s,
            offset: 0.0,
        })
    }

    /// The lane position `arc` metres along `lane`'s centerline, `offset`
    /// from its center toward `+t`.
    fn on_centerline(&self, lane: LaneId, arc: f32, offset: f64) -> Option<LanePosition> {
        let on = self.road_lane(lane)?;
        let section = self.road(on.road)?.section(on.section)?;
        let points = self.lane(lane)?.center.points();
        Some(LanePosition {
            lane,
            s: station_at(points, &section.stations, arc),
            offset,
        })
    }
}

/// 1.0 for a lane whose traffic runs along `+s`, and -1.0 against it.
fn travel(direction: Direction) -> f64 {
    match direction {
        Direction::Forward => 1.0,
        Direction::Backward => -1.0,
    }
}
