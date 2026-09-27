//! Signals, placed once every road is baked.

use std::f64::consts::PI;

use super::{attr_f64, child, orientation, validity, BakedRoad, Orientation, SignalProvenance};
use crate::coords::Vector;
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
    let mut out = Signals::default();
    for (node, road) in roads {
        let Some(signals) = child(*node, "signals") else {
            continue;
        };
        for signal in signals.children().filter(|n| n.has_tag_name("signal")) {
            place_signal(signal, road, &mut out);
        }
    }
    out
}

/// Bake one `<signal>` on `road` into `out`. One missing `s` or `t`, or off
/// the ends of the road, is skipped.
///
/// The board stands `zOffset` straight above the road surface at `(s, t)`.
/// It faces the traffic its `orientation` names, against `+s` for `+` and
/// along it otherwise, turned `hOffset` counter-clockwise. `pitch` and
/// `roll` are against the horizontal, not the road, so a board on a banked
/// road stays upright.
fn place_signal(node: roxmltree::Node, road: &BakedRoad, out: &mut Signals) {
    let (Some(s), Some(t)) = (attr_f64(node, "s"), attr_f64(node, "t")) else {
        return;
    };
    if !road.on_road(s) {
        return;
    }
    let orientation = orientation(node);
    let number = |name| attr_f64(node, name).unwrap_or(0.0);
    let (ground, road_heading) = road.surface(s, t);
    let facing = match orientation {
        Orientation::Positive => PI,
        Orientation::Negative | Orientation::Both => 0.0,
    };
    let raise = Vector::new(0.0, 0.0, number("zOffset") as f32);
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
        applies_at: vec![ground],
        position: ground + raise,
        heading: wrap(road_heading + facing + number("hOffset")) as f32,
        pitch: wrap(number("pitch")) as f32,
        roll: wrap(number("roll")) as f32,
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
