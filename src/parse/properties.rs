//! What holds along each baked lane: its road's `<type>`s, and its speed
//! limits from its own `<speed>`s or its road's.

use super::{along, attr_f64, BakedRoad, BakedSection, Warning};
use crate::coords::Point;
use crate::{Along, Lane, LaneId, RoadType, SpeedLimit};

/// A `<speed>` as the file gives it.
#[derive(Clone, PartialEq)]
pub(super) enum SpeedDef {
    Limit(SpeedLimit),
    Undefined,
    Unreadable { max: String, unit: String },
}

impl SpeedDef {
    /// A `<speed>`'s `max` in its `unit`. `words` allows the road's `no
    /// limit` and `undefined`, which a lane's `max` may not be.
    ///
    /// The spec says a missing unit is m/s. A `max` below 0, or a unit other
    /// than `m/s`, `km/h` or `mph`, is unreadable.
    fn parse(node: roxmltree::Node, words: bool) -> Self {
        let max = node.attribute("max").unwrap_or_default();
        match max {
            "no limit" if words => return Self::Limit(SpeedLimit::Unlimited),
            "undefined" if words => return Self::Undefined,
            _ => {}
        }
        let scale = match node.attribute("unit") {
            None | Some("m/s") => Some(1.0),
            Some("km/h") => Some(1.0 / 3.6),
            Some("mph") => Some(0.44704),
            Some(_) => None,
        };
        let value = attr_f64(node, "max").filter(|v| *v >= 0.0);
        match (value, scale) {
            (Some(v), Some(k)) => Self::Limit(SpeedLimit::Max((v * k) as f32)),
            _ => Self::Unreadable {
                max: max.to_string(),
                unit: node.attribute("unit").unwrap_or_default().to_string(),
            },
        }
    }

    fn limit(&self) -> Option<SpeedLimit> {
        match self {
            Self::Limit(limit) => Some(*limit),
            Self::Undefined | Self::Unreadable { .. } => None,
        }
    }
}

/// A road's `<type>`: what the road is from `s` on, and the speed limit it
/// sets, if it has a `<speed>`.
pub(super) struct TypeDef {
    s: f64,
    kind: RoadType,
    speed: Option<SpeedDef>,
}

/// A lane's `<speed>`: its limit from `s_offset` into its lane section on.
pub(super) struct LaneSpeedDef {
    s_offset: f64,
    speed: SpeedDef,
}

/// A road's `<type>`s, in ascending `s`.
///
/// The spec requires `s` and `type`, and says the types come in ascending
/// `s`. A type without `s` is skipped, and types out of order are sorted.
pub(super) fn road_types(road: roxmltree::Node) -> Vec<TypeDef> {
    let mut types: Vec<TypeDef> = road
        .children()
        .filter(|n| n.has_tag_name("type"))
        .filter_map(|n| {
            Some(TypeDef {
                s: attr_f64(n, "s")?,
                kind: road_type(n.attribute("type")),
                speed: n
                    .children()
                    .find(|c| c.has_tag_name("speed"))
                    .map(|c| SpeedDef::parse(c, true)),
            })
        })
        .collect();
    types.sort_by(|a, b| a.s.total_cmp(&b.s));
    types
}

/// A lane's `<speed>`s, in ascending `sOffset`.
///
/// The spec requires `sOffset` and says it is at least 0, and says the
/// entries come in ascending `sOffset`. A missing or negative one is 0, as
/// for a `<height>`, and entries out of order are sorted.
pub(super) fn lane_speeds(lane: roxmltree::Node) -> Vec<LaneSpeedDef> {
    let mut speeds: Vec<LaneSpeedDef> = lane
        .children()
        .filter(|n| n.has_tag_name("speed"))
        .map(|n| LaneSpeedDef {
            s_offset: attr_f64(n, "sOffset").unwrap_or(0.0).max(0.0),
            speed: SpeedDef::parse(n, false),
        })
        .collect();
    speeds.sort_by(|a, b| a.s_offset.total_cmp(&b.s_offset));
    speeds
}

/// The [`RoadType`] an OpenDRIVE `<type>` names. An absent or unrecognised
/// type is [`RoadType::Unknown`].
fn road_type(od_type: Option<&str>) -> RoadType {
    match od_type.unwrap_or_default() {
        "rural" => RoadType::Rural,
        "motorway" => RoadType::Motorway,
        "town" => RoadType::Town,
        "lowSpeed" => RoadType::LowSpeed,
        "pedestrian" => RoadType::Pedestrian,
        "bicycle" => RoadType::Bicycle,
        "townExpressway" => RoadType::TownExpressway,
        "townCollector" => RoadType::TownCollector,
        "townArterial" => RoadType::TownArterial,
        "townPrivate" => RoadType::TownPrivate,
        "townLocal" => RoadType::TownLocal,
        "townPlayStreet" => RoadType::TownPlayStreet,
        _ => RoadType::Unknown,
    }
}

/// The speed limits and road types along every baked lane of `roads`.
#[derive(Default)]
pub(super) struct Placed {
    pub speed_limits: Vec<Along<SpeedLimit>>,
    pub road_types: Vec<Along<RoadType>>,
}

/// Place the speed limits and road types along `lanes`, the baked lanes of
/// `roads`.
///
/// A road's type holds from its `s` to the next, or to the end of the road.
/// A lane's `<speed>` holds from its `sOffset` to the next, or to the end of
/// its lane section, and overrides the road's. Where the lane has none, the
/// speed of its road's type holds.
pub(super) fn place<'a>(roads: impl Iterator<Item = &'a BakedRoad>, lanes: &[Lane]) -> Placed {
    let mut out = Placed::default();
    for road in roads {
        let type_at = |s: f64| road.types.iter().rev().find(|t| t.s <= s + 1e-9);
        for sec in &road.sections {
            for lane in &sec.lanes {
                let Some(def) = sec.def.lane(lane.od_id) else {
                    continue;
                };
                let speeds: Vec<(f64, SpeedLimit)> = def
                    .speeds
                    .iter()
                    .filter_map(|l| Some((sec.start + l.s_offset, l.speed.limit()?)))
                    .collect();
                let own = |s: f64| speeds.iter().rev().find(|(at, _)| *at <= s + 1e-9);
                let type_starts = road.types.iter().map(|t| t.s);
                let points = lanes[lane.index].center.points();
                let kinds = stretches(sec, type_starts.clone(), |s| type_at(s).map(|t| t.kind));
                let limits = stretches(sec, type_starts.chain(speeds.iter().map(|l| l.0)), |s| {
                    own(s).map(|l| l.1).or_else(|| {
                        type_at(s)
                            .and_then(|t| t.speed.as_ref())
                            .and_then(SpeedDef::limit)
                    })
                });
                out.road_types
                    .extend(on_lane(lane.id, points, &sec.stations, kinds));
                out.speed_limits
                    .extend(on_lane(lane.id, points, &sec.stations, limits));
            }
        }
    }
    out
}

/// `stretches` of road, each `(from, to, value)` in metres along the
/// reference line, as stretches of `lane`, whose centerline `points` stand
/// at the road `stations`. One that is empty along the lane is left out.
fn on_lane<'a, T: 'a>(
    lane: LaneId,
    points: &'a [Point],
    stations: &'a [f64],
    stretches: Vec<(f64, f64, T)>,
) -> impl Iterator<Item = Along<T>> + 'a {
    stretches
        .into_iter()
        .map(move |(from, to, value)| Along {
            lane,
            from: along(points, stations, from),
            to: along(points, stations, to),
            value,
        })
        .filter(|a| a.to > a.from)
}

/// The stretches of `sec` over which `value_at` holds one value, given the
/// stations `cuts` where it may change. Neighbours with the same value
/// merge, and a stretch with no value is left out.
fn stretches<T: PartialEq>(
    sec: &BakedSection,
    cuts: impl Iterator<Item = f64>,
    value_at: impl Fn(f64) -> Option<T>,
) -> Vec<(f64, f64, T)> {
    let mut cuts: Vec<f64> = cuts
        .filter(|&s| s > sec.start + 1e-6 && s < sec.end - 1e-6)
        .chain([sec.start, sec.end])
        .collect();
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    let mut out: Vec<(f64, f64, T)> = Vec::new();
    for w in cuts.windows(2) {
        let Some(value) = value_at(w[0]) else {
            continue;
        };
        match out.last_mut() {
            Some(last) if last.1 == w[0] && last.2 == value => last.1 = w[1],
            _ => out.push((w[0], w[1], value)),
        }
    }
    out
}

impl BakedRoad {
    /// Each `<speed>` whose `max` or `unit` the crate can't read: the
    /// road's, then each lane's, section by section.
    pub(super) fn speed_warnings(&self) -> Vec<Warning> {
        let dropped = |s, lane, speed: &SpeedDef| match speed {
            SpeedDef::Unreadable { max, unit } => Some(Warning::SpeedLimitDropped {
                road_id: self.id.clone(),
                s,
                lane,
                max: max.clone(),
                unit: unit.clone(),
            }),
            SpeedDef::Limit(_) | SpeedDef::Undefined => None,
        };
        let road = self
            .types
            .iter()
            .filter_map(|t| dropped(t.s, None, t.speed.as_ref()?));
        let lanes = self.sections.iter().flat_map(|sec| {
            sec.def
                .left
                .iter()
                .chain(&sec.def.right)
                .flat_map(move |l| {
                    l.speeds
                        .iter()
                        .map(move |sp| (sec.start + sp.s_offset, l.id, &sp.speed))
                })
        });
        road.chain(lanes.filter_map(|(s, id, speed)| dropped(s, Some(id), speed)))
            .collect()
    }
}
