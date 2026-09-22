//! OpenDRIVE connectivity: collect road/lane links and junctions while parsing,
//! then resolve them into **drive-direction** lane adjacency on the baked
//! lanes. "Successor" here means "a lane you can drive into when leaving this
//! lane's travel-direction exit end", already accounting for lane sign and the
//! link `contactPoint` (start/end). It is a routing graph, not a raw mirror
//! of the file's `+s` links.

use std::collections::HashMap;

use crate::{Direction, Lane, LaneId};

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Contact {
    Start,
    End,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ElemType {
    Road,
    Junction,
}

/// A road-level `<predecessor>`/`<successor>`: what a road connects to at one
/// end, and by which end of that neighbor.
#[derive(Clone)]
pub(crate) struct LinkTarget {
    pub elem: ElemType,
    pub id: String,
    pub contact: Contact,
}

pub(crate) struct RoadInfo {
    pub sections: usize,
    pub predecessor: Option<LinkTarget>,
    pub successor: Option<LinkTarget>,
}

/// One `<junction>` `<connection>`: an incoming road flows through a connecting
/// road, with per-lane `from -> to` links.
pub(crate) struct JunctionConn {
    pub incoming_road: String,
    pub connecting_road: String,
    pub contact: Contact,
    pub lane_links: Vec<(i32, i32)>,
}

/// Everything about one emitted lane needed to resolve its links: its assigned
/// id, where it came from (road/section/OpenDRIVE lane id), its travel
/// direction, and its lane-level `<link>` neighbor ids.
pub(crate) struct LaneMeta {
    pub id: LaneId,
    pub road: String,
    pub section: usize,
    pub od_id: i32,
    pub direction: Direction,
    pub succ_link: Option<i32>,
    pub pred_link: Option<i32>,
}

/// Accumulated during parsing; consumed by [`resolve`].
#[derive(Default)]
pub(crate) struct Topology {
    /// Next `LaneId` to hand out, so ids stay unique across all roads.
    pub next_id: usize,
    pub registry: HashMap<(String, usize, i32), LaneId>,
    pub metas: Vec<LaneMeta>,
    pub roads: HashMap<String, RoadInfo>,
    pub junctions: HashMap<String, Vec<JunctionConn>>,
}

impl Topology {
    fn last_section(&self, road: &str) -> usize {
        self.roads
            .get(road)
            .map(|r| r.sections.saturating_sub(1))
            .unwrap_or(0)
    }
}

/// Fill each lane's `successors`/`predecessors` from the collected topology.
/// Successors are computed per lane; predecessors are their inverse, so the two
/// are always consistent.
pub(crate) fn resolve(lanes: &mut [Lane], topo: &Topology) {
    let mut succ: HashMap<LaneId, Vec<LaneId>> = HashMap::new();
    for meta in &topo.metas {
        let s = successors_of(meta, topo);
        if !s.is_empty() {
            succ.insert(meta.id, s);
        }
    }
    let mut pred: HashMap<LaneId, Vec<LaneId>> = HashMap::new();
    for (from, tos) in &succ {
        for to in tos {
            pred.entry(*to).or_default().push(*from);
        }
    }
    for lane in lanes.iter_mut() {
        if let Some(s) = succ.get(&lane.id) {
            lane.successors = s.clone();
            lane.successors.sort_by_key(|l| l.0);
        }
        if let Some(p) = pred.get(&lane.id) {
            lane.predecessors = p.clone();
            lane.predecessors.sort_by_key(|l| l.0);
        }
    }
}

/// The lanes reachable by driving off `meta`'s exit end. A forward lane exits at
/// the road's `+s` end (next section, or the road's successor); a backward lane
/// exits at the `-s` (start) end (previous section, or the predecessor).
fn successors_of(meta: &LaneMeta, topo: &Topology) -> Vec<LaneId> {
    let last = topo.last_section(&meta.road);
    match meta.direction {
        Direction::Forward => {
            if meta.section < last {
                lookup(topo, &meta.road, meta.section + 1, meta.succ_link)
            } else {
                let target = topo.roads.get(&meta.road).and_then(|r| r.successor.clone());
                cross(topo, meta, target, meta.succ_link)
            }
        }
        Direction::Backward => {
            if meta.section > 0 {
                lookup(topo, &meta.road, meta.section - 1, meta.pred_link)
            } else {
                let target = topo
                    .roads
                    .get(&meta.road)
                    .and_then(|r| r.predecessor.clone());
                cross(topo, meta, target, meta.pred_link)
            }
        }
    }
}

/// The lane with OpenDRIVE id `od` in (`road`, `section`), if it exists.
fn lookup(topo: &Topology, road: &str, section: usize, od: Option<i32>) -> Vec<LaneId> {
    od.and_then(|o| topo.registry.get(&(road.to_string(), section, o)).copied())
        .into_iter()
        .collect()
}

/// Resolve a cross-road boundary: follow the road's link target (another road,
/// or a junction) to the lane(s) it leads into. `lane_link` is this lane's own
/// `<link>` neighbor id (used for a direct road link).
fn cross(
    topo: &Topology,
    meta: &LaneMeta,
    target: Option<LinkTarget>,
    lane_link: Option<i32>,
) -> Vec<LaneId> {
    let Some(target) = target else {
        return Vec::new();
    };
    match target.elem {
        // Direct road-to-road: the neighbor's section is its first (start
        // contact) or last (end contact); the lane is our own <link> id.
        ElemType::Road => {
            let section = match target.contact {
                Contact::Start => 0,
                Contact::End => topo.last_section(&target.id),
            };
            lookup(topo, &target.id, section, lane_link)
        }
        // Through a junction: each connection out of our road maps our lane id
        // to a connecting-road lane via its laneLinks. May fan out.
        ElemType::Junction => {
            let Some(conns) = topo.junctions.get(&target.id) else {
                return Vec::new();
            };
            let mut out = Vec::new();
            for c in conns.iter().filter(|c| c.incoming_road == meta.road) {
                let section = match c.contact {
                    Contact::Start => 0,
                    Contact::End => topo.last_section(&c.connecting_road),
                };
                for (_, to) in c.lane_links.iter().filter(|(f, _)| *f == meta.od_id) {
                    out.extend(lookup(topo, &c.connecting_road, section, Some(*to)));
                }
            }
            out
        }
    }
}

// --- Parsing (roxmltree -> the types above) ----------------------------------

fn contact_of(node: roxmltree::Node) -> Contact {
    match node.attribute("contactPoint") {
        Some("end") => Contact::End,
        _ => Contact::Start,
    }
}

/// Parse a road's `<link>` predecessor/successor targets.
pub(crate) fn road_link(road: roxmltree::Node) -> (Option<LinkTarget>, Option<LinkTarget>) {
    let Some(link) = road.children().find(|n| n.has_tag_name("link")) else {
        return (None, None);
    };
    let target = |tag: &str| {
        link.children().find(|n| n.has_tag_name(tag)).and_then(|n| {
            let elem = match n.attribute("elementType") {
                Some("junction") => ElemType::Junction,
                _ => ElemType::Road,
            };
            Some(LinkTarget {
                elem,
                id: n.attribute("elementId")?.to_string(),
                contact: contact_of(n),
            })
        })
    };
    (target("predecessor"), target("successor"))
}

/// A lane's `<link>` predecessor/successor lane ids.
pub(crate) fn lane_link(lane: roxmltree::Node) -> (Option<i32>, Option<i32>) {
    let Some(link) = lane.children().find(|n| n.has_tag_name("link")) else {
        return (None, None);
    };
    let id = |tag: &str| {
        link.children()
            .find(|n| n.has_tag_name(tag))
            .and_then(|n| n.attribute("id"))
            .and_then(|s| s.parse().ok())
    };
    (id("predecessor"), id("successor"))
}

/// Parse every `<junction>` in the document into `id -> connections`.
pub(crate) fn junctions(root: roxmltree::Node) -> HashMap<String, Vec<JunctionConn>> {
    let mut out = HashMap::new();
    for j in root.children().filter(|n| n.has_tag_name("junction")) {
        let Some(jid) = j.attribute("id") else {
            continue;
        };
        let conns = j
            .children()
            .filter(|n| n.has_tag_name("connection"))
            .filter_map(|c| {
                Some(JunctionConn {
                    incoming_road: c.attribute("incomingRoad")?.to_string(),
                    connecting_road: c.attribute("connectingRoad")?.to_string(),
                    contact: contact_of(c),
                    lane_links: c
                        .children()
                        .filter(|n| n.has_tag_name("laneLink"))
                        .filter_map(|l| {
                            Some((
                                l.attribute("from")?.parse().ok()?,
                                l.attribute("to")?.parse().ok()?,
                            ))
                        })
                        .collect(),
                })
            })
            .collect();
        out.insert(jid.to_string(), conns);
    }
    out
}
