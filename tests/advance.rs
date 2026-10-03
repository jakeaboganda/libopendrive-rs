//! Moving along the lanes: `RoadNetwork::advance`, `left_of` and `right_of`.

use xodr::{load_file, load_str, Advance, Direction, LaneId, LanePosition, RoadNetwork};

/// The baked lane `od_id` of the first section of the road `road`.
fn lane(net: &RoadNetwork, road: &str, od_id: i32) -> LaneId {
    let road = net.road_by_od_id(road).expect("the road").id();
    net.lanes()
        .iter()
        .map(|l| l.id)
        .find(|&id| {
            let at = net.road_lane(id).unwrap();
            at.road == road && at.od_id == od_id
        })
        .unwrap_or_else(|| panic!("lane {od_id}"))
}

fn at(lane: LaneId, s: f64, offset: f64) -> LanePosition {
    LanePosition { lane, s, offset }
}

/// Each branch's end, rounded to the millimetre.
fn ends(branches: &[Advance]) -> Vec<(LaneId, f64, f64, Option<f64>)> {
    let mm = |v: f64| (v * 1000.0).round() / 1000.0;
    branches
        .iter()
        .map(|b| {
            let short = match b {
                Advance::Reached(_) => None,
                Advance::DeadEnd { short, .. } => Some(mm(*short)),
            };
            let p = b.position();
            (p.lane, mm(p.s), mm(p.offset), short)
        })
        .collect()
}

/// Road 1 runs 50 m along +X. Road 2 runs back from (100, 0) to its end at
/// (50, 0), so the two meet end to end, facing each other.
const FACING: &str = r#"<OpenDRIVE>
  <road id="1" length="50" junction="-1">
    <link><successor elementType="road" elementId="2" contactPoint="end"/></link>
    <planView><geometry s="0" x="0" y="0" hdg="0" length="50"><line/></geometry></planView>
    <lanes><laneSection s="0">
      <left><lane id="1" type="driving"><link><successor id="-1"/></link><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane></left>
      <center><lane id="0" type="none"/></center>
      <right>
        <lane id="-1" type="driving"><link><successor id="1"/></link><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane>
        <lane id="-2" type="sidewalk"><width sOffset="0" a="2" b="0" c="0" d="0"/></lane>
      </right>
    </laneSection></lanes>
  </road>
  <road id="2" length="50" junction="-1">
    <link><successor elementType="road" elementId="1" contactPoint="end"/></link>
    <planView><geometry s="0" x="100" y="0" hdg="3.141592653589793" length="50"><line/></geometry></planView>
    <lanes><laneSection s="0">
      <left><lane id="1" type="driving"><link><successor id="-1"/></link><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane></left>
      <center><lane id="0" type="none"/></center>
      <right><lane id="-1" type="driving"><link><successor id="1"/></link><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane></right>
    </laneSection></lanes>
  </road>
</OpenDRIVE>"#;

#[test]
fn a_position_moves_along_its_lane_the_way_its_traffic_runs() {
    let net = load_str(FACING).unwrap();
    let (forward, backward) = (lane(&net, "1", -1), lane(&net, "1", 1));
    assert_eq!(
        ends(&net.advance(at(forward, 10.0, 0.0), 15.0)),
        [(forward, 25.0, 0.0, None)]
    );
    assert_eq!(
        ends(&net.advance(at(backward, 30.0, 0.0), 15.0)),
        [(backward, 15.0, 0.0, None)]
    );
    // A negative distance goes back.
    assert_eq!(
        ends(&net.advance(at(forward, 10.0, 0.0), -4.0)),
        [(forward, 6.0, 0.0, None)]
    );
}

#[test]
fn onto_a_road_running_the_other_way_the_offset_keeps_its_side() {
    let net = load_str(FACING).unwrap();
    let (from, into) = (lane(&net, "1", -1), lane(&net, "2", 1));
    // 0.5 m toward road 1's +t is left of the traffic, and so is 0.5 m
    // toward road 2's -t.
    assert_eq!(
        ends(&net.advance(at(from, 45.0, 0.5), 10.0)),
        [(into, 45.0, -0.5, None)]
    );
    // And back again.
    assert_eq!(
        ends(&net.advance(at(into, 45.0, -0.5), -10.0)),
        [(from, 45.0, 0.5, None)]
    );
}

#[test]
fn a_branch_stops_where_its_lane_ends_and_says_how_far_short() {
    let net = load_str(FACING).unwrap();
    let sidewalk = lane(&net, "1", -2);
    assert_eq!(
        ends(&net.advance(at(sidewalk, 45.0, 0.0), 12.0)),
        [(sidewalk, 50.0, 0.0, Some(7.0))]
    );
    assert_eq!(
        ends(&net.advance(at(sidewalk, 5.0, 0.0), -8.0)),
        [(sidewalk, 0.0, 0.0, Some(3.0))]
    );
}

#[test]
fn a_fork_gives_one_branch_per_lane_it_leads_to() {
    let net = load_file("tests/data/town07.xodr").unwrap();
    let mut forks = 0;
    for lane in net.lanes().iter().filter(|l| l.successors.len() > 1) {
        let points = lane.center.points();
        let exit = match lane.direction {
            Direction::Forward => points[points.len() - 1],
            Direction::Backward => points[0],
            Direction::Both => continue,
        };
        let road = net.road_lane(lane.id).unwrap().road;
        let s = net.road_position_on(road, exit).unwrap().s;
        // Some junction lanes start with a section 2.5 cm long.
        let step = lane
            .successors
            .iter()
            .map(|&id| f64::from(net.lane(id).unwrap().center.length()))
            .fold(f64::INFINITY, f64::min)
            / 2.0;
        let branches = net.advance(at(lane.id, s, 0.0), step);
        let mut reached: Vec<LaneId> = branches.iter().map(|b| b.position().lane).collect();
        reached.sort_by_key(|l| l.0);
        let mut want = lane.successors.clone();
        want.sort_by_key(|l| l.0);
        want.dedup();
        assert_eq!(reached, want, "lane {:?}", lane.id);
        forks += 1;
    }
    assert!(forks > 50, "only {forks} forks");
}

#[test]
fn a_direct_junction_is_crossed_both_ways() {
    let net = load_file("tests/data/direct_junctions.xodr").unwrap();
    assert_eq!(
        ends(&net.advance(at(lane(&net, "0", -2), 45.0, 0.0), 10.0)),
        [(lane(&net, "2", -1), 5.0, 0.0, None)]
    );
    // Junction 8 gives no way back, and the crate reads one.
    assert_eq!(
        ends(&net.advance(at(lane(&net, "1", 1), 5.0, 0.0), 10.0)),
        [(lane(&net, "0", 1), 45.0, 0.0, None)]
    );
}

#[test]
fn under_left_hand_traffic_the_left_lanes_run_along_s() {
    let net = load_file("tests/data/traffic_rule.xodr").unwrap();
    assert_eq!(
        ends(&net.advance(at(lane(&net, "2", 1), 45.0, 0.0), 10.0)),
        [(lane(&net, "3", 1), 5.0, 0.0, None)]
    );
    assert_eq!(
        ends(&net.advance(at(lane(&net, "3", -1), 5.0, 0.0), 10.0)),
        [(lane(&net, "2", -1), 45.0, 0.0, None)]
    );
}

#[test]
fn the_distance_is_along_the_lane_not_the_road() {
    // Road 3 is a 25 m arc curving left, and lane -1's center 1.75 m outside
    // it, so 20 m along the lane is 20 * 25 / 26.75 m along the road.
    let net = load_file("tests/data/lane_heights.xodr").unwrap();
    let lane = lane(&net, "3", -1);
    let start = at(lane, 5.0, 0.0);
    let [Advance::Reached(end)] = net.advance(start, 20.0)[..] else {
        panic!("one branch");
    };
    let travelled = net.centerline_s(end).unwrap() - net.centerline_s(start).unwrap();
    assert!((travelled - 20.0).abs() < 1e-3, "{travelled}");
    assert!((end.s - 5.0 - 20.0 * 25.0 / 26.75).abs() < 0.01, "{end:?}");
}

#[test]
fn the_lanes_beside_are_left_and_right_of_the_traffic() {
    let net = load_str(FACING).unwrap();
    let (one, minus_one, minus_two) =
        (lane(&net, "1", 1), lane(&net, "1", -1), lane(&net, "1", -2));
    let beside = |lane, left: bool| {
        let from = at(lane, 20.0, 0.3);
        if left {
            net.left_of(from)
        } else {
            net.right_of(from)
        }
        .map(|p| (p.lane, p.s, p.offset))
    };
    // Lane -1 runs along +X: its left is lane 1, its right the sidewalk.
    assert_eq!(beside(minus_one, true), Some((one, 20.0, 0.0)));
    assert_eq!(beside(minus_one, false), Some((minus_two, 20.0, 0.0)));
    // Lane 1 runs along -X: its left is lane -1, and nothing on its right.
    assert_eq!(beside(one, true), Some((minus_one, 20.0, 0.0)));
    assert_eq!(beside(one, false), None);
    assert_eq!(beside(minus_two, false), None);
    assert!(net.left_of(at(one, 50.5, 0.0)).is_none());
}

#[test]
fn a_position_off_the_network_goes_nowhere() {
    let net = load_str(FACING).unwrap();
    assert!(net.advance(at(LaneId(99), 1.0, 0.0), 5.0).is_empty());
    assert!(net
        .advance(at(lane(&net, "1", 1), 60.0, 0.0), 5.0)
        .is_empty());
}

/// Road 1's lane -1 leads back into its own start, a loop with no end.
const LOOP: &str = r#"<OpenDRIVE>
  <road id="1" length="50" junction="-1">
    <link><successor elementType="road" elementId="1" contactPoint="start"/></link>
    <planView><geometry s="0" x="0" y="0" hdg="0" length="50"><line/></geometry></planView>
    <lanes><laneSection s="0">
      <center><lane id="0" type="none"/></center>
      <right><lane id="-1" type="driving"><link><successor id="-1"/></link><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane></right>
    </laneSection></lanes>
  </road>
</OpenDRIVE>"#;

#[test]
fn an_infinite_or_nan_distance_gives_no_places_even_round_a_loop() {
    let net = load_str(LOOP).unwrap();
    let from = at(net.lanes()[0].id, 10.0, 0.0);
    assert_eq!(net.lanes()[0].successors, [net.lanes()[0].id]);
    assert!(net.advance(from, f64::INFINITY).is_empty());
    assert!(net.advance(from, f64::NEG_INFINITY).is_empty());
    assert!(net.advance(from, f64::NAN).is_empty());
    assert_eq!(net.advance(from, 1000.0).len(), 1);
}
