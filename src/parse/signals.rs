//! Signals, placed once every road is baked.

use std::collections::HashMap;
use std::f64::consts::PI;

use super::{attr_f64, child, orientation, validity, BakedRoad, Orientation, SignalProvenance};
use crate::coords::{Point, Vector};
use crate::{Signal, SignalId, Unit};

/// The signals baked so far, and the provenance of each, in step.
#[derive(Default)]
pub(super) struct Signals {
    pub baked: Vec<Signal>,
    pub provenance: Vec<SignalProvenance>,
}

/// Bake every `<signal>` of every road. `roads` pairs each baked road with
/// its `<road>`.
pub(super) fn place(roads: &[(roxmltree::Node, BakedRoad)]) -> Signals {
    let mut by_id = HashMap::new();
    for (_, road) in roads {
        by_id.entry(road.id.as_str()).or_insert(road);
    }
    let mut out = Signals::default();
    for (node, road) in roads {
        let Some(signals) = child(*node, "signals") else {
            continue;
        };
        for signal in signals.children().filter(|n| n.has_tag_name("signal")) {
            place_signal(signal, road, &by_id, &mut out);
        }
    }
    out
}

/// Where a signal's board stands, and how it is turned.
struct Board {
    position: Point,
    heading: f64,
    pitch: f64,
    roll: f64,
}

/// Bake one `<signal>` on `road` into `out`. `roads` finds the road a
/// `<positionRoad>` names. One missing `s` or `t`, or off the ends of the
/// road, is skipped.
fn place_signal(
    node: roxmltree::Node,
    road: &BakedRoad,
    roads: &HashMap<&str, &BakedRoad>,
    out: &mut Signals,
) {
    let (Some(s), Some(t)) = (attr_f64(node, "s"), attr_f64(node, "t")) else {
        return;
    };
    let orientation = orientation(node);
    let Some(own) = standing(node, road, orientation) else {
        return;
    };
    let elsewhere = || {
        let at = child(node, "positionRoad")?;
        standing(at, roads.get(at.attribute("roadId")?)?, orientation)
    };
    let board = child(node, "positionInertial")
        .and_then(inertial)
        .or_else(elsewhere)
        .unwrap_or(own);
    let text = |name| node.attribute(name).unwrap_or_default().to_string();
    let flag = |name| matches!(node.attribute(name), Some("true" | "1"));
    let size = |name| attr_f64(node, name).map(|v| v as f32);
    let id = SignalId(out.baked.len());
    out.baked.push(Signal {
        id,
        name: text("name"),
        dynamic: node.attribute("dynamic") == Some("yes"),
        country: text("country"),
        country_revision: text("countryRevision"),
        kind: text("type"),
        subtype: text("subtype"),
        value: attr_f64(node, "value"),
        unit: node.attribute("unit").and_then(unit),
        text: text("text"),
        invalidated: flag("invalidated"),
        temporary: flag("temporary"),
        lanes: road.lanes((s, s), &lane_ranges(orientation, validity(node))),
        applies_at: vec![road.surface(s, t).0],
        position: board.position,
        heading: wrap(board.heading) as f32,
        pitch: wrap(board.pitch) as f32,
        roll: wrap(board.roll) as f32,
        length: size("length"),
        width: size("width"),
        height: size("height"),
    });
    out.provenance.push(SignalProvenance {
        signal: id,
        road_id: road.id.clone(),
        od_id: text("id"),
        s,
        t,
        orientation,
    });
}

/// A board standing `zOffset` straight above `road`'s surface at the `s` and
/// `t` of `node`, a `<signal>` or a `<positionRoad>`, or `None` if it is
/// missing either or off the ends of the road.
///
/// The board faces the traffic `orientation` names, against `+s` for `+`
/// and along it otherwise, turned `hOffset` counter-clockwise. `pitch` and
/// `roll` are against the horizontal, not the road, so a board on a banked
/// road stays upright.
fn standing(node: roxmltree::Node, road: &BakedRoad, orientation: Orientation) -> Option<Board> {
    let (s, t) = (attr_f64(node, "s")?, attr_f64(node, "t")?);
    if !road.on_road(s) {
        return None;
    }
    let number = |name| attr_f64(node, name).unwrap_or(0.0);
    let (ground, road_heading) = road.surface(s, t);
    let facing = match orientation {
        Orientation::Positive => PI,
        Orientation::Negative | Orientation::Both => 0.0,
    };
    Some(Board {
        position: ground + Vector::new(0.0, 0.0, number("zOffset") as f32),
        heading: road_heading + facing + number("hOffset"),
        pitch: number("pitch"),
        roll: number("roll"),
    })
}

/// A board at a `<positionInertial>`, facing its `hdg`, or `None` if it is
/// missing `x`, `y` or `z`.
fn inertial(node: roxmltree::Node) -> Option<Board> {
    let number = |name| attr_f64(node, name).unwrap_or(0.0);
    let (x, y, z) = (
        attr_f64(node, "x")?,
        attr_f64(node, "y")?,
        attr_f64(node, "z")?,
    );
    Some(Board {
        position: Point::new(x as f32, y as f32, z as f32),
        heading: number("hdg"),
        pitch: number("pitch"),
        roll: number("roll"),
    })
}

/// The `<lane id>` ranges a signal applies to: its `<validity>` ranges if it
/// has any, else the side of the road whose traffic its `orientation` names.
fn lane_ranges(orientation: Orientation, validity: Vec<(i32, i32)>) -> Vec<(i32, i32)> {
    if !validity.is_empty() {
        return validity;
    }
    match orientation {
        Orientation::Positive => vec![(i32::MIN, -1)],
        Orientation::Negative => vec![(1, i32::MAX)],
        Orientation::Both => Vec::new(),
    }
}

/// `angle` in `(-π, π]`.
fn wrap(angle: f64) -> f64 {
    let wrapped = angle.rem_euclid(2.0 * PI);
    if wrapped > PI {
        wrapped - 2.0 * PI
    } else {
        wrapped
    }
}

/// The unit a `unit` attribute names. `None` for an empty one.
fn unit(name: &str) -> Option<Unit> {
    Some(match name {
        "" => return None,
        "m" => Unit::Metre,
        "km" => Unit::Kilometre,
        "ft" => Unit::Foot,
        "mile" => Unit::Mile,
        "m/s" => Unit::MetrePerSecond,
        "mph" => Unit::MilePerHour,
        "km/h" => Unit::KilometrePerHour,
        "kg" => Unit::Kilogram,
        "t" => Unit::Tonne,
        "%" => Unit::Percent,
        _ => Unit::Unknown,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load_str;

    /// A flat 20 m road heading +X with lane -1, carrying `signals`.
    fn road_with(signals: &str) -> String {
        format!(
            r#"<OpenDRIVE>
  <road name="r" length="20.0" id="1" junction="-1">
    <planView><geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry></planView>
    <lanes><laneSection s="0.0"><right><lane id="-1" type="driving">
      <width sOffset="0.0" a="3.5"/>
    </lane></right></laneSection></lanes>
    <signals>{signals}</signals>
  </road>
</OpenDRIVE>"#
        )
    }

    fn signals_of(signals: &str) -> Vec<Signal> {
        load_str(&road_with(signals))
            .expect("import")
            .signals()
            .to_vec()
    }

    #[test]
    fn a_signal_without_a_station_or_off_the_road_is_skipped() {
        let signals = signals_of(
            r#"<signal id="a" t="0" orientation="+"/>
               <signal id="b" s="5" orientation="+"/>
               <signal id="c" s="25" t="0" orientation="+"/>
               <signal id="d" s="20" t="0" orientation="+"/>"#,
        );
        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0].position, crate::Point::new(20.0, 0.0, 0.0));
    }

    #[test]
    fn a_position_that_cannot_be_placed_leaves_the_board_at_its_station() {
        let signals = signals_of(
            r#"<signal s="5" t="-2" zOffset="1" orientation="-">
                 <positionRoad roadId="9" s="1" t="0" zOffset="0" hOffset="0"/>
               </signal>
               <signal s="5" t="-2" zOffset="1" orientation="-">
                 <positionInertial x="1" y="2" hdg="0"/>
               </signal>
               <signal s="5" t="-2" zOffset="1" orientation="-">
                 <positionRoad roadId="1" s="30" t="0" zOffset="0" hOffset="0"/>
               </signal>"#,
        );
        for signal in &signals {
            assert_eq!(signal.position, crate::Point::new(5.0, -2.0, 1.0));
        }
        assert_eq!(signals.len(), 3);
    }

    #[test]
    fn an_unrecognised_unit_is_unknown_and_an_empty_one_is_none() {
        let signals = signals_of(
            r#"<signal s="1" t="0" value="3" unit="furlong"/>
               <signal s="1" t="0" value="3" unit=""/>
               <signal s="1" t="0" value="x" unit="mph"/>"#,
        );
        let units: Vec<_> = signals.iter().map(|s| (s.value, s.unit)).collect();
        assert_eq!(
            units,
            [
                (Some(3.0), Some(Unit::Unknown)),
                (Some(3.0), None),
                (None, Some(Unit::MilePerHour)),
            ]
        );
    }

    #[test]
    fn an_angle_wraps_into_the_half_open_turn() {
        for (angle, want) in [
            (0.0, 0.0),
            (PI, PI),
            (-PI, PI),
            (3.0 * PI / 2.0, -PI / 2.0),
            (14.0 * PI + 0.1, 0.1),
            (-14.0 * PI - 0.1, -0.1),
        ] {
            assert!(
                (wrap(angle) - want).abs() < 1e-9,
                "{angle} wrapped to {}",
                wrap(angle)
            );
        }
    }
}
