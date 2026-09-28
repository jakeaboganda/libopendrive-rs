//! Virtual junctions, read once every road is baked and its lanes
//! registered.

use super::links::Topology;
use super::{attr_f64, child, orientation, BakedRoad, RoadEnd, Warning};
use crate::junction::{LinkPoint, VirtualJunction, VirtualLink};
use crate::network::LaneId;
use crate::road::Road;

/// Every `<junction type="virtual">` whose main road the load has, in file
/// order, with its links, and a warning for each junction or link the crate
/// drops.
///
/// A link is a road of the junction whose `<predecessor>` or `<successor>`
/// gives an `elementS`, with the lanes its lane `<link>`s and the
/// junction's `<laneLink>`s join, or a deprecated `type="virtual"`
/// connection, with its `<laneLink>`s. One naming a road the load lacks, an
/// `s` off that road, or an `elementDir` other than `+` or `-` is dropped.
pub(super) fn place(
    root: roxmltree::Node,
    roads: &[(roxmltree::Node, BakedRoad)],
    topo: &Topology,
) -> (Vec<VirtualJunction>, Vec<Warning>) {
    let (mut out, mut warnings) = (Vec::new(), Vec::new());
    let junctions = root
        .children()
        .filter(|n| n.has_tag_name("junction") && n.attribute("type") == Some("virtual"));
    for junction in junctions {
        let jid = junction.attribute("id").unwrap_or_default();
        let main = junction
            .attribute("mainRoad")
            .and_then(|id| find(roads, id));
        let s = |name| attr_f64(junction, name).filter(|s| main.is_some_and(|r| r.on_road(*s)));
        let (Some(main), Some(s_start), Some(s_end)) = (main, s("sStart"), s("sEnd")) else {
            warnings.push(Warning::VirtualJunctionDropped {
                junction_id: jid.to_string(),
            });
            continue;
        };
        let mut links = Vec::new();
        let mut dropped = |road_id: &str, connection_id: &str| {
            warnings.push(Warning::VirtualLinkDropped {
                junction_id: jid.to_string(),
                road_id: road_id.to_string(),
                connection_id: connection_id.to_string(),
            })
        };
        for (node, baked) in roads.iter().filter(|(_, r)| r.road.junction() == Some(jid)) {
            let road = &baked.road;
            for (tag, end) in [("predecessor", RoadEnd::Start), ("successor", RoadEnd::End)] {
                let Some(link) = child(*node, "link").and_then(|l| child(l, tag)) else {
                    continue;
                };
                if link.attribute("elementS").is_none() {
                    continue;
                }
                let Some(along) = point(roads, link) else {
                    dropped(&road.od_id, "");
                    continue;
                };
                let here = LinkPoint::End { road: road.id, end };
                let section = section_of(road, topo, &here);
                let other = find(roads, link.attribute("elementId").unwrap_or_default());
                let other_section = other.map(|o| section_of(o, topo, &along));
                let mut pairs: Vec<(i32, i32)> = topo
                    .metas
                    .iter()
                    .filter(|m| m.road == road.od_id && m.section == section)
                    .filter_map(|m| {
                        let to = match end {
                            RoadEnd::Start => m.pred_link,
                            RoadEnd::End => m.succ_link,
                        }?;
                        Some((m.od_id, to))
                    })
                    .collect();
                pairs.extend(
                    junction
                        .children()
                        .filter(|c| c.has_tag_name("connection"))
                        .filter(|c| {
                            c.attribute("connectingRoad") == Some(road.od_id.as_str())
                                && c.attribute("incomingRoad") == link.attribute("elementId")
                                && (c.attribute("contactPoint") == Some("end"))
                                    == (end == RoadEnd::End)
                        })
                        .flat_map(lane_links)
                        .map(|(incoming, connecting)| (connecting, incoming)),
                );
                let mut lanes: Vec<(LaneId, LaneId)> = pairs
                    .into_iter()
                    .filter_map(|(here_od, there_od)| {
                        let here_lane = lane(topo, &road.od_id, section, here_od)?;
                        let there = other?;
                        let there_lane = lane(topo, &there.od_id, other_section?, there_od)?;
                        Some(match end {
                            RoadEnd::Start => (there_lane, here_lane),
                            RoadEnd::End => (here_lane, there_lane),
                        })
                    })
                    .collect();
                lanes.sort_by_key(|(a, b)| (a.0, b.0));
                lanes.dedup();
                let (from, to) = match end {
                    RoadEnd::Start => (along, here),
                    RoadEnd::End => (here, along),
                };
                links.push(VirtualLink { from, to, lanes });
            }
        }
        let deprecated = junction
            .children()
            .filter(|c| c.has_tag_name("connection") && c.attribute("type") == Some("virtual"));
        for connection in deprecated {
            let ends = (
                child(connection, "predecessor").and_then(|n| point(roads, n)),
                child(connection, "successor").and_then(|n| point(roads, n)),
            );
            let (Some(from), Some(to)) = ends else {
                dropped("", connection.attribute("id").unwrap_or_default());
                continue;
            };
            let side = |p: &LinkPoint| {
                let road = roads
                    .iter()
                    .map(|(_, r)| &r.road)
                    .find(|r| r.id == p.road())?;
                Some((road.od_id.as_str(), section_of(road, topo, p)))
            };
            let (Some((from_road, from_section)), Some((to_road, to_section))) =
                (side(&from), side(&to))
            else {
                continue;
            };
            let lanes = lane_links(connection)
                .into_iter()
                .filter_map(|(a, b)| {
                    Some((
                        lane(topo, from_road, from_section, a)?,
                        lane(topo, to_road, to_section, b)?,
                    ))
                })
                .collect();
            links.push(VirtualLink { from, to, lanes });
        }
        out.push(VirtualJunction {
            od_id: jid.to_string(),
            name: junction.attribute("name").unwrap_or_default().to_string(),
            main_road: main.id,
            s_start,
            s_end,
            orientation: orientation(junction),
            links,
        });
    }
    (out, warnings)
}

/// Where a `<predecessor>` or `<successor>` meets its road: part way along
/// it if it gives an `elementS`, or else the end its `contactPoint` names.
fn point(roads: &[(roxmltree::Node, BakedRoad)], link: roxmltree::Node) -> Option<LinkPoint> {
    let road = find(roads, link.attribute("elementId")?)?;
    if link.attribute("elementS").is_some() {
        let s = attr_f64(link, "elementS").filter(|s| road.on_road(*s))?;
        let forward = match link.attribute("elementDir")? {
            "+" => true,
            "-" => false,
            _ => return None,
        };
        return Some(LinkPoint::Along {
            road: road.id,
            s,
            forward,
        });
    }
    let end = match link.attribute("contactPoint") {
        Some("end") => RoadEnd::End,
        _ => RoadEnd::Start,
    };
    Some(LinkPoint::End { road: road.id, end })
}

/// The `from` and `to` of each `<laneLink>` of `connection`.
fn lane_links(connection: roxmltree::Node) -> Vec<(i32, i32)> {
    connection
        .children()
        .filter(|n| n.has_tag_name("laneLink"))
        .filter_map(|l| {
            Some((
                l.attribute("from")?.parse().ok()?,
                l.attribute("to")?.parse().ok()?,
            ))
        })
        .collect()
}

/// The index of `road`'s lane section at `point`.
fn section_of(road: &Road, topo: &Topology, point: &LinkPoint) -> usize {
    match *point {
        LinkPoint::End {
            end: RoadEnd::Start,
            ..
        } => 0,
        LinkPoint::End {
            end: RoadEnd::End, ..
        } => topo.last_section(&road.od_id),
        LinkPoint::Along { s, .. } => road.section_at(s).map_or(0, |section| section.index),
    }
}

/// The lane with OpenDRIVE id `od_id` in `road`'s lane section `section`.
fn lane(topo: &Topology, road: &str, section: usize, od_id: i32) -> Option<LaneId> {
    topo.registry
        .get(&(road.to_string(), section, od_id))
        .copied()
}

/// The baked road with `<road id>` `id`.
fn find<'a>(roads: &'a [(roxmltree::Node, BakedRoad)], id: &str) -> Option<&'a Road> {
    roads.iter().map(|(_, r)| &r.road).find(|r| r.od_id == id)
}
