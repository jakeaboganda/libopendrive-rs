//! The OpenDRIVE parser: `.xodr` XML in, a baked [`RoadNetwork`] out.
//!
//! Reference geometry (`line`, `arc`, `spiral`, `paramPoly3`, `poly3`) is
//! evaluated to points at a fixed arc-length step, elevation and
//! superelevation profiles are applied per sample, and lane connectivity is
//! resolved in [`links`] once every lane has an id.
//!
//! Geometry is cross-checked against the reference C++
//! [libOpenDRIVE](https://github.com/pageldev/libOpenDRIVE).

use std::collections::HashMap;

use crate::coords::{Point, Vector};
use crate::object::orient;
use crate::{
    Border, Corner, Coverage, Direction, Extent, Lane, LaneId, LaneType, Marking, Material, Object,
    ObjectId, ObjectType, ParkingSpace, Polyline, RoadNetwork, Section, Shape, Structure,
    StructureId, StructureKind, UserData,
};

mod links;
use links::{LaneMeta, RoadInfo, Topology};

/// Arc-length spacing (meters) at which curved geometry is baked to points.
const SAMPLE_STEP: f64 = 2.0;

/// The most objects one `<repeat>` may expand to, and the most dashes one
/// `<marking>` may paint. A tiny `distance` over a long `length` would
/// otherwise ask for billions of them. Real rows are far
/// shorter: esmini's e6mini, a 1.5 km highway lined with posts every 4 m,
/// repeats 367 at most.
const MAX_REPEAT_INSTANCES: f64 = 100_000.0;

/// Why an OpenDRIVE document did not import.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// The document is not well-formed XML.
    #[error("invalid OpenDRIVE XML: {0}")]
    Xml(#[from] roxmltree::Error),
    /// The XML parsed, but it is not a usable map. Either it was unreadable
    /// from disk, or it carries no lanes at all.
    #[error("malformed OpenDRIVE: {0}")]
    Malformed(String),
}

/// The OpenDRIVE identity of one baked lane: which road, lane section, and
/// original `<lane id>` it was tessellated from.
///
/// A baked [`Lane`] is format-agnostic and carries only an opaque [`LaneId`];
/// this is the OpenDRIVE-specific provenance, kept in a separate table so
/// [`Lane`] stays format-neutral. Obtain it from [`load_str_with_provenance`]
/// or [`load_file_with_provenance`], and look a lane up by its [`LaneId`]
/// rather than by position.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LaneProvenance {
    /// The baked lane this record describes.
    pub lane: LaneId,
    /// The `<road id>` attribute the lane came from.
    pub road_id: String,
    /// The zero-based lane-section index within that road (sections ordered by
    /// `s`).
    pub section: usize,
    /// The original OpenDRIVE `<lane id>` (signed; negative right of the
    /// reference line, positive left).
    pub od_id: i32,
}

/// The OpenDRIVE identity of one baked object: which road and `<object>` it
/// came from, and where on that road it is anchored.
///
/// Like [`LaneProvenance`], this is kept apart from [`Object`] so the baked
/// object stays format-neutral. Everything here is relative to an OpenDRIVE
/// road.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ObjectProvenance {
    /// The baked object this record describes.
    pub object: ObjectId,
    /// The `<road id>` the object is on.
    pub road_id: String,
    /// The `<object id>` it came from. Every object a `<repeat>` or several
    /// outlines bake from one `<object>` shares it. Empty if the file gives
    /// none. For an object an `<objectReference>` placed, this is the id the
    /// reference names.
    pub od_id: String,
    /// Where on the road the object is anchored, in metres along the
    /// reference line: a solid's own station, the start of the part of a
    /// sweep on the road, or the `<object>`'s station for an outline.
    pub s: f64,
    /// The lateral offset from the reference line at that station, in metres,
    /// positive to the left.
    pub t: f64,
    /// Which direction of the road the object applies to.
    pub orientation: Orientation,
    /// How far along the road the object is valid from `s`, in metres, if
    /// the file says. Meant for objects such as a speed bump that act over a
    /// stretch of road.
    pub valid_length: Option<f64>,
    /// `Some` if an `<objectReference>` on `road_id` placed the object: the
    /// road its `<object>` is on. `s`, `t`, `orientation` and `valid_length`
    /// are then the reference's, and the shape is the `<object>`'s, moved to
    /// the reference's station.
    pub referenced_from: Option<String>,
}

/// Which direction of its road an object applies to, from its
/// `orientation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Orientation {
    /// Traffic along +s, `orientation="+"`.
    Positive,
    /// Traffic along -s, `orientation="-"`.
    Negative,
    /// Both, `orientation="none"`. Also what a missing or unrecognised
    /// value reads as.
    Both,
}

/// The OpenDRIVE identity of one baked tunnel or bridge: which road and
/// `<tunnel>` or `<bridge>` it came from, and the stretch of road it spans.
///
/// Kept apart from [`Structure`] as [`ObjectProvenance`] is from [`Object`].
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StructureProvenance {
    /// The baked structure this record describes.
    pub structure: StructureId,
    /// The `<road id>` it is on.
    pub road_id: String,
    /// The `<tunnel id>` or `<bridge id>` it came from. Empty if the file
    /// gives none.
    pub od_id: String,
    /// Where it starts, in metres along the road's reference line.
    pub s: f64,
    /// How far along the road it runs from `s`, in metres.
    pub length: f64,
}

/// The OpenDRIVE identity of everything a load baked, from
/// [`load_str_with_provenance`] or [`load_file_with_provenance`].
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Provenance {
    /// One per baked lane, in baked-lane order.
    pub lanes: Vec<LaneProvenance>,
    /// One per baked object, in baked-object order.
    pub objects: Vec<ObjectProvenance>,
    /// One per baked tunnel or bridge, in baked-structure order.
    pub structures: Vec<StructureProvenance>,
}

/// Load an OpenDRIVE file from disk and bake it into a `RoadNetwork`.
pub fn load_file(path: impl AsRef<std::path::Path>) -> Result<RoadNetwork, ImportError> {
    load_file_with_provenance(path).map(|(net, _)| net)
}

/// Bake an OpenDRIVE document (as a string) into a `RoadNetwork`.
pub fn load_str(xml: &str) -> Result<RoadNetwork, ImportError> {
    load_str_with_provenance(xml).map(|(net, _)| net)
}

/// Like [`load_file`], but also returns the OpenDRIVE [`Provenance`] of
/// every baked lane, object and structure, for a viewer or editor that must name the
/// road and original id each one came from.
pub fn load_file_with_provenance(
    path: impl AsRef<std::path::Path>,
) -> Result<(RoadNetwork, Provenance), ImportError> {
    let xml = std::fs::read_to_string(path.as_ref())
        .map_err(|e| ImportError::Malformed(format!("reading {:?}: {e}", path.as_ref())))?;
    load_str_with_provenance(&xml)
}

/// Like [`load_str`], but also returns the OpenDRIVE [`Provenance`] of
/// every baked lane, object and structure. Each record carries the
/// [`LaneId`], [`ObjectId`] or [`StructureId`] it describes.
pub fn load_str_with_provenance(xml: &str) -> Result<(RoadNetwork, Provenance), ImportError> {
    let cleaned = sanitize(xml);
    let doc = roxmltree::Document::parse(cleaned.as_ref())?;
    let root = doc.root_element();
    let mut lanes = Vec::new();
    let mut objects = Objects::default();
    let mut structures = Structures::default();
    let mut topo = Topology::default();
    let index = object_index(root);
    for road in root.children().filter(|n| n.has_tag_name("road")) {
        parse_road(
            road,
            &index,
            &mut lanes,
            &mut objects,
            &mut structures,
            &mut topo,
        );
    }
    if lanes.is_empty() {
        return Err(ImportError::Malformed("no lanes found".into()));
    }
    // Resolve connectivity once all lanes exist and are registered.
    topo.junctions = links::junctions(root);
    links::resolve(&mut lanes, &topo);
    // The parser already recorded each lane's OpenDRIVE origin while baking;
    // surface it rather than reconstructing lane-id order downstream.
    let provenance = Provenance {
        lanes: topo
            .metas
            .iter()
            .map(|m| LaneProvenance {
                lane: m.id,
                road_id: m.road.clone(),
                section: m.section,
                od_id: m.od_id,
            })
            .collect(),
        objects: objects.provenance,
        structures: structures.provenance,
    };
    Ok((
        RoadNetwork::new(lanes)
            .with_objects(objects.baked)
            .with_structures(structures.baked),
        provenance,
    ))
}

/// Make a real-world document parseable: strip a UTF-8 BOM and remove the
/// `<?xml ... ?>` declaration. Tools (e.g. CARLA) emit a license comment
/// *before* the declaration, which is malformed XML that strict parsers reject;
/// the declaration only names version/encoding, which we don't need for UTF-8.
fn sanitize(xml: &str) -> std::borrow::Cow<'_, str> {
    let xml = xml.trim_start_matches('\u{feff}');
    if let Some(start) = xml.find("<?xml") {
        if let Some(rel_end) = xml[start..].find("?>") {
            let mut out = String::with_capacity(xml.len());
            out.push_str(&xml[..start]);
            out.push_str(&xml[start + rel_end + 2..]);
            return std::borrow::Cow::Owned(out);
        }
    }
    std::borrow::Cow::Borrowed(xml)
}

// --- Reference-line geometry --------------------------------------------------

enum Geom {
    Line,
    Arc {
        curvature: f64,
    },
    /// A curve with no elementary arc-length form (spiral/clothoid or
    /// paramPoly3), baked to `(ds, x, y, hdg)` samples at load. See
    /// `bake_spiral` and `bake_param_poly3`. `pose` interpolates them by arc
    /// length `ds`.
    Baked {
        samples: Vec<(f64, f64, f64, f64)>,
    },
}

/// Bake a clothoid to fine `(ds, x, y, hdg)` samples. Curvature varies linearly
/// `curv_start -> curv_end` over `length`, so heading is the closed form
/// `hdg0 + curv_start*u + (c_dot/2)*u^2`; position is its running integral,
/// which has no elementary form, so integrate cos/sin(heading) by the midpoint
/// rule at a fine step (mm-accurate over hundreds of meters).
fn bake_spiral(
    x0: f64,
    y0: f64,
    hdg0: f64,
    curv_start: f64,
    curv_end: f64,
    length: f64,
) -> Vec<(f64, f64, f64, f64)> {
    const STEP: f64 = 0.25;
    let c_dot = if length > 1e-9 {
        (curv_end - curv_start) / length
    } else {
        0.0
    };
    let heading = |u: f64| hdg0 + curv_start * u + 0.5 * c_dot * u * u;
    let (mut x, mut y, mut u) = (x0, y0, 0.0);
    let mut out = vec![(0.0, x0, y0, hdg0)];
    while u < length - 1e-9 {
        let step = STEP.min(length - u);
        let theta_mid = heading(u + step / 2.0);
        x += theta_mid.cos() * step;
        y += theta_mid.sin() * step;
        u += step;
        out.push((u, x, y, heading(u)));
    }
    out
}

/// Evaluate `a + b*p + c*p^2 + d*p^3`.
fn eval_poly3([a, b, c, d]: [f64; 4], p: f64) -> f64 {
    a + b * p + c * p * p + d * p * p * p
}

/// Bake a paramPoly3 to `(ds, x, y, hdg)` samples. The curve is given in a local
/// `(u, v)` frame (u forward, v left of `hdg0`) by two cubics in a parameter
/// `p`; `p_max` is `1` for `pRange="normalized"`, else the geometry length.
/// Because `p` is not arc length, we step `p` finely, transform each point into
/// world space, and accumulate the true arc length `ds` so `pose` can sample by
/// distance like every other geometry.
fn bake_param_poly3(
    x0: f64,
    y0: f64,
    hdg0: f64,
    u: [f64; 4],
    v: [f64; 4],
    p_max: f64,
    length: f64,
) -> Vec<(f64, f64, f64, f64)> {
    let (sin, cos) = hdg0.sin_cos();
    let world = |p: f64| {
        let (uu, vv) = (eval_poly3(u, p), eval_poly3(v, p));
        (x0 + uu * cos - vv * sin, y0 + uu * sin + vv * cos)
    };
    let heading = |p: f64| {
        let du = u[1] + 2.0 * u[2] * p + 3.0 * u[3] * p * p;
        let dv = v[1] + 2.0 * v[2] * p + 3.0 * v[3] * p * p;
        hdg0 + dv.atan2(du)
    };
    // ~0.25 m resolution, from the geometry length.
    let n = ((length / 0.25).ceil() as usize).max(8);
    let (mut px, mut py) = world(0.0);
    let mut ds = 0.0;
    let mut out = vec![(0.0, px, py, heading(0.0))];
    for k in 1..=n {
        let p = p_max * (k as f64) / (n as f64);
        let (x, y) = world(p);
        ds += ((x - px).powi(2) + (y - py).powi(2)).sqrt();
        out.push((ds, x, y, heading(p)));
        (px, py) = (x, y);
    }
    out
}

/// One `<geometry>` record: its start pose on the reference line plus its shape.
struct GeomRec {
    s: f64,
    x: f64,
    y: f64,
    hdg: f64,
    geom: Geom,
}

impl GeomRec {
    /// Position `(x, y)` and heading at road arc-length `s` within this record.
    fn pose(&self, s: f64) -> (f64, f64, f64) {
        let ds = s - self.s;
        match &self.geom {
            Geom::Arc { curvature } if curvature.abs() > 1e-9 => {
                let h = self.hdg + curvature * ds;
                let x = self.x + (h.sin() - self.hdg.sin()) / curvature;
                let y = self.y - (h.cos() - self.hdg.cos()) / curvature;
                (x, y, h)
            }
            Geom::Baked { samples } => {
                let max = samples.last().map(|p| p.0).unwrap_or(0.0);
                let ds = ds.clamp(0.0, max);
                let i = samples
                    .partition_point(|p| p.0 <= ds)
                    .saturating_sub(1)
                    .min(samples.len().saturating_sub(2));
                let (a_s, ax, ay, ah) = samples[i];
                let (b_s, bx, by, bh) = samples[i + 1];
                let t = if b_s > a_s {
                    (ds - a_s) / (b_s - a_s)
                } else {
                    0.0
                };
                (ax + (bx - ax) * t, ay + (by - ay) * t, ah + (bh - ah) * t)
            }
            // A line, or a degenerate (straight) arc.
            _ => (
                self.x + ds * self.hdg.cos(),
                self.y + ds * self.hdg.sin(),
                self.hdg,
            ),
        }
    }
}

/// One cubic record (elevation, or a lane width), evaluated relative to its
/// own start offset: `a + b*ds + c*ds^2 + d*ds^3`.
struct Cubic {
    start: f64,
    a: f64,
    b: f64,
    c: f64,
    d: f64,
}

impl Cubic {
    fn eval(&self, s: f64) -> f64 {
        let ds = s - self.start;
        self.a + self.b * ds + self.c * ds * ds + self.d * ds * ds * ds
    }
}

/// The record whose start is the greatest not exceeding `s` (records sorted by
/// start). `None` if `s` precedes them all / the list is empty.
///
/// A scan, not a binary search, even though this runs once per profile per
/// sampled station. Real files keep these lists short. Across Town07's 234
/// roads the longest elevation, width and laneOffset lists are 13, 17 and a
/// handful of records, and at that size `partition_point` measured slower
/// than walking back from the end.
fn active(records: &[Cubic], s: f64) -> Option<&Cubic> {
    records.iter().rev().find(|r| r.start <= s + 1e-9)
}

/// One lane parsed from a `<laneSection>`, before it is sampled into a [`Lane`].
struct LaneDef {
    id: i32,
    kind: LaneType,
    widths: Vec<Cubic>,
    pred_link: Option<i32>,
    succ_link: Option<i32>,
}

/// The [`LaneType`] an OpenDRIVE `<lane>` `type` maps to. A new lane type is
/// one arm here.
///
/// Every name maps to something. An absent or unrecognised type becomes
/// [`LaneType::Unknown`], so every `<lane>` carrying a width becomes a [`Lane`].
/// The importer used to drop the types it had no variant for, which left holes
/// in the surface where `none` and vendor-specific lanes belonged. Town07 alone
/// lost 27 lanes that way, averaging 3.5 m wide.
///
/// `mwyEntry` and `mwyExit` are the older spellings of `entry` and `exit`, and
/// land on the same variants.
fn lane_type(od_type: Option<&str>) -> LaneType {
    let Some(od_type) = od_type else {
        return LaneType::Unknown;
    };
    match od_type {
        "none" => LaneType::None,
        "driving" => LaneType::Driving,
        "bidirectional" => LaneType::Bidirectional,
        "bus" => LaneType::Bus,
        "taxi" => LaneType::Taxi,
        "HOV" => LaneType::Hov,
        "entry" | "mwyEntry" => LaneType::Entry,
        "exit" | "mwyExit" => LaneType::Exit,
        "onRamp" => LaneType::OnRamp,
        "offRamp" => LaneType::OffRamp,
        "connectingRamp" => LaneType::ConnectingRamp,
        "slipLane" => LaneType::SlipLane,
        "parking" => LaneType::Parking,
        "stop" => LaneType::Stop,
        "restricted" => LaneType::Restricted,
        "biking" => LaneType::Biking,
        "sidewalk" => LaneType::Sidewalk,
        "shoulder" => LaneType::Shoulder,
        "border" => LaneType::Border,
        "curb" => LaneType::Curb,
        "median" => LaneType::Median,
        "roadWorks" => LaneType::RoadWorks,
        "tram" => LaneType::Tram,
        "rail" => LaneType::Rail,
        "special1" => LaneType::Special1,
        "special2" => LaneType::Special2,
        "special3" => LaneType::Special3,
        _ => LaneType::Unknown,
    }
}

// --- Parsing ------------------------------------------------------------------

/// A numeric attribute, or `None` if it is absent, unparseable, or non-finite.
///
/// The finiteness check is not paranoia: Rust's float parser accepts the
/// literal `NaN`, and turns an out-of-range exponent (`1e400`) into infinity,
/// so an XML attribute carries either straight into the baked geometry. One
/// such value poisons every point derived from it, and a NaN vertex makes the
/// road's physics trimesh impossible to build.
fn attr_f64(node: roxmltree::Node, name: &str) -> Option<f64> {
    node.attribute(name)
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|v| v.is_finite())
}

/// Bakes one `<road>`'s lanes into `out`, places its objects in `objects`,
/// looking up what its `<objectReference>`s name in `index`, and puts its
/// tunnels and bridges over its lanes in `structures`.
///
/// A road the importer cannot interpret is *skipped*, not fatal. That covers
/// no length, no `<planView>`, no supported geometry, and no `<lanes>`. Real
/// exports carry the occasional junk road, and losing a whole city map to one
/// of them is the worse failure. Individual malformed lanes are already skipped the
/// same way. `load_str` still errors if the document as a whole yielded no
/// lanes at all, so a thoroughly broken file is never silently accepted.
fn parse_road(
    road: roxmltree::Node,
    index: &ObjectIndex,
    out: &mut Vec<Lane>,
    objects: &mut Objects,
    structures: &mut Structures,
    topo: &mut Topology,
) {
    let road_id = road.attribute("id").unwrap_or_default().to_string();
    let Some(length) = attr_f64(road, "length") else {
        return;
    };

    let Some(plan_view) = child(road, "planView") else {
        return;
    };
    let mut geoms: Vec<GeomRec> = Vec::new();
    for g in plan_view.children().filter(|n| n.has_tag_name("geometry")) {
        // A geometry record missing (or carrying a non-finite) pose is skipped
        // rather than baked: the rest of the road is still usable.
        let (Some(s), Some(x), Some(y), Some(hdg), Some(length)) = (
            attr_f64(g, "s"),
            attr_f64(g, "x"),
            attr_f64(g, "y"),
            attr_f64(g, "hdg"),
            attr_f64(g, "length"),
        ) else {
            continue;
        };
        let geom = if let Some(arc) = child(g, "arc") {
            let Some(curvature) = attr_f64(arc, "curvature") else {
                continue;
            };
            Geom::Arc { curvature }
        } else if let Some(sp) = child(g, "spiral") {
            let (Some(curv_start), Some(curv_end)) =
                (attr_f64(sp, "curvStart"), attr_f64(sp, "curvEnd"))
            else {
                continue;
            };
            Geom::Baked {
                samples: bake_spiral(x, y, hdg, curv_start, curv_end, length),
            }
        } else if let Some(pp) = child(g, "paramPoly3") {
            let coeff = |n: &str| attr_f64(pp, n).unwrap_or(0.0);
            // "arcLength" -> p in [0,len]; anything else, including an absent
            // attribute, is "normalized" p in [0,1], matching libOpenDRIVE's
            // default and case-insensitive compare (files set it explicitly).
            let p_max = match pp.attribute("pRange").map(str::to_ascii_lowercase) {
                Some(ref r) if r == "arclength" => length,
                _ => 1.0,
            };
            Geom::Baked {
                samples: bake_param_poly3(
                    x,
                    y,
                    hdg,
                    [coeff("aU"), coeff("bU"), coeff("cU"), coeff("dU")],
                    [coeff("aV"), coeff("bV"), coeff("cV"), coeff("dV")],
                    p_max,
                    length,
                ),
            }
        } else if let Some(p3) = child(g, "poly3") {
            // poly3 is the special case of paramPoly3 with u(p)=p and v(p) the
            // cubic in u; reuse the same baker over p in [0, length].
            let coeff = |n: &str| attr_f64(p3, n).unwrap_or(0.0);
            Geom::Baked {
                samples: bake_param_poly3(
                    x,
                    y,
                    hdg,
                    [0.0, 1.0, 0.0, 0.0],
                    [coeff("a"), coeff("b"), coeff("c"), coeff("d")],
                    length,
                    length,
                ),
            }
        } else if child(g, "line").is_some() {
            Geom::Line
        } else {
            // An unknown geometry shape: skip it rather than fail the load.
            continue;
        };
        geoms.push(GeomRec { s, x, y, hdg, geom });
    }
    if geoms.is_empty() {
        return; // nothing drivable to bake
    }
    geoms.sort_by(|a, b| a.s.total_cmp(&b.s));

    let elevations = child(road, "elevationProfile")
        .map(|n| cubics_in(n, "elevation", "s"))
        .unwrap_or_default();
    // Superelevation: a roll of the whole cross-section about the reference
    // line, a cubic in road-s. Same `Cubic`/`active` machinery as elevation;
    // applied per lane by the reference-line pivot in `sample_lane`.
    let superelevations = child(road, "lateralProfile")
        .map(|n| cubics_in(n, "superelevation", "s"))
        .unwrap_or_default();
    let sections = bake_lanes(
        road,
        &road_id,
        length,
        &geoms,
        &elevations,
        &superelevations,
        out,
        topo,
    );
    if let Some(objects_node) = child(road, "objects") {
        let road = BakedRoad {
            id: &road_id,
            length,
            sections: &sections,
            surface: &|s, t| surface_at(&geoms, &elevations, &superelevations, s, t),
        };
        place_objects(objects_node, index, &road, objects);
        place_structures(objects_node, &road, out, structures);
    }
}

/// One lane section of a road as baked: the stretch of road it covers, and
/// the lanes in it.
struct BakedSection {
    start: f64,
    end: f64,
    lanes: Vec<BakedLane>,
}

/// One baked lane of a [`BakedSection`].
struct BakedLane {
    od_id: i32,
    id: LaneId,
    /// Its position in the lane list.
    index: usize,
}

/// Whether the lane `od_id` is in one of the `validity` ranges, or there are
/// none.
fn is_valid(od_id: i32, validity: &[(i32, i32)]) -> bool {
    validity.is_empty()
        || validity
            .iter()
            .any(|&(a, b)| (a.min(b)..=a.max(b)).contains(&od_id))
}

/// What placing an object needs to know about the road it is on.
struct BakedRoad<'a> {
    id: &'a str,
    length: f64,
    sections: &'a [BakedSection],
    /// The road surface at a station `(s, t)`, and the reference line's
    /// heading there.
    surface: &'a dyn Fn(f64, f64) -> (Point, f64),
}

impl BakedRoad<'_> {
    fn on_road(&self, s: f64) -> bool {
        (0.0..=self.length).contains(&s)
    }

    /// The lanes alongside the stretch `[from, to]` whose `<lane id>` is in
    /// one of the `validity` ranges, or all of them if there are no ranges.
    /// A section starting exactly at `to` counts only if the stretch is a
    /// single station, so a station on a section boundary is in the section
    /// that starts there.
    fn lanes(&self, (from, to): (f64, f64), validity: &[(i32, i32)]) -> Vec<LaneId> {
        let last = self.sections.len().saturating_sub(1);
        self.sections
            .iter()
            .enumerate()
            .filter(|(i, sec)| {
                sec.start <= to && (from < sec.end || (*i == last && from <= sec.end))
            })
            .flat_map(|(_, sec)| &sec.lanes)
            .filter(|l| is_valid(l.od_id, validity))
            .map(|l| l.id)
            .collect()
    }
}

/// Bakes one `<road>`'s lanes into `out`, and returns its lane sections. None
/// for a road with no `<lanes>`.
#[allow(clippy::too_many_arguments)]
fn bake_lanes(
    road: roxmltree::Node,
    road_id: &str,
    length: f64,
    geoms: &[GeomRec],
    elevations: &[Cubic],
    superelevations: &[Cubic],
    out: &mut Vec<Lane>,
    topo: &mut Topology,
) -> Vec<BakedSection> {
    let Some(lanes_node) = child(road, "lanes") else {
        return Vec::new();
    };
    // laneOffset shifts the whole lane cross-section laterally off lane 0 (lane
    // widening, merges, a centerline that isn't the road reference). It adds to
    // every lane's offset, so it must be applied or all lanes are mis-placed.
    let lane_offsets = cubics_in(lanes_node, "laneOffset", "s");

    // Every lane section becomes its own set of lanes, each spanning that
    // section's `s`-range `[start, next-start-or-length]`.
    let mut sections: Vec<roxmltree::Node> = lanes_node
        .children()
        .filter(|n| n.has_tag_name("laneSection"))
        .collect();
    if sections.is_empty() {
        return Vec::new();
    }
    sections.sort_by(|a, b| {
        attr_f64(*a, "s")
            .unwrap_or(0.0)
            .total_cmp(&attr_f64(*b, "s").unwrap_or(0.0))
    });

    // Record the road's link targets + section count for connectivity.
    let (predecessor, successor) = links::road_link(road);
    topo.roads.insert(
        road_id.to_string(),
        RoadInfo {
            sections: sections.len(),
            predecessor,
            successor,
        },
    );

    let mut baked = Vec::new();
    for (i, section) in sections.iter().enumerate() {
        let s_start = attr_f64(*section, "s").unwrap_or(0.0);
        let s_end = sections
            .get(i + 1)
            .map(|n| attr_f64(*n, "s").unwrap_or(length))
            .unwrap_or(length)
            .min(length);
        if s_end - s_start < 1e-3 {
            continue; // zero-length section
        }
        let (first, first_lane) = (topo.metas.len(), out.len());
        emit_section(
            *section,
            s_start,
            s_end,
            geoms,
            elevations,
            superelevations,
            &lane_offsets,
            out,
            topo,
            road_id,
            i,
        );
        // emit_section records a lane's meta as it pushes the lane, so the
        // new metas and the new lanes are in step.
        baked.push(BakedSection {
            start: s_start,
            end: s_end,
            lanes: topo.metas[first..]
                .iter()
                .enumerate()
                .map(|(k, m)| BakedLane {
                    od_id: m.od_id,
                    id: m.id,
                    index: first_lane + k,
                })
                .collect(),
        });
    }
    baked
}

/// Append each lane of one section as a `Lane` spanning `[s_start, s_end]`.
/// Malformed individual lanes are skipped, not fatal (real files).
///
/// Every lane with a width is sampled for the running lateral offset, whatever
/// its type; only the ones [`lane_type`] recognises become a `Lane`.
#[allow(clippy::too_many_arguments)]
fn emit_section(
    section: roxmltree::Node,
    s_start: f64,
    s_end: f64,
    geoms: &[GeomRec],
    elevations: &[Cubic],
    superelevations: &[Cubic],
    lane_offsets: &[Cubic],
    out: &mut Vec<Lane>,
    topo: &mut Topology,
    road_id: &str,
    section_idx: usize,
) {
    let (mut left, mut right) = (Vec::new(), Vec::new());
    for side in ["left", "right"] {
        let Some(side_node) = child(section, side) else {
            continue;
        };
        for lane in side_node.children().filter(|n| n.has_tag_name("lane")) {
            let Some(id) = lane.attribute("id").and_then(|s| s.parse::<i32>().ok()) else {
                continue;
            };
            let widths = parse_width_cubics(lane);
            if widths.is_empty() {
                continue; // no width: nothing to sample, and no offset to add
            }
            let kind = lane_type(lane.attribute("type"));
            let (pred_link, succ_link) = links::lane_link(lane);
            let def = LaneDef {
                id,
                kind,
                widths,
                pred_link,
                succ_link,
            };
            if id > 0 {
                left.push(def);
            } else if id < 0 {
                right.push(def);
            }
        }
    }
    // Order each side from the center outward, so cumulative width works.
    left.sort_by_key(|l| l.id); // 1, 2, 3, ...
    right.sort_by_key(|l| -l.id); // -1, -2, -3, ...

    let sample_s = sample_positions(s_start, s_end);
    // The bank profile is a road-level property (the cross-section's roll about
    // the reference line), identical for every lane in the section, sampled
    // parallel to `sample_s`. Collapse an all-flat profile to empty, the
    // "flat lane" sentinel, so unbanked roads stay byte-identical.
    let bank: Vec<f32> = sample_s
        .iter()
        .map(|&s| active(superelevations, s).map(|e| e.eval(s)).unwrap_or(0.0) as f32)
        .collect();
    let bank = if bank.iter().all(|b| b.abs() < 1e-9) {
        Vec::new()
    } else {
        bank
    };
    for side in [&left, &right] {
        // Left lanes (positive id) offset toward +t and travel against +s;
        // right lanes (negative id) offset toward -t and travel with +s.
        let is_left = side.first().map(|l| l.id > 0).unwrap_or(false);
        let sign = if is_left { 1.0 } else { -1.0 };
        // Standard right-hand-traffic convention: negative-id (right) lanes run
        // with +s, positive-id (left) against it. OpenDRIVE itself encodes no
        // travel direction; a left-hand-traffic map would invert this.
        let direction = if is_left {
            Direction::Backward
        } else {
            Direction::Forward
        };
        // Lanes emitted on this side, center-outward, as (id, index in `out`,
        // kind). Consecutive ones are lateral neighbors (lane-change edges),
        // subject to the drivability test below.
        let mut emitted: Vec<(LaneId, usize, LaneType)> = Vec::new();
        for (i, lane) in side.iter().enumerate() {
            let kind = lane.kind;
            let (points, headings) = sample_lane(
                geoms,
                elevations,
                superelevations,
                lane_offsets,
                s_start,
                lane,
                &side[..i], // inner lanes on the same side, closer to center
                sign,
                &sample_s,
            );
            // Anchor the ends on the true curve tangents so this section's ribs
            // line up with its neighbours'.
            let (Some(&start_tangent), Some(&end_tangent)) = (headings.first(), headings.last())
            else {
                continue;
            };
            let Some(center) = Polyline::try_new_with_tangents(points, start_tangent, end_tangent)
            else {
                continue;
            };
            let id = LaneId(topo.next_id);
            topo.next_id += 1;
            topo.registry
                .insert((road_id.to_string(), section_idx, lane.id), id);
            topo.metas.push(LaneMeta {
                id,
                road: road_id.to_string(),
                section: section_idx,
                od_id: lane.id,
                direction,
                succ_link: lane.succ_link,
                pred_link: lane.pred_link,
            });
            emitted.push((id, out.len(), kind));
            let (width, widths) = width_profile(lane, s_start, &sample_s);
            out.push(Lane {
                id,
                kind,
                direction,
                center,
                width,
                widths,
                // Road-level roll, shared across the section's lanes; parallel to
                // the sampled centerline. Empty when the road is flat.
                bank: bank.clone(),
                // Filled by links::resolve once all lanes are registered.
                successors: Vec::new(),
                predecessors: Vec::new(),
                neighbors: Vec::new(),
            });
        }
        // Consecutive same-side lanes are each other's lane-change neighbors,
        // but only where both ends carry through traffic. Without that test a
        // driving lane would list the sidewalk beside it as a lane change, and
        // the router would happily take it. A non-drivable lane between two
        // driving lanes separates them for the same reason: it stands in the
        // sequence, so they are not consecutive and no edge spans it.
        for k in 0..emitted.len() {
            if !emitted[k].2.is_drivable() {
                continue;
            }
            let nbrs = [k.checked_sub(1), Some(k + 1)]
                .into_iter()
                .flatten()
                .filter_map(|n| emitted.get(n))
                .filter(|(_, _, kind)| kind.is_drivable())
                .map(|(id, _, _)| *id)
                .collect();
            out[emitted[k].1].neighbors = nbrs;
        }
    }
}

/// Sample one lane's centerline to points in our coordinate frame, with the
/// reference line's analytical heading at each sample.
///
/// The headings are the exact `hdg` of the underlying geometry record, not the
/// chords between the points, so two sections sampled either side of the same
/// station report the same heading there. That shared value is what lets their
/// meshed ribs meet flush; see [`Polyline::try_new_with_tangents`].
#[allow(clippy::too_many_arguments)]
fn sample_lane(
    geoms: &[GeomRec],
    elevations: &[Cubic],
    superelevations: &[Cubic],
    lane_offsets: &[Cubic],
    section_s: f64,
    lane: &LaneDef,
    inner: &[LaneDef],
    sign: f64,
    sample_s: &[f64],
) -> (Vec<Point>, Vec<Vector>) {
    sample_s
        .iter()
        .map(|&s| {
            let s_lane = s - section_s;
            // Center offset: the road's laneOffset (shared by all lanes) plus
            // this lane's own: cumulative inner-lane widths + half its width.
            let base = active(lane_offsets, s).map(|o| o.eval(s)).unwrap_or(0.0);
            let inner_w: f64 = inner.iter().map(|l| width_at(l, s_lane)).sum();
            let t = base + sign * (inner_w + width_at(lane, s_lane) / 2.0);
            let (point, hdg) = surface_at(geoms, elevations, superelevations, s, t);
            // A lane offset laterally by a constant `t` is parallel to the
            // reference line, so it shares its heading. Where `t` varies with
            // `s` the lane's own tangent swings off it slightly; the reference
            // heading is used anyway, because being the *same* on both sides of
            // a section joint is what closes the seam, and a lane's `dt/ds`
            // generally steps across that joint.
            let heading = Vector::new(hdg.cos() as f32, hdg.sin() as f32, 0.0);
            (point, heading)
        })
        .unzip()
}

/// The road surface at station `s`, lateral offset `t` from the reference
/// line, and the reference line's heading there.
fn surface_at(
    geoms: &[GeomRec],
    elevations: &[Cubic],
    superelevations: &[Cubic],
    s: f64,
    t: f64,
) -> (Point, f64) {
    let (x, y, hdg) = geom_at(geoms, s).pose(s);
    let elev = active(elevations, s).map(|e| e.eval(s)).unwrap_or(0.0);
    // Reference-line pivot: superelevation rolls the cross-section about the
    // reference line by phi, so a point at lateral t rides up by t·sin phi
    // (positive t = left edge, raised for phi > 0) and its horizontal reach
    // shrinks to t·cos phi. phi = 0 leaves this the pure-horizontal offset it
    // was.
    let phi = active(superelevations, s).map(|e| e.eval(s)).unwrap_or(0.0);
    let (sin_phi, cos_phi) = phi.sin_cos();
    let t_h = t * cos_phi;
    // The baked frame is OpenDRIVE's own, so the reference-line point needs no
    // mapping. Offset along the left-hand normal, which in the reference line's
    // plane is (-sin hdg, cos hdg).
    let point = Point::new(
        (x - t_h * hdg.sin()) as f32,
        (y + t_h * hdg.cos()) as f32,
        (elev + t * sin_phi) as f32,
    );
    (point, hdg)
}

fn width_at(lane: &LaneDef, s_lane: f64) -> f64 {
    active(&lane.widths, s_lane)
        .map(|w| w.eval(s_lane).max(0.0))
        .unwrap_or(0.0)
}

/// A lane's nominal width and its per-sample profile, sampled parallel to
/// `sample_s`.
///
/// The profile collapses to empty when the lane holds one width all the way
/// along, which covers most lanes, so an ordinary lane bakes to exactly what
/// it baked to before per-station widths existed.
///
/// The nominal width is the widest the lane gets, not the width where it
/// starts. A lane that opens out of a point starts at 0 m, and reporting that
/// as its width gave a gore area no surface at all.
fn width_profile(lane: &LaneDef, s_start: f64, sample_s: &[f64]) -> (f32, Vec<f32>) {
    let widths: Vec<f32> = sample_s
        .iter()
        .map(|&s| width_at(lane, s - s_start) as f32)
        .collect();
    let nominal = widths.iter().copied().fold(0.0_f32, f32::max);
    let constant = widths.iter().all(|w| (w - nominal).abs() < 1e-6);
    (nominal, if constant { Vec::new() } else { widths })
}

fn geom_at(geoms: &[GeomRec], s: f64) -> &GeomRec {
    geoms
        .iter()
        .rev()
        .find(|g| g.s <= s + 1e-9)
        .unwrap_or(&geoms[0])
}

/// Arc-length sample positions over `[start, end]`, spaced `SAMPLE_STEP`,
/// always including the exact endpoints.
fn sample_positions(start: f64, end: f64) -> Vec<f64> {
    let mut ss = Vec::new();
    let mut s = start;
    while s < end - 1e-6 {
        ss.push(s);
        s += SAMPLE_STEP;
    }
    ss.push(end);
    ss
}

/// Parse every `<item>` child of `parent` as a cubic (elevation, laneOffset),
/// keyed on `start_attr`, sorted by start.
fn cubics_in(parent: roxmltree::Node, item: &str, start_attr: &str) -> Vec<Cubic> {
    let mut out: Vec<Cubic> = parent
        .children()
        .filter(|n| n.has_tag_name(item))
        .filter_map(|n| {
            Some(Cubic {
                start: attr_f64(n, start_attr)?,
                a: attr_f64(n, "a").unwrap_or(0.0),
                b: attr_f64(n, "b").unwrap_or(0.0),
                c: attr_f64(n, "c").unwrap_or(0.0),
                d: attr_f64(n, "d").unwrap_or(0.0),
            })
        })
        .collect();
    out.sort_by(|a, b| a.start.total_cmp(&b.start));
    out
}

fn parse_width_cubics(lane: roxmltree::Node) -> Vec<Cubic> {
    let mut out: Vec<Cubic> = lane
        .children()
        .filter(|n| n.has_tag_name("width"))
        .filter_map(|n| {
            Some(Cubic {
                start: attr_f64(n, "sOffset").unwrap_or(0.0),
                a: attr_f64(n, "a")?,
                b: attr_f64(n, "b").unwrap_or(0.0),
                c: attr_f64(n, "c").unwrap_or(0.0),
                d: attr_f64(n, "d").unwrap_or(0.0),
            })
        })
        .collect();
    out.sort_by(|a, b| a.start.total_cmp(&b.start));
    out
}

// --- Objects ------------------------------------------------------------------

/// The objects baked so far, and the provenance of each, in step.
#[derive(Default)]
struct Objects {
    baked: Vec<Object>,
    provenance: Vec<ObjectProvenance>,
}

/// Where an object sits along its road, and how big it is there. An
/// `<object>` gives one of these, and a `<repeat>` gives one per station.
struct Station {
    s: f64,
    t: f64,
    z_offset: f64,
    length: Option<f64>,
    width: Option<f64>,
    height: Option<f64>,
    radius: Option<f64>,
}

/// An object's own frame at one station: its origin, raised off the road by
/// `zOffset`, and its orientation. The yaw is the reference line's heading
/// plus the object's `hdg`. `pitch` and `roll` are already against the ground
/// plane.
struct Frame {
    origin: Point,
    yaw: f64,
    pitch: f64,
    roll: f64,
}

impl Frame {
    /// A point given in this frame, in the network's. The rotation is yaw
    /// about Z, then pitch about the turned Y, then roll about the turned X.
    fn point(&self, local: [f64; 3]) -> Point {
        let [u, v, z] = orient(self.yaw, self.pitch, self.roll, local);
        self.origin + Vector::new(u as f32, v as f32, z as f32)
    }
}

/// The structures baked so far, and the provenance of each, in step.
#[derive(Default)]
struct Structures {
    baked: Vec<Structure>,
    provenance: Vec<StructureProvenance>,
}

/// Bake every `<tunnel>` and `<bridge>` under one road's `<objects>` into
/// `out`, as the part of each of the road's `lanes` it covers.
///
/// A structure covers `length` metres of road from `s`, across every lane
/// there, or across those in its `<validity>` ranges if it has any. It has no
/// geometry: OpenDRIVE describes neither a tunnel's tube nor a bridge's deck.
/// One missing `s` or `length`, or with a negative length, is skipped.
fn place_structures(
    objects_node: roxmltree::Node,
    road: &BakedRoad,
    lanes: &[Lane],
    out: &mut Structures,
) {
    for node in objects_node.children() {
        let text = |name| node.attribute(name).unwrap_or_default().to_string();
        let kind = if node.has_tag_name("tunnel") {
            StructureKind::Tunnel {
                kind: text("type"),
                lighting: attr_f64(node, "lighting").map(|v| v as f32),
                daylight: attr_f64(node, "daylight").map(|v| v as f32),
            }
        } else if node.has_tag_name("bridge") {
            StructureKind::Bridge { kind: text("type") }
        } else {
            continue;
        };
        let (Some(s), Some(length)) = (attr_f64(node, "s"), attr_f64(node, "length")) else {
            continue;
        };
        if length < 0.0 {
            continue;
        }
        let validity = validity(node);
        let mut covered = Vec::new();
        for sec in road.sections {
            let (from, to) = (s.max(sec.start), (s + length).min(sec.end));
            if to - from < 1e-6 {
                continue;
            }
            let stations = sample_positions(sec.start, sec.end);
            for lane in sec.lanes.iter().filter(|l| is_valid(l.od_id, &validity)) {
                let center = &lanes[lane.index].center;
                covered.push(Coverage {
                    lane: lane.id,
                    from: along(center.points(), &stations, from),
                    to: along(center.points(), &stations, to),
                });
            }
        }
        let id = StructureId(out.baked.len());
        out.baked.push(Structure {
            id,
            kind,
            name: text("name"),
            lanes: covered,
        });
        out.provenance.push(StructureProvenance {
            structure: id,
            road_id: road.id.to_string(),
            od_id: text("id"),
            s,
            length,
        });
    }
}

/// How far along a lane road station `s` is, in metres from its first
/// point. `points` are the lane's centerline, sampled at the road
/// `stations`, one each.
fn along(points: &[Point], stations: &[f64], s: f64) -> f32 {
    debug_assert_eq!(points.len(), stations.len());
    let i = stations
        .partition_point(|&x| x <= s)
        .saturating_sub(1)
        .min(stations.len().saturating_sub(2));
    let before: f32 = points[..=i]
        .windows(2)
        .map(|w| w[0].distance_to(w[1]))
        .sum();
    let f = ((s - stations[i]) / (stations[i + 1] - stations[i])).clamp(0.0, 1.0);
    before + points[i].distance_to(points[i + 1]) * f as f32
}

/// Every `<object>` in the document by its id, with the road it is on, for
/// an `<objectReference>` to find. Ids are meant to be unique across the
/// file. Where two objects share one, the first wins.
type ObjectIndex<'a> = HashMap<&'a str, (&'a str, roxmltree::Node<'a, 'a>)>;

fn object_index<'a>(root: roxmltree::Node<'a, 'a>) -> ObjectIndex<'a> {
    let mut index = ObjectIndex::new();
    for road in root.children().filter(|n| n.has_tag_name("road")) {
        let road_id = road.attribute("id").unwrap_or_default();
        let Some(objects) = child(road, "objects") else {
            continue;
        };
        for object in objects.children().filter(|n| n.has_tag_name("object")) {
            if let Some(id) = object.attribute("id") {
                index.entry(id).or_insert((road_id, object));
            }
        }
    }
    index
}

/// Where one `<object>` is baked, and what its provenance says about it. An
/// `<object>` is placed at its own station. An `<objectReference>` places the
/// `<object>` it names at the reference's station instead, with the
/// reference's `zOffset`, `orientation` and `validLength`.
struct Placement<'a> {
    s: f64,
    t: f64,
    z_offset: f64,
    orientation: Orientation,
    valid_length: Option<f64>,
    /// How far the placement moves the object along and across the road,
    /// from the station its `<object>` gives to this one. What the
    /// `<object>` gives in road coordinates, its `<repeat>`s and its
    /// `<cornerRoad>`s, moves with it. `(0, 0)` for an `<object>` placed
    /// where it says.
    shift: (f64, f64),
    /// For a reference, the road the `<object>` is on.
    referenced_from: Option<&'a str>,
    /// The `<validity>` lane ranges, `fromLane` and `toLane`, of the
    /// `<object>` or the `<objectReference>`. A reference does not take the
    /// `<object>`'s, which name lanes on the `<object>`'s road.
    validity: Vec<(i32, i32)>,
}

impl<'a> Placement<'a> {
    /// An `<object>` where it says it is, or `None` if it is missing `s` or
    /// `t`.
    fn own(object: roxmltree::Node) -> Option<Self> {
        Some(Self {
            s: attr_f64(object, "s")?,
            t: attr_f64(object, "t")?,
            z_offset: attr_f64(object, "zOffset").unwrap_or(0.0),
            orientation: orientation(object),
            valid_length: attr_f64(object, "validLength"),
            shift: (0.0, 0.0),
            referenced_from: None,
            validity: validity(object),
        })
    }

    /// `object`, on `road`, where `reference` puts it, or `None` if the
    /// reference is missing `s` or `t`.
    fn reference(
        reference: roxmltree::Node,
        object: roxmltree::Node,
        road: &'a str,
    ) -> Option<Self> {
        let (s, t) = (attr_f64(reference, "s")?, attr_f64(reference, "t")?);
        let from = |name| attr_f64(object, name).unwrap_or(0.0);
        Some(Self {
            s,
            t,
            z_offset: attr_f64(reference, "zOffset").unwrap_or(0.0),
            orientation: orientation(reference),
            valid_length: attr_f64(reference, "validLength"),
            shift: (s - from("s"), t - from("t")),
            referenced_from: Some(road),
            validity: validity(reference),
        })
    }
}

/// The `fromLane`-`toLane` range of each `<validity>` under `node`. One
/// missing either is skipped.
fn validity(node: roxmltree::Node) -> Vec<(i32, i32)> {
    let lane = |v: roxmltree::Node, name| v.attribute(name)?.parse::<i32>().ok();
    node.children()
        .filter(|n| n.has_tag_name("validity"))
        .filter_map(|v| Some((lane(v, "fromLane")?, lane(v, "toLane")?)))
        .collect()
}

/// Which direction of the road an `<object>` or `<objectReference>` applies
/// to.
fn orientation(node: roxmltree::Node) -> Orientation {
    match node.attribute("orientation") {
        Some("+") => Orientation::Positive,
        Some("-") => Orientation::Negative,
        _ => Orientation::Both,
    }
}

/// Bake every `<object>` and `<objectReference>` under one road's
/// `<objects>` into `out`. `index` finds the `<object>` a reference names.
///
/// A reference to an id no `<object>` has is skipped, as is one missing `s`
/// or `t`.
fn place_objects(
    objects_node: roxmltree::Node,
    index: &ObjectIndex,
    road: &BakedRoad,
    out: &mut Objects,
) {
    for node in objects_node.children() {
        let placed = if node.has_tag_name("object") {
            Placement::own(node).map(|at| (node, at))
        } else if node.has_tag_name("objectReference") {
            node.attribute("id")
                .and_then(|id| index.get(id))
                .and_then(|&(road, object)| {
                    Placement::reference(node, object, road).map(|at| (object, at))
                })
        } else {
            None
        };
        if let Some((object, at)) = placed {
            place_object(object, &at, road, out);
        }
    }
}

/// Bake one `<object>` into `out`, placed on `road` as `at` says.
///
/// Each object sits on the road surface, so it rides the elevation and
/// superelevation profiles like a lane does. One `<object>` can bake to
/// several [`Object`]s, following libOpenDRIVE:
///
/// - with neither `<repeat>`s nor outlines, one [`Shape::Solid`];
/// - one [`Shape::Solid`] per step of each `<repeat>` with a `distance`, and
///   one [`Shape::Sweep`] per `<repeat>` with a `distance` of 0;
/// - one [`Shape::Outline`] per outer `<outline>`. Outlines are not
///   repeated, and an object with outlines gets no solid of its own. An
///   `outer="false"` outline is a hole in the first closed outer outline
///   that encloses it in plan, and is dropped if none does.
///
/// Any part of it that falls off the ends of the road is skipped: a solid, a
/// whole outline with a corner there, or the length of a sweep past the end.
///
/// Each baked object applies to the lanes alongside the stretch of road it
/// spans, narrowed by `at`'s validity.
fn place_object(node: roxmltree::Node, at: &Placement, road: &BakedRoad, out: &mut Objects) {
    let base = Station {
        s: at.s,
        t: at.t,
        z_offset: at.z_offset,
        length: attr_f64(node, "length"),
        width: attr_f64(node, "width"),
        height: attr_f64(node, "height"),
        radius: attr_f64(node, "radius"),
    };
    let hdg = attr_f64(node, "hdg").unwrap_or(0.0);
    let pitch = attr_f64(node, "pitch").unwrap_or(0.0);
    let roll = attr_f64(node, "roll").unwrap_or(0.0);
    let frame = |st: &Station| {
        let (mut origin, road_hdg) = (road.surface)(st.s, st.t);
        origin.z += st.z_offset as f32;
        Frame {
            origin,
            yaw: road_hdg + hdg,
            pitch,
            roll,
        }
    };
    let solid = |st: &Station| {
        let f = frame(st);
        Shape::Solid {
            position: f.origin,
            heading: f.yaw as f32,
            pitch: f.pitch as f32,
            roll: f.roll as f32,
            extent: extent(st),
        }
    };

    let mut parts = Vec::new();
    let repeats: Vec<Repeat> = node
        .children()
        .filter(|n| n.has_tag_name("repeat"))
        .filter_map(|n| Repeat::parse(n, at.shift))
        .collect();
    let outlines = outline_nodes(node);
    let point = |st: &Station| Part::new(solid(st), st, (st.s, st.s));
    if repeats.is_empty() && outlines.is_empty() && road.on_road(base.s) {
        parts.push(point(&base));
    }
    for repeat in &repeats {
        if repeat.distance > 0.0 {
            let stations = repeat.stations(&base);
            parts.extend(stations.iter().filter(|st| road.on_road(st.s)).map(point));
        } else if let Some((sections, start, end)) = sweep(repeat, &base, road) {
            parts.push(Part::new(Shape::Sweep { sections }, &start, (start.s, end)));
        }
    }
    let origin = road.on_road(base.s).then(|| frame(&base));
    let mut holes = Vec::new();
    for outline_node in outlines {
        let Some((corners, closed, stretch)) =
            outline(outline_node, origin.as_ref(), base.s, at.shift, road)
        else {
            continue;
        };
        if outline_node.attribute("outer") == Some("false") {
            holes.push((outline_node, corners));
            continue;
        }
        let mut part = Part::new(
            Shape::Outline {
                corners,
                closed,
                holes: Vec::new(),
            },
            &base,
            stretch,
        );
        if let Shape::Outline { corners, .. } = &part.shape {
            part.markings = markings(node, outline_node, corners);
            part.borders = borders(node, outline_node, corners, closed);
        }
        parts.push(part);
    }
    for (outline_node, corners) in holes {
        let owner = parts.iter_mut().find(|p| {
            matches!(&p.shape, Shape::Outline { corners: outer, closed: true, .. }
                if corners.len() >= 3 && corners.iter().all(|c| encloses(outer, c.base)))
        });
        let Some(owner) = owner else {
            continue;
        };
        owner
            .markings
            .extend(markings(node, outline_node, &corners));
        owner
            .borders
            .extend(borders(node, outline_node, &corners, true));
        if let Shape::Outline { holes, .. } = &mut owner.shape {
            holes.push(corners);
        }
    }

    let kind = object_type(node.attribute("type"));
    let text = |name| node.attribute(name).unwrap_or_default().to_string();
    let parking_space = child(node, "parkingSpace").map(|p| {
        let text = |name| p.attribute(name).unwrap_or_default().to_string();
        ParkingSpace {
            access: text("access"),
            restrictions: text("restrictions"),
        }
    });
    let materials: Vec<Material> = node
        .children()
        .filter(|n| n.has_tag_name("material"))
        .map(|m| Material {
            surface: m.attribute("surface").unwrap_or_default().to_string(),
            friction: attr_f64(m, "friction").map(|v| v as f32),
            roughness: attr_f64(m, "roughness").map(|v| v as f32),
        })
        .collect();
    let user_data: Vec<UserData> = node
        .children()
        .filter(|n| n.has_tag_name("userData"))
        .map(|u| {
            let text = |name| u.attribute(name).unwrap_or_default().to_string();
            UserData {
                code: text("code"),
                value: text("value"),
            }
        })
        .collect();
    for part in parts {
        let id = ObjectId(out.baked.len());
        out.baked.push(Object {
            id,
            kind,
            subtype: text("subtype"),
            name: text("name"),
            dynamic: node.attribute("dynamic") == Some("yes"),
            lanes: road.lanes(part.stretch, &at.validity),
            markings: part.markings,
            borders: part.borders,
            parking_space: parking_space.clone(),
            materials: materials.clone(),
            user_data: user_data.clone(),
            shape: part.shape,
        });
        out.provenance.push(ObjectProvenance {
            object: id,
            road_id: road.id.to_string(),
            od_id: text("id"),
            s: part.s,
            t: part.t,
            orientation: at.orientation,
            valid_length: at.valid_length,
            referenced_from: at.referenced_from.map(str::to_string),
        });
    }
}

/// One shape an `<object>` bakes to, and what goes with it, before it becomes
/// an [`Object`].
struct Part {
    shape: Shape,
    /// The road station it is anchored at.
    s: f64,
    t: f64,
    /// The stretch of road it spans, which picks its lanes.
    stretch: (f64, f64),
    markings: Vec<Marking>,
    borders: Vec<Border>,
}

impl Part {
    fn new(shape: Shape, at: &Station, stretch: (f64, f64)) -> Self {
        Self {
            shape,
            s: at.s,
            t: at.t,
            stretch,
            markings: Vec::new(),
            borders: Vec::new(),
        }
    }
}

/// One `<repeat>`: the object again along `length` metres of road from `s`,
/// either every `distance` metres or, for a `distance` of 0, continuously.
struct Repeat<'a> {
    node: roxmltree::Node<'a, 'a>,
    start: f64,
    length: f64,
    distance: f64,
    /// Added to the `t` values the repeat gives, from its [`Placement`].
    shift_t: f64,
}

impl<'a> Repeat<'a> {
    /// The repeat moved by a [`Placement`]'s `shift`, or `None` for one
    /// missing its `s`, `length` or `distance`, or carrying a negative one of
    /// the last two.
    fn parse(node: roxmltree::Node<'a, 'a>, (shift_s, shift_t): (f64, f64)) -> Option<Self> {
        let repeat = Self {
            node,
            start: attr_f64(node, "s")? + shift_s,
            length: attr_f64(node, "length")?,
            distance: attr_f64(node, "distance")?,
            shift_t,
        };
        (repeat.length >= 0.0 && repeat.distance >= 0.0).then_some(repeat)
    }

    /// The object's station a fraction `f` of the way along the repeat, with
    /// `t`, `zOffset` and each dimension interpolated from its start value to
    /// its end value. A value the repeat does not give is the object's own.
    fn at(&self, base: &Station, f: f64) -> Station {
        let lerp = |name: &str, fallback: Option<f64>, shift: f64| {
            let given = |end| attr_f64(self.node, &format!("{name}{end}")).map(|v| v + shift);
            let start = given("Start").or(fallback)?;
            let end = given("End").unwrap_or(start);
            Some(start + (end - start) * f)
        };
        Station {
            s: self.start + self.length * f,
            t: lerp("t", Some(base.t), self.shift_t).unwrap_or(base.t),
            z_offset: lerp("zOffset", Some(base.z_offset), 0.0).unwrap_or(base.z_offset),
            length: lerp("length", base.length, 0.0),
            width: lerp("width", base.width, 0.0),
            height: lerp("height", base.height, 0.0),
            radius: lerp("radius", base.radius, 0.0),
        }
    }

    /// One station every `distance` metres, both ends included, or none if
    /// that is more than [`MAX_REPEAT_INSTANCES`].
    fn stations(&self, base: &Station) -> Vec<Station> {
        let count = (self.length / self.distance).floor() + 1.0;
        if count > MAX_REPEAT_INSTANCES {
            return Vec::new();
        }
        (0..count as usize)
            .map(|k| {
                let along = k as f64 * self.distance;
                let f = if self.length > 0.0 {
                    along / self.length
                } else {
                    0.0
                };
                self.at(base, f)
            })
            .collect()
    }
}

/// A continuous repeat's cross-section at every sample station along the part
/// of it on the road: `width` wide about its `t`, `height` tall from its
/// `zOffset`. A missing width is 0, a wall with no thickness, as libOpenDRIVE
/// has it. Also returns the station the sections start at, and the `s` they
/// end at. `None` if less than a millimetre of it is on the road.
fn sweep(
    repeat: &Repeat,
    base: &Station,
    road: &BakedRoad,
) -> Option<(Vec<Section>, Station, f64)> {
    let start = repeat.start.max(0.0);
    let end = (repeat.start + repeat.length).min(road.length);
    if end - start < 1e-3 {
        return None;
    }
    let sections = sample_positions(start, end)
        .into_iter()
        .map(|s| {
            let st = repeat.at(base, (s - repeat.start) / repeat.length);
            let half = st.width.unwrap_or(0.0) / 2.0;
            let height = st.height.unwrap_or(0.0) as f32;
            let corner = |t| {
                let (mut base, _) = (road.surface)(s, t);
                base.z += st.z_offset as f32;
                Corner {
                    base,
                    top: base + Vector::Z * height,
                }
            };
            Section {
                left: corner(st.t + half),
                right: corner(st.t - half),
            }
        })
        .collect();
    Some((
        sections,
        repeat.at(base, (start - repeat.start) / repeat.length),
        end,
    ))
}

/// An object's `<outline>`s: under `<outlines>` since OpenDRIVE 1.5, and
/// straight under the `<object>` in 1.4.
fn outline_nodes<'a>(object: roxmltree::Node<'a, 'a>) -> Vec<roxmltree::Node<'a, 'a>> {
    child(object, "outlines")
        .unwrap_or(object)
        .children()
        .filter(|n| n.has_tag_name("outline"))
        .collect()
}

/// One `<outline>`'s corners in the network's frame, and whether it is
/// closed.
///
/// A `<cornerRoad>` is a road station `(s, t)`, moved by `shift`, raised by
/// `dz`. A
/// `<cornerLocal>` is `(u, v, z)` in the object's own frame, so it needs the
/// object's origin to be on the road. Each corner's top is `height` above its
/// base, up in the frame the corner is given in. An outline with a corner
/// that cannot be placed is dropped whole, because the polygon without it is
/// a different shape. So is one with fewer than two corners.
///
/// Also returns the stretch of road the outline spans: from its first
/// `<cornerRoad>` station to its last, taking in `s`, the object's own
/// station, if it has a `<cornerLocal>`.
fn outline(
    node: roxmltree::Node,
    frame: Option<&Frame>,
    s: f64,
    (shift_s, shift_t): (f64, f64),
    road: &BakedRoad,
) -> Option<(Vec<Corner>, bool, (f64, f64))> {
    let mut stretch = (f64::INFINITY, f64::NEG_INFINITY);
    let mut spans = |s: f64| stretch = (stretch.0.min(s), stretch.1.max(s));
    let corner = |c: roxmltree::Node| -> Option<Corner> {
        let height = attr_f64(c, "height").unwrap_or(0.0);
        if c.has_tag_name("cornerRoad") {
            let (s, t) = (attr_f64(c, "s")? + shift_s, attr_f64(c, "t")? + shift_t);
            if !road.on_road(s) {
                return None;
            }
            spans(s);
            let (mut base, _) = (road.surface)(s, t);
            base.z += attr_f64(c, "dz").unwrap_or(0.0) as f32;
            Some(Corner {
                base,
                top: base + Vector::Z * height as f32,
            })
        } else {
            let frame = frame?;
            spans(s);
            let (u, v) = (attr_f64(c, "u")?, attr_f64(c, "v")?);
            let z = attr_f64(c, "z").unwrap_or(0.0);
            Some(Corner {
                base: frame.point([u, v, z]),
                top: frame.point([u, v, z + height]),
            })
        }
    };
    let corners = corner_nodes(node).map(corner).collect::<Option<Vec<_>>>()?;
    if corners.len() < 2 {
        return None;
    }
    // Closed unless the file says otherwise, as libOpenDRIVE reads it.
    let closed = node.attribute("closed") != Some("false");
    Some((corners, closed, stretch))
}

/// Whether `point` is inside `ring` in plan, by the even-odd rule.
fn encloses(ring: &[Corner], point: Point) -> bool {
    let n = ring.len();
    (0..n).fold(false, |inside, i| {
        let (a, b) = (ring[i].base, ring[(i + 1) % n].base);
        let crosses = (a.y > point.y) != (b.y > point.y)
            && point.x < a.x + (point.y - a.y) * (b.x - a.x) / (b.y - a.y);
        inside != crosses
    })
}

/// An `<outline>`'s corners, in order.
fn corner_nodes<'a>(
    outline: roxmltree::Node<'a, 'a>,
) -> impl Iterator<Item = roxmltree::Node<'a, 'a>> {
    outline
        .children()
        .filter(|n| n.has_tag_name("cornerRoad") || n.has_tag_name("cornerLocal"))
}

/// The `<marking>`s under `object`'s `<markings>` that run along edges of
/// one of its outlines, `outline`, whose placed corners are `corners`.
///
/// A marking follows the corners its `<cornerReference>`s name, in order, by
/// the corners' `id`s. It belongs to the outline that has every one of them,
/// so one naming a corner the outline lacks is left to another outline. So
/// is one with fewer than two references, or none of any width.
///
/// The paint is raised `zOffset` off the corners' bases, 5 mm if the map does
/// not say, and starts `startOffset` along the first edge and stops
/// `stopOffset` short of the end of the last.
fn markings(object: roxmltree::Node, outline: roxmltree::Node, corners: &[Corner]) -> Vec<Marking> {
    let ids: Vec<Option<&str>> = corner_nodes(outline).map(|c| c.attribute("id")).collect();
    let Some(markings) = child(object, "markings") else {
        return Vec::new();
    };
    markings
        .children()
        .filter(|n| n.has_tag_name("marking"))
        .filter_map(|m| {
            let width = attr_f64(m, "width").filter(|w| *w > 0.0)?;
            let raise = Vector::Z * attr_f64(m, "zOffset").unwrap_or(0.005) as f32;
            let path = corner_path(m, &ids, corners)?
                .into_iter()
                .map(|p| p + raise)
                .collect::<Vec<_>>();
            let line = attr_f64(m, "lineLength").unwrap_or(0.0).max(0.0);
            let space = attr_f64(m, "spaceLength").unwrap_or(0.0).max(0.0);
            let text = |name| m.attribute(name).unwrap_or_default().to_string();
            Some(Marking {
                side: text("side"),
                color: text("color"),
                width: width as f32,
                line_length: line as f32,
                space_length: space as f32,
                pieces: strip(
                    &path,
                    attr_f64(m, "startOffset").unwrap_or(0.0),
                    attr_f64(m, "stopOffset").unwrap_or(0.0),
                    (line > 0.0 && space > 0.0).then_some((line, space)),
                    width / 2.0,
                ),
            })
        })
        .collect()
}

/// The `<border>`s under `object`'s `<borders>` that run along edges of one
/// of its outlines, `outline`, whose placed corners are `corners`.
///
/// A border belongs to the outline whose `id` is its `outlineId`, or with no
/// `outlineId`, to any outline that fits it. With `useCompleteOutline` it
/// runs along every edge, the closing one too if the outline is `closed`.
/// Otherwise it follows the corners its `<cornerReference>`s name, as a
/// [`Marking`] does. One with no width is skipped.
fn borders(
    object: roxmltree::Node,
    outline: roxmltree::Node,
    corners: &[Corner],
    closed: bool,
) -> Vec<Border> {
    let ids: Vec<Option<&str>> = corner_nodes(outline).map(|c| c.attribute("id")).collect();
    let Some(borders) = child(object, "borders") else {
        return Vec::new();
    };
    borders
        .children()
        .filter(|n| n.has_tag_name("border"))
        .filter(|b| {
            b.attribute("outlineId")
                .is_none_or(|id| outline.attribute("id") == Some(id))
        })
        .filter_map(|b| {
            let width = attr_f64(b, "width").filter(|w| *w > 0.0)?;
            let path = if b.attribute("useCompleteOutline") == Some("true") {
                let mut path: Vec<Point> = corners.iter().map(|c| c.base).collect();
                if closed {
                    path.push(corners[0].base);
                }
                path
            } else {
                corner_path(b, &ids, corners)?
            };
            Some(Border {
                kind: b.attribute("type").unwrap_or_default().to_string(),
                width: width as f32,
                pieces: strip(&path, 0.0, 0.0, None, width / 2.0),
            })
        })
        .collect()
}

/// The bases of the corners `node`'s `<cornerReference>`s name, in order.
/// `ids` are the corners' `id`s, in step with `corners`. `None` if a
/// reference names no corner in `ids`, or there are fewer than two.
fn corner_path(
    node: roxmltree::Node,
    ids: &[Option<&str>],
    corners: &[Corner],
) -> Option<Vec<Point>> {
    let path = node
        .children()
        .filter(|n| n.has_tag_name("cornerReference"))
        .map(|r| {
            let id = r.attribute("id")?;
            let at = ids.iter().position(|c| *c == Some(id))?;
            Some(corners[at].base)
        })
        .collect::<Option<Vec<_>>>()?;
    (path.len() >= 2).then_some(path)
}

/// A strip `half_width` either side of the line through `path`, from `start`
/// metres along it to `stop` metres short of its end: one quad per edge, or
/// with `dashes` of `(line, space)` metres, one per dash per edge. Each quad
/// goes anticlockwise seen from above. An edge with no length in plan has no
/// across to measure, and gets none. Nor does a pattern of more than
/// [`MAX_REPEAT_INSTANCES`] dashes, as a runaway `<repeat>` does not.
fn strip(
    path: &[Point],
    start: f64,
    stop: f64,
    dashes: Option<(f64, f64)>,
    half_width: f64,
) -> Vec<[Point; 4]> {
    let mut along = vec![0.0];
    for w in path.windows(2) {
        along.push(along.last().unwrap() + f64::from(w[0].distance_to(w[1])));
    }
    let end = along.last().unwrap() - stop;
    let painted: Vec<(f64, f64)> = match dashes {
        None => vec![(start, end)],
        Some((line, space)) if (end - start) / (line + space) > MAX_REPEAT_INSTANCES => Vec::new(),
        Some((line, space)) => (0..)
            .map(|k| start + k as f64 * (line + space))
            .take_while(|&a| a < end)
            .map(|a| (a, (a + line).min(end)))
            .collect(),
    };
    let mut pieces = Vec::new();
    for (i, w) in path.windows(2).enumerate() {
        let (a, b) = (w[0], w[1]);
        let d = b - a;
        let across = Vector::new(-d.y, d.x, 0.0).normalize_or_zero() * half_width as f32;
        if across == Vector::ZERO {
            continue;
        }
        let (from, to) = (along[i], along[i + 1]);
        for &(p, q) in &painted {
            let (p, q) = (p.max(from), q.min(to));
            if q - p < 1e-6 {
                continue;
            }
            let at = |x: f64| a.lerp(b, ((x - from) / (to - from)) as f32);
            let (p, q) = (at(p), at(q));
            pieces.push([p - across, q - across, q + across, p + across]);
        }
    }
    pieces
}

/// The volume a solid occupies: a cylinder if it has a radius, a box if it
/// has any of a length, a width or a height, and nothing otherwise. A
/// dimension the box does not have is 0.
fn extent(st: &Station) -> Option<Extent> {
    let height = st.height.unwrap_or(0.0) as f32;
    if let Some(radius) = st.radius {
        return Some(Extent::Cylinder {
            radius: radius as f32,
            height,
        });
    }
    if st.length.is_none() && st.width.is_none() && st.height.is_none() {
        return None;
    }
    Some(Extent::Box {
        length: st.length.unwrap_or(0.0) as f32,
        width: st.width.unwrap_or(0.0) as f32,
        height,
    })
}

/// The [`ObjectType`] an OpenDRIVE `<object>` `type` maps to. The deprecated
/// moving participants (`car`, `pedestrian` and the rest) are
/// [`ObjectType::Unknown`] along with every other unrecognised name.
fn object_type(od_type: Option<&str>) -> ObjectType {
    match od_type.unwrap_or_default() {
        "none" => ObjectType::None,
        "obstacle" => ObjectType::Obstacle,
        "pole" => ObjectType::Pole,
        "tree" => ObjectType::Tree,
        "vegetation" => ObjectType::Vegetation,
        "barrier" => ObjectType::Barrier,
        "building" => ObjectType::Building,
        "parkingSpace" => ObjectType::ParkingSpace,
        "patch" => ObjectType::Patch,
        "railing" => ObjectType::Railing,
        "trafficIsland" => ObjectType::TrafficIsland,
        "crosswalk" => ObjectType::Crosswalk,
        "streetLamp" => ObjectType::StreetLamp,
        "gantry" => ObjectType::Gantry,
        "soundBarrier" => ObjectType::SoundBarrier,
        "roadMark" => ObjectType::RoadMark,
        _ => ObjectType::Unknown,
    }
}

fn child<'a>(node: roxmltree::Node<'a, 'a>, tag: &str) -> Option<roxmltree::Node<'a, 'a>> {
    node.children().find(|n| n.has_tag_name(tag))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coords::Vector;

    // A straight 40 m road climbing at 4%, one right (forward) driving lane.
    const STRAIGHT: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="s" length="40.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="40.0"><line/></geometry>
    </planView>
    <elevationProfile>
      <elevation s="0.0" a="0.0" b="0.04" c="0.0" d="0.0"/>
    </elevationProfile>
    <lanes>
      <laneSection s="0.0">
        <right>
          <lane id="-1" type="driving">
            <width sOffset="0.0" a="3.5" b="0.0" c="0.0" d="0.0"/>
          </lane>
        </right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn straight_one_forward_lane() {
        let net = load_str(STRAIGHT).expect("import");
        assert_eq!(net.driving_lanes().count(), 1);
        let lane = net.lanes().first().unwrap();
        assert_eq!(lane.direction, Direction::Forward);
        let start = lane.center.pose_at(0.0);
        let end = lane.center.pose_at(lane.center.length());
        // Heads +X, the right lane sits on the -Y side, and it climbs.
        assert!(start.heading.x > 0.9, "start heading {:?}", start.heading);
        assert!(
            (start.position.y + 1.75).abs() < 0.1,
            "y {}",
            start.position.y
        );
        assert!(end.position.z > start.position.z + 1.0, "no climb");
    }

    // A straight then a 90-degree left arc (radius 30), two opposing lanes,
    // the same shape as the hand-authored demo_road.
    const STRAIGHT_ARC: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="sa" length="87.12" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="40.0"><line/></geometry>
      <geometry s="40.0" x="40.0" y="0.0" hdg="0.0" length="47.12"><arc curvature="0.03333"/></geometry>
    </planView>
    <lanes>
      <laneSection s="0.0">
        <left>
          <lane id="1" type="driving"><width sOffset="0.0" a="3.5"/></lane>
        </left>
        <right>
          <lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane>
        </right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn straight_then_left_arc_two_lanes() {
        let net = load_str(STRAIGHT_ARC).expect("import");
        assert_eq!(net.driving_lanes().count(), 2);
        // Forward lane = the right (negative-id) lane.
        let fwd = net
            .lanes()
            .iter()
            .find(|l| l.direction == Direction::Forward)
            .unwrap();
        let start = fwd.center.pose_at(0.0);
        let end = fwd.center.pose_at(fwd.center.length());
        assert!(start.heading.x > 0.9, "start {:?}", start.heading);
        // After a 90-degree left turn, heading points toward +Y.
        assert!(end.heading.y > 0.9, "end {:?}", end.heading);
    }

    #[test]
    fn lanes_sit_a_width_apart() {
        let net = load_str(STRAIGHT_ARC).expect("import");
        let fwd = net
            .lanes()
            .iter()
            .find(|l| l.direction == Direction::Forward)
            .unwrap();
        let bwd = net
            .lanes()
            .iter()
            .find(|l| l.direction == Direction::Backward)
            .unwrap();
        // On the straight, the two lane centers are ~one lane width apart.
        let a = fwd.center.point_at(10.0);
        let gap = (a - bwd.center.project(a).point).length();
        assert!((gap - 3.5).abs() < 0.2, "gap {gap}");
    }

    // A straight road placed away from the origin and pointed along +Y, so
    // every axis carries a distinct number and a transposed or negated axis
    // cannot hide. One right lane, 4 m wide, at a constant 3 m elevation.
    const PLACED: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="placed" length="30.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="100.0" y="50.0" hdg="1.5707963267948966" length="30.0"><line/></geometry>
    </planView>
    <elevationProfile>
      <elevation s="0.0" a="3.0" b="0.0" c="0.0" d="0.0"/>
    </elevationProfile>
    <lanes>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="4.0"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    // The baked frame is OpenDRIVE's own, so a point can be read straight off
    // the file. Heading is +Y, so left is -X and the right lane sits at +X.
    #[test]
    fn a_placed_road_bakes_to_its_opendrive_coordinates() {
        let net = load_str(PLACED).expect("import");
        let lane = &net.lanes()[0];

        // Reference line runs (100, 50) -> (100, 80); lane centre is t = -2,
        // which for heading +Y is 2 m toward +X. Elevation is a flat 3.
        for (s, want) in [
            (0.0, Point::new(102.0, 50.0, 3.0)),
            (15.0, Point::new(102.0, 65.0, 3.0)),
            (30.0, Point::new(102.0, 80.0, 3.0)),
        ] {
            let got = lane.center.point_at(s);
            assert!(
                (got - want).length() < 1e-3,
                "s={s}: baked {got:?}, want {want:?}"
            );
        }
        // Travel is along +Y and the surface is level.
        assert!(lane
            .center
            .pose_at(15.0)
            .heading
            .abs_diff_eq(Vector::Y, 1e-4));
        assert!(lane.sample_at(15.0).up.abs_diff_eq(Vector::Z, 1e-5));
    }

    // A straight road with a constant laneOffset of +2.0 (shifts the whole
    // cross-section left, toward +Y in the baked frame).
    const STRAIGHT_OFFSET: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="o" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lanes>
      <laneOffset s="0.0" a="2.0" b="0.0" c="0.0" d="0.0"/>
      <laneSection s="0.0">
        <right>
          <lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane>
        </right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn lane_offset_shifts_the_cross_section() {
        // Right lane without offset sits at y = -1.75; laneOffset +2.0 shifts
        // it left by 2.0 -> y = +0.25.
        let net = load_str(STRAIGHT_OFFSET).expect("import");
        let y = net.lanes()[0].center.pose_at(0.0).position.y;
        assert!((y - 0.25).abs() < 0.05, "y {y}");
    }

    // A driving lane outboard of a shoulder. The shoulder is a lane of its own,
    // and its width also pushes the driving lane out.
    const SHOULDER_INBOARD: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="s" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lanes>
      <laneSection s="0.0">
        <right>
          <lane id="-1" type="shoulder"><width sOffset="0.0" a="2.0"/></lane>
          <lane id="-2" type="driving"><width sOffset="0.0" a="3.0"/></lane>
        </right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn a_non_driving_inner_lane_still_offsets_the_driving_lane() {
        // Lane -2 sits outboard of the 2.0 m shoulder: its center is at
        // t = -(2.0 + 3.0/2) = -3.5, so y = -3.5. Skipping the shoulder's width
        // would leave it at -1.5.
        let net = load_str(SHOULDER_INBOARD).expect("import");
        assert_eq!(net.lanes().len(), 2, "the shoulder is a lane too");
        let driving = net.driving_lanes().next().expect("the driving lane");
        let y = driving.center.pose_at(0.0).position.y;
        assert!((y + 3.5).abs() < 0.05, "y {y}, expected -3.5");
        assert!(
            driving.neighbors.is_empty(),
            "a shoulder is not a lane you change into: {:?}",
            driving.neighbors
        );
    }

    // A straight road split into two lane sections at s=25. Each section's
    // lanes should become their own polyline spanning only that section.
    const TWO_SECTIONS: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="ms" length="40.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="40.0"><line/></geometry>
    </planView>
    <lanes>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
      <laneSection s="25.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn each_lane_section_becomes_its_own_lane() {
        let net = load_str(TWO_SECTIONS).expect("import");
        assert_eq!(net.driving_lanes().count(), 2, "one lane per section");
        let mut lens: Vec<f32> = net.lanes().iter().map(|l| l.center.length()).collect();
        lens.sort_by(|a, b| a.total_cmp(b));
        // Sections span [0,25] and [25,40] -> ~25 and ~15 m.
        assert!((lens[0] - 15.0).abs() < 1.0, "short section {}", lens[0]);
        assert!((lens[1] - 25.0).abs() < 1.0, "long section {}", lens[1]);
    }

    // A cross-section using most of the lane vocabulary: a sidewalk, a kerb, a
    // parking lane and an unnamed strip outboard of two running lanes, a median
    // between the two directions, a vendor-specific type, and a type that is
    // not in the format at all.
    const MANY_TYPES: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="m" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lanes>
      <laneSection s="0.0">
        <left>
          <lane id="1" type="median"><width sOffset="0.0" a="1.0"/></lane>
          <lane id="2" type="special1"><width sOffset="0.0" a="1.0"/></lane>
          <lane id="3" type="driving"><width sOffset="0.0" a="3.5"/></lane>
          <lane id="4" type="banana"><width sOffset="0.0" a="1.5"/></lane>
        </left>
        <right>
          <lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane>
          <lane id="-2" type="onRamp"><width sOffset="0.0" a="3.0"/></lane>
          <lane id="-3" type="none"><width sOffset="0.0" a="0.5"/></lane>
          <lane id="-4" type="parking"><width sOffset="0.0" a="2.5"/></lane>
          <lane id="-5" type="curb"><width sOffset="0.0" a="0.3"/></lane>
          <lane id="-6" type="sidewalk"><width sOffset="0.0" a="2.0"/></lane>
        </right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn every_named_lane_type_bakes_as_itself() {
        let (net, Provenance { lanes: prov, .. }) =
            load_str_with_provenance(MANY_TYPES).expect("import");
        let kind = |od_id: i32| {
            prov.iter()
                .find(|p| p.od_id == od_id)
                .and_then(|p| net.lane(p.lane))
                .map(|l| l.kind)
        };
        assert_eq!(kind(1), Some(LaneType::Median));
        assert_eq!(kind(3), Some(LaneType::Driving));
        assert_eq!(kind(-1), Some(LaneType::Driving));
        assert_eq!(kind(-2), Some(LaneType::OnRamp));
        assert_eq!(kind(-3), Some(LaneType::None), "`none` is paved surface");
        assert_eq!(kind(-4), Some(LaneType::Parking));
        assert_eq!(kind(-5), Some(LaneType::Curb));
        assert_eq!(kind(-6), Some(LaneType::Sidewalk));
        assert_eq!(kind(2), Some(LaneType::Special1));
        assert_eq!(kind(4), Some(LaneType::Unknown), "banana is not a type");
        // Every lane in the section, with nothing dropped for its type.
        assert_eq!(net.lanes().len(), 10);
        assert_eq!(net.driving_lanes().count(), 2);
    }

    // A gore area, as CARLA writes one: a lane that starts as a point and opens
    // out cubically, then holds a constant width. Town07's road 64 lane -5 and
    // road 17 lane -2 are both this shape.
    const TAPERED: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="t" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lanes>
      <laneSection s="0.0">
        <right>
          <lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane>
          <lane id="-2" type="none">
            <width sOffset="0.0" a="0.0" b="0.0" c="0.04" d="0.0"/>
            <width sOffset="10.0" a="4.0" b="0.0" c="0.0" d="0.0"/>
          </lane>
        </right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn a_lane_that_opens_out_of_nothing_keeps_its_width_along_its_length() {
        // The width runs 0 -> 4 m. Reporting the width at s=0 as the lane's
        // width made the whole strip 0 m wide.
        let (net, Provenance { lanes: prov, .. }) =
            load_str_with_provenance(TAPERED).expect("import");
        let lane = prov
            .iter()
            .find(|p| p.od_id == -2)
            .and_then(|p| net.lane(p.lane))
            .expect("the tapered lane");

        assert!((lane.width - 4.0).abs() < 0.01, "nominal {}", lane.width);
        assert!(
            lane.width_at(0.0) < 0.01,
            "it starts as a point: {}",
            lane.width_at(0.0)
        );
        assert!(
            (lane.width_at(lane.center.length()) - 4.0).abs() < 0.01,
            "and ends at full width: {}",
            lane.width_at(lane.center.length())
        );
        // The profile grows the whole way, following the cubic rather than
        // stepping between the two width records.
        let mut last = -1.0;
        for k in 0..=20 {
            let w = lane.width_at(lane.center.length() * k as f32 / 20.0);
            assert!(w >= last - 1e-4, "width dipped at step {k}: {last} -> {w}");
            last = w;
        }
    }

    #[test]
    fn a_tapered_lane_tessellates_to_a_wedge_not_a_sliver() {
        // What the viewer showed. The lane was in the lane list, it had a span
        // in the mesh, and it covered no area.
        let (net, Provenance { lanes: prov, .. }) =
            load_str_with_provenance(TAPERED).expect("import");
        let id = prov
            .iter()
            .find(|p| p.od_id == -2)
            .expect("provenance")
            .lane;
        let mesh = net.surface_mesh();
        let span = mesh
            .lanes
            .iter()
            .find(|s| s.lane == id)
            .expect("the lane owns a mesh slice");

        let ribs: Vec<f32> = mesh.vertices
            [span.vertices.start as usize..span.vertices.end as usize]
            .chunks_exact(2)
            .map(|p| (p[0] - p[1]).length())
            .collect();
        assert!(ribs.first().copied().unwrap_or(1.0) < 0.01, "starts sharp");
        assert!(
            ribs.last().copied().unwrap_or(0.0) > 3.9,
            "opens to full width: {:?}",
            ribs.last()
        );
    }

    #[test]
    fn an_unrecognised_lane_type_is_a_lane_rather_than_a_hole() {
        // An unrecognised type still describes a real strip of surface. It
        // bakes with its geometry intact, under a name that says the type was
        // not understood.
        let (net, Provenance { lanes: prov, .. }) =
            load_str_with_provenance(MANY_TYPES).expect("import");
        let lane = prov
            .iter()
            .find(|p| p.od_id == 4)
            .and_then(|p| net.lane(p.lane))
            .expect("the unrecognised lane is baked");
        assert_eq!(lane.kind, LaneType::Unknown);
        assert!(
            !lane.kind.is_drivable(),
            "and nothing may be routed onto it"
        );
        assert!((lane.width - 1.5).abs() < 1e-6, "width {}", lane.width);
        assert!(lane.center.length() > 19.0, "it has the road's length");
    }

    #[test]
    fn inner_lane_widths_accumulate_across_the_cross_section() {
        // Left side, center outward: median 1.0, special1 1.0, driving 3.5. The
        // driving lane's center is at t = +(1.0 + 1.0 + 3.5/2) = +3.75. Missing
        // either inner width would leave it at +2.75 or nearer.
        let (net, Provenance { lanes: prov, .. }) =
            load_str_with_provenance(MANY_TYPES).expect("import");
        let driving = prov
            .iter()
            .find(|p| p.od_id == 3)
            .and_then(|p| net.lane(p.lane))
            .expect("the left driving lane");
        let y = driving.center.pose_at(0.0).position.y;
        assert!((y - 3.75).abs() < 0.05, "y {y}, expected 3.75");
    }

    #[test]
    fn lane_change_edges_stop_at_the_first_lane_traffic_cannot_use() {
        let (net, Provenance { lanes: prov, .. }) =
            load_str_with_provenance(MANY_TYPES).expect("import");
        let lane = |od_id: i32| {
            prov.iter()
                .find(|p| p.od_id == od_id)
                .and_then(|p| net.lane(p.lane))
                .expect("a baked lane")
        };
        // Right side: driving -1 and the onRamp -2 beside it are a lane change
        // apart; the parking lane past them is not, and neither is the sidewalk.
        let (driving, ramp) = (lane(-1), lane(-2));
        assert_eq!(driving.neighbors, vec![ramp.id]);
        assert_eq!(ramp.neighbors, vec![driving.id]);
        assert!(
            lane(-4).neighbors.is_empty(),
            "parking is not a lane change"
        );
        assert!(lane(-6).neighbors.is_empty(), "sidewalk is not either");
        assert!(lane(-3).neighbors.is_empty(), "nor is unnamed surface");
        // Left side: nothing drivable sits beside the only driving lane, so it
        // has nothing to change into at all.
        assert!(lane(3).neighbors.is_empty(), "a median is not crossable");
    }

    #[test]
    fn provenance_names_each_baked_lane_by_road_section_and_od_id() {
        let (net, Provenance { lanes: prov, .. }) =
            load_str_with_provenance(TWO_SECTIONS).expect("import with provenance");
        assert_eq!(prov.len(), net.lanes().len(), "one record per baked lane");
        // Every record points at a real lane, and names the source road/lane.
        for p in &prov {
            assert!(net.lane(p.lane).is_some(), "{:?} names no lane", p.lane);
            assert_eq!(p.road_id, "1");
            assert_eq!(p.od_id, -1);
        }
        // The two sections are distinguished, 0 and 1.
        let mut sections: Vec<usize> = prov.iter().map(|p| p.section).collect();
        sections.sort_unstable();
        assert_eq!(sections, vec![0, 1], "one lane per section, indexed 0,1");
    }

    // A pure clothoid: curvStart 0, curvEnd 0.1 over 10 m. End heading is the
    // closed form 0.5*c_dot*L^2 = 0.5*(0.01)*100 = 0.5 rad (a left turn -> +Y).
    const SPIRAL_ONLY: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="sp" length="10.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="10.0">
        <spiral curvStart="0.0" curvEnd="0.1"/>
      </geometry>
    </planView>
    <lanes>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn spiral_heading_matches_closed_form() {
        // Sampled at s=5 (interior, where the polyline's interpolated tangent is
        // accurate, since the very endpoint tangent is a last-segment artifact).
        // Reference heading there = 0.5*c_dot*s^2 = 0.5*0.01*25 = 0.125 rad.
        let net = load_str(SPIRAL_ONLY).expect("import");
        let h = net.lanes()[0].center.pose_at(5.0).heading;
        let theta = h.y.atan2(h.x);
        assert!((theta - 0.125).abs() < 0.03, "mid heading angle {theta}");
    }

    // A normalized paramPoly3: u(p)=10p, v(p)=5p^2 for p in [0,1], so the curve
    // runs from OD (0,0) to OD (10,5), which is the baked frame unchanged, so
    // the end lands near y=+5. The road is longer than the curve (arc length
    // ~11.5), so the end clamps to that point. A straight line (v ignored)
    // would end at y=0.
    const PARAM_POLY3: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="pp" length="15.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="15.0">
        <paramPoly3 pRange="normalized" aU="0" bU="10" cU="0" dU="0" aV="0" bV="0" cV="5" dV="0"/>
      </geometry>
    </planView>
    <lanes>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn param_poly3_follows_the_v_deviation() {
        let net = load_str(PARAM_POLY3).expect("import");
        let end = net.lanes()[0]
            .center
            .pose_at(net.lanes()[0].center.length())
            .position;
        assert!(end.x > 8.0, "end x {}", end.x);
        assert!(end.y > 3.0, "end y {} (should follow v to ~+5)", end.y);
    }

    // poly3 with v(u)=0.05*u^2 over length 10, curving laterally toward +y
    // (like the arcLength paramPoly3 case; exact endpoint depends on the
    // arc-length reparametrization, so just assert a clear deviation).
    const POLY3: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="p3" length="10.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="10.0">
        <poly3 a="0" b="0" c="0.05" d="0"/>
      </geometry>
    </planView>
    <lanes>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn poly3_curves_laterally() {
        let net = load_str(POLY3).expect("import");
        let end = net.lanes()[0]
            .center
            .pose_at(net.lanes()[0].center.length())
            .position;
        assert!(end.y > 2.0, "end y {} (poly3 should curve)", end.y);
    }

    // Two lane sections in one road, linked lane -1 -> lane -1.
    const TWO_SECTION_LINKED: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="r" length="40.0" id="1" junction="-1">
    <planView><geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="40.0"><line/></geometry></planView>
    <lanes>
      <laneSection s="0.0"><right><lane id="-1" type="driving">
        <link><successor id="-1"/></link><width sOffset="0.0" a="3.5"/>
      </lane></right></laneSection>
      <laneSection s="15.0"><right><lane id="-1" type="driving">
        <link><predecessor id="-1"/></link><width sOffset="0.0" a="3.5"/>
      </lane></right></laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn cross_section_link() {
        let net = load_str(TWO_SECTION_LINKED).expect("import");
        assert_eq!(net.lanes().len(), 2);
        assert_eq!(net.lanes()[0].successors, vec![net.lanes()[1].id]);
        assert_eq!(net.lanes()[1].predecessors, vec![net.lanes()[0].id]);
    }

    // Road 1 -> road 2 (its successor), each a single forward lane.
    const TWO_ROADS_LINKED: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="a" length="20.0" id="1" junction="-1">
    <link><successor elementType="road" elementId="2" contactPoint="start"/></link>
    <planView><geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry></planView>
    <lanes><laneSection s="0.0"><right><lane id="-1" type="driving">
      <link><successor id="-1"/></link><width sOffset="0.0" a="3.5"/>
    </lane></right></laneSection></lanes>
  </road>
  <road name="b" length="20.0" id="2" junction="-1">
    <link><predecessor elementType="road" elementId="1" contactPoint="end"/></link>
    <planView><geometry s="0.0" x="20.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry></planView>
    <lanes><laneSection s="0.0"><right><lane id="-1" type="driving">
      <link><predecessor id="-1"/></link><width sOffset="0.0" a="3.5"/>
    </lane></right></laneSection></lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn road_to_road_link() {
        let net = load_str(TWO_ROADS_LINKED).expect("import");
        assert_eq!(net.lanes().len(), 2);
        assert_eq!(net.lanes()[0].successors, vec![net.lanes()[1].id], "A -> B");
        assert_eq!(net.lanes()[1].predecessors, vec![net.lanes()[0].id]);
    }

    // Road 1 -> junction 100 -> connecting road 2, via a laneLink.
    const JUNCTION: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="in" length="20.0" id="1" junction="-1">
    <link><successor elementType="junction" elementId="100"/></link>
    <planView><geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry></planView>
    <lanes><laneSection s="0.0"><right><lane id="-1" type="driving">
      <link><successor id="-1"/></link><width sOffset="0.0" a="3.5"/>
    </lane></right></laneSection></lanes>
  </road>
  <road name="conn" length="20.0" id="2" junction="100">
    <link><predecessor elementType="road" elementId="1" contactPoint="end"/></link>
    <planView><geometry s="0.0" x="20.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry></planView>
    <lanes><laneSection s="0.0"><right><lane id="-1" type="driving">
      <width sOffset="0.0" a="3.5"/>
    </lane></right></laneSection></lanes>
  </road>
  <junction id="100">
    <connection id="0" incomingRoad="1" connectingRoad="2" contactPoint="start">
      <laneLink from="-1" to="-1"/>
    </connection>
  </junction>
</OpenDRIVE>"#;

    #[test]
    fn junction_link() {
        let net = load_str(JUNCTION).expect("import");
        assert_eq!(net.lanes().len(), 2);
        assert_eq!(
            net.lanes()[0].successors,
            vec![net.lanes()[1].id],
            "road -> junction -> connecting road"
        );
    }

    #[test]
    fn empty_or_junk_is_an_error() {
        assert!(load_str("<OpenDRIVE></OpenDRIVE>").is_err());
        assert!(load_str("not xml at all <<<").is_err());
    }

    // A flat straight (no lateralProfile) must leave the bank profile empty,
    // the "flat lane" sentinel, so imported flat roads are unchanged.
    #[test]
    fn no_lateral_profile_leaves_bank_empty() {
        let net = load_str(STRAIGHT).expect("import");
        assert!(
            net.lanes()[0].bank.is_empty(),
            "a flat road carries no bank"
        );
        assert_eq!(net.lanes()[0].bank_at(10.0), 0.0);
    }

    // A straight, level road banked at a constant 0.1 rad, one lane each side.
    // Reference-line pivot: the left (+t) lane rides up by ~t·sin φ, the right
    // (−t) lane drops by the same, and both read the same bank angle φ.
    const SUPERELEV_CONST: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="se" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lateralProfile>
      <superelevation s="0.0" a="0.1" b="0.0" c="0.0" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneSection s="0.0">
        <left><lane id="1" type="driving"><width sOffset="0.0" a="3.5"/></lane></left>
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn constant_superelevation_pivots_about_the_reference_line() {
        let net = load_str(SUPERELEV_CONST).expect("import");
        let left = net
            .lanes()
            .iter()
            .find(|l| l.direction == Direction::Backward)
            .expect("a left lane");
        let right = net
            .lanes()
            .iter()
            .find(|l| l.direction == Direction::Forward)
            .expect("a right lane");

        // Both lanes read the surface roll angle, ~0.1 rad, everywhere.
        assert!(
            (left.bank_at(10.0) - 0.1).abs() < 1e-4,
            "{}",
            left.bank_at(10.0)
        );
        assert!(
            (right.bank_at(10.0) - 0.1).abs() < 1e-4,
            "{}",
            right.bank_at(10.0)
        );
        assert!(!left.bank.is_empty(), "a banked lane carries a profile");

        // Lane centers sit half a lane-width off the reference (t = ±1.75), so
        // the pivot raises the left by 1.75·sin0.1 and drops the right likewise.
        let expect = 1.75 * 0.1_f32.sin();
        let ly = left.center.point_at(10.0).z;
        let ry = right.center.point_at(10.0).z;
        assert!((ly - expect).abs() < 0.02, "left z {ly}, want {expect}");
        assert!((ry + expect).abs() < 0.02, "right z {ry}, want {}", -expect);
        // Positive φ raises the left edge: left above right.
        assert!(ly > ry, "left {ly} should ride above right {ry}");

        // Horizontal offset shrinks by cos φ (the lane leans in, not straight
        // out): |y| a touch under 1.75.
        let ly = left.center.point_at(10.0).y.abs();
        assert!(ly < 1.75 && ly > 1.75 * 0.1_f32.cos() - 0.02, "y {ly}");
    }

    // Superelevation ramping in along s: φ(s) = 0.01·s, so the bank grows.
    const SUPERELEV_RAMP: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="ser" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lateralProfile>
      <superelevation s="0.0" a="0.0" b="0.01" c="0.0" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn ramped_superelevation_grows_along_s() {
        let net = load_str(SUPERELEV_RAMP).expect("import");
        let lane = &net.lanes()[0];
        // φ(5)=0.05, φ(15)=0.15, a monotonic increase matching the cubic.
        assert!(
            (lane.bank_at(5.0) - 0.05).abs() < 5e-3,
            "{}",
            lane.bank_at(5.0)
        );
        assert!(
            (lane.bank_at(15.0) - 0.15).abs() < 5e-3,
            "{}",
            lane.bank_at(15.0)
        );
        assert!(lane.bank_at(15.0) > lane.bank_at(5.0));
    }

    // Two left lanes (id 1 inner, id 2 outer) on a road banked +0.1 rad. The
    // headline reference-line-pivot case: the outer lane, further from the
    // reference line, rides measurably higher than the inner one.
    const SUPERELEV_TWO_LEFT: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="se2" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lateralProfile>
      <superelevation s="0.0" a="0.1" b="0.0" c="0.0" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneSection s="0.0">
        <left>
          <lane id="1" type="driving"><width sOffset="0.0" a="3.5"/></lane>
          <lane id="2" type="driving"><width sOffset="0.0" a="3.5"/></lane>
        </left>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn outer_lane_of_a_banked_road_rides_higher() {
        let net = load_str(SUPERELEV_TWO_LEFT).expect("import");
        // Inner lane center t = 1.75, outer t = 5.25 (one full width further out).
        // Both climb by t·sin0.1; the outer sits ~3.5·sin0.1 ≈ 0.35 m above.
        let mut zs: Vec<f32> = net
            .lanes()
            .iter()
            .map(|l| l.center.point_at(10.0).z)
            .collect();
        zs.sort_by(|a, b| a.total_cmp(b));
        let inner_z = 1.75 * 0.1_f32.sin();
        let outer_z = 5.25 * 0.1_f32.sin();
        assert!((zs[0] - inner_z).abs() < 0.02, "inner z {}", zs[0]);
        assert!((zs[1] - outer_z).abs() < 0.02, "outer z {}", zs[1]);
        assert!(
            zs[1] - zs[0] > 0.3,
            "outer lane {} should ride well above inner {}",
            zs[1],
            zs[0]
        );
    }

    // Negative superelevation rolls the other way: the right (−t) edge lifts and
    // the left (+t) edge drops, the mirror of the positive case, pinning sign.
    const SUPERELEV_NEG: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="sen" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lateralProfile>
      <superelevation s="0.0" a="-0.1" b="0.0" c="0.0" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneSection s="0.0">
        <left><lane id="1" type="driving"><width sOffset="0.0" a="3.5"/></lane></left>
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn negative_superelevation_raises_the_right_edge() {
        let net = load_str(SUPERELEV_NEG).expect("import");
        let left = net
            .lanes()
            .iter()
            .find(|l| l.direction == Direction::Backward)
            .expect("a left lane");
        let right = net
            .lanes()
            .iter()
            .find(|l| l.direction == Direction::Forward)
            .expect("a right lane");
        // φ = −0.1: the right lane now rides above the left (mirror of +φ).
        assert!(
            right.center.point_at(10.0).z > left.center.point_at(10.0).z,
            "right {} should ride above left {} for negative bank",
            right.center.point_at(10.0).z,
            left.center.point_at(10.0).z
        );
        // The stored angle carries the sign.
        assert!(
            (left.bank_at(10.0) + 0.1).abs() < 1e-4,
            "{}",
            left.bank_at(10.0)
        );
    }

    // ===================================================================
    // Stress the reference-line pivot beyond the straight, level,
    // single-offset cases above.
    // ===================================================================

    // A straight-then-90-degree-left-arc road (the STRAIGHT_ARC shape) with a
    // constant 0.15 rad superelevation. Bank must bake finite, sane geometry all
    // the way around the curve and read the surface roll at any station.
    const SUPERELEV_ARC: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="sa" length="87.12" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="40.0"><line/></geometry>
      <geometry s="40.0" x="40.0" y="0.0" hdg="0.0" length="47.12"><arc curvature="0.03333"/></geometry>
    </planView>
    <lateralProfile>
      <superelevation s="0.0" a="0.15" b="0.0" c="0.0" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneSection s="0.0">
        <left><lane id="1" type="driving"><width sOffset="0.0" a="3.5"/></lane></left>
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn superelevation_on_a_banked_arc() {
        let net = load_str(SUPERELEV_ARC).expect("import");
        let left = net
            .lanes()
            .iter()
            .find(|l| l.direction == Direction::Backward)
            .expect("a left lane");
        let right = net
            .lanes()
            .iter()
            .find(|l| l.direction == Direction::Forward)
            .expect("a right lane");

        // Every baked centerline point is finite (no NaN/inf from the arc math).
        for lane in [left, right] {
            assert!(!lane.bank.is_empty(), "banked lane carries a profile");
            for &b in &lane.bank {
                assert!(b.is_finite(), "bank entry not finite: {b}");
            }
            for p in lane.center.points() {
                assert!(p.is_finite(), "centerline point not finite: {p:?}");
            }
        }

        // Read the roll at several stations, including well into the arc
        // (s=20 straight, s=60 arc, near the very end). Constant profile => 0.15
        // everywhere, on both lanes.
        for &s in &[0.0_f32, 20.0, 60.0, 85.0] {
            assert!(
                (left.bank_at(s) - 0.15).abs() < 1e-3,
                "left bank_at({s}) = {}",
                left.bank_at(s)
            );
            assert!(
                (right.bank_at(s) - 0.15).abs() < 1e-3,
                "right bank_at({s}) = {}",
                right.bank_at(s)
            );
        }

        // Reference-line pivot: with constant t (=+/-1.75) and constant phi, the
        // banked height is constant along the whole road, arc included. The left
        // (+t) lane rides up by 1.75*sin0.15, the right (-t) down by the same.
        let expect = 1.75 * 0.15_f32.sin();
        for &s in &[20.0_f32, 60.0, 85.0] {
            let ly = left.center.point_at(s).z;
            let ry = right.center.point_at(s).z;
            assert!(
                (ly - expect).abs() < 0.05,
                "left z@{s} = {ly}, want {expect}"
            );
            assert!((ry + expect).abs() < 0.05, "right z@{s} = {ry}");
            assert!(ly > ry, "left {ly} should ride above right {ry} @ s={s}");
        }
    }

    // End-to-end: importing a banked `.xodr` and calling `sample_near` on the
    // imported network yields a sane RoadSample on both the straight and the
    // arc portion of the banked road: correct bank, and a unit up-normal that
    // points up and carries lateral cant only (up.heading == 0).
    #[test]
    fn sample_near_on_an_imported_banked_road_is_sane() {
        let net = load_str(SUPERELEV_ARC).expect("import");
        let left = net
            .lanes()
            .iter()
            .find(|l| l.direction == Direction::Backward)
            .expect("a left lane");

        // Query at points sitting on the left lane's own centerline: s=20 (the
        // straight) and s=60 (well into the arc). sample_near must land back on
        // that lane and report its ~0.15 rad bank.
        for &s in &[20.0_f32, 60.0] {
            let on_lane = left.center.point_at(s);
            let rs = net.sample_near(on_lane).expect("a sample near the road");
            assert!(rs.point.is_finite(), "sample point not finite @ s={s}");
            assert!(
                (rs.bank.abs() - 0.15).abs() < 1e-2,
                "bank @ s={s} = {} (want |0.15|)",
                rs.bank
            );
            // Unit up-normal, pointing up, lateral cant only.
            assert!(
                (rs.up.length() - 1.0).abs() < 1e-4,
                "up not unit @ s={s}: {:?}",
                rs.up
            );
            assert!(rs.up.z > 0.9, "up.z too low @ s={s}: {}", rs.up.z);
            assert!(
                rs.up.dot(rs.heading).abs() < 1e-5,
                "up.heading @ s={s} = {}",
                rs.up.dot(rs.heading)
            );
            // The sample landed close to where we queried (same lane).
            assert!(
                (rs.point - on_lane).length() < 0.5,
                "sample drifted from the query @ s={s}: {:?} vs {on_lane:?}",
                rs.point
            );
        }
    }

    // Superelevation composed with an elevation grade: the road climbs at 4% AND
    // banks at 0.1 rad. A lane's height must be grade(s) + t*sin(phi). The two
    // add, and neither clobbers the other.
    const SUPERELEV_PLUS_GRADE: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="sg" length="40.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="40.0"><line/></geometry>
    </planView>
    <elevationProfile>
      <elevation s="0.0" a="0.0" b="0.04" c="0.0" d="0.0"/>
    </elevationProfile>
    <lateralProfile>
      <superelevation s="0.0" a="0.1" b="0.0" c="0.0" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneSection s="0.0">
        <left><lane id="1" type="driving"><width sOffset="0.0" a="3.5"/></lane></left>
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn superelevation_composes_with_elevation_grade() {
        let net = load_str(SUPERELEV_PLUS_GRADE).expect("import");
        let left = net
            .lanes()
            .iter()
            .find(|l| l.direction == Direction::Backward)
            .expect("a left lane");
        let right = net
            .lanes()
            .iter()
            .find(|l| l.direction == Direction::Forward)
            .expect("a right lane");
        let cant = 1.75 * 0.1_f32.sin();
        for &s in &[10.0_f32, 20.0, 30.0] {
            let grade = 0.04 * s; // elevation cubic: a=0, b=0.04
                                  // Left (+t) rides above the grade line, right (-t) below it, by the
                                  // same cant, so the grade is the midline of the two.
            let ly = left.center.point_at(s).z;
            let ry = right.center.point_at(s).z;
            assert!(
                (ly - (grade + cant)).abs() < 0.02,
                "left z@{s} = {ly}, want {}",
                grade + cant
            );
            assert!(
                (ry - (grade - cant)).abs() < 0.02,
                "right z@{s} = {ry}, want {}",
                grade - cant
            );
            // The mean of the two lanes recovers the grade (bank cancels).
            assert!(((ly + ry) / 2.0 - grade).abs() < 0.02, "grade midline @{s}");
        }
    }

    // Superelevation + laneOffset: the +2.0 laneOffset shifts the whole
    // cross-section, and the pivot must apply to the *shifted* t. Right lane -1
    // (own offset -1.75) with laneOffset +2.0 lands at t = +0.25, so its height
    // is the small POSITIVE 0.25*sin(phi), not -1.75*sin(phi) (own offset only)
    // nor +2.0*sin(phi) (laneOffset only).
    const SUPERELEV_PLUS_OFFSET: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="so" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lateralProfile>
      <superelevation s="0.0" a="0.1" b="0.0" c="0.0" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneOffset s="0.0" a="2.0" b="0.0" c="0.0" d="0.0"/>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn superelevation_pivots_about_the_offset_shifted_cross_section() {
        let net = load_str(SUPERELEV_PLUS_OFFSET).expect("import");
        let lane = &net.lanes()[0];
        let t = 2.0 - 1.75; // laneOffset + own (right) offset = +0.25
        let want_z = t * 0.1_f32.sin();
        let z = lane.center.point_at(10.0).z;
        assert!(
            (z - want_z).abs() < 5e-3,
            "z = {z}, want {want_z} (pivot on shifted t=+0.25, not own -1.75 nor offset +2.0)"
        );
        // Height is clearly positive: had the pivot used the lane's own -1.75,
        // it would be negative (~ -0.175).
        assert!(
            z > 0.0,
            "shifted t is +0.25 -> height must be positive, got {z}"
        );
        // Horizontal reach shrinks by cos(phi): |y| = t*cos0.1 ~= 0.2487.
        let y = lane.center.point_at(10.0).y;
        assert!(
            (y - t * 0.1_f32.cos()).abs() < 5e-3,
            "y = {y}, want {}",
            t * 0.1_f32.cos()
        );
    }

    // Importing the same banked map twice must yield byte-identical bank vectors
    // and centerline points (no ordering / float nondeterminism).
    #[test]
    fn banked_import_is_deterministic() {
        let a = load_str(SUPERELEV_ARC).expect("import a");
        let b = load_str(SUPERELEV_ARC).expect("import b");
        assert_eq!(a.lanes().len(), b.lanes().len());
        for (la, lb) in a.lanes().iter().zip(b.lanes().iter()) {
            assert_eq!(la.bank, lb.bank, "bank vectors differ between imports");
            assert_eq!(
                la.center.points(),
                lb.center.points(),
                "centerline points differ between imports"
            );
        }
    }

    // A very large roll near pi/2: sin ~= 1 (height ~= t), cos ~= 0 (horizontal
    // reach collapses). Must stay finite and read back the angle, with no blow-up.
    const SUPERELEV_STEEP: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="st" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lateralProfile>
      <superelevation s="0.0" a="1.5" b="0.0" c="0.0" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn steep_superelevation_stays_finite() {
        let net = load_str(SUPERELEV_STEEP).expect("import");
        let lane = &net.lanes()[0];
        assert!(
            (lane.bank_at(10.0) - 1.5).abs() < 1e-3,
            "{}",
            lane.bank_at(10.0)
        );
        let p = lane.center.point_at(10.0);
        assert!(p.is_finite(), "point not finite at steep bank: {p:?}");
        // t = -1.75; height ~= -1.75*sin1.5 ~= -1.746, |y| ~= 1.75*cos1.5 ~= 0.124.
        assert!((p.z - (-1.75 * 1.5_f32.sin())).abs() < 0.02, "z {}", p.z);
        assert!(
            p.y.abs() < 0.2,
            "horizontal reach should collapse, y {}",
            p.y
        );
    }

    // An explicit zero superelevation (a="0.0") must still collapse to the empty
    // "flat lane" sentinel, exactly like no lateralProfile at all.
    const SUPERELEV_EXPLICIT_ZERO: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="sz" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lateralProfile>
      <superelevation s="0.0" a="0.0" b="0.0" c="0.0" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn explicit_zero_superelevation_collapses_to_empty() {
        let net = load_str(SUPERELEV_EXPLICIT_ZERO).expect("import");
        assert!(
            net.lanes()[0].bank.is_empty(),
            "an all-zero profile must collapse to the flat sentinel"
        );
        assert_eq!(net.lanes()[0].bank_at(10.0), 0.0);
        // And its centerline height is pure horizontal (y = -1.75, z = 0).
        let p = net.lanes()[0].center.point_at(10.0);
        assert!(p.z.abs() < 1e-4, "flat road, z {}", p.z);
    }

    // A superelevation record that starts at s=10 on a 20 m road: stations before
    // s=10 fall back to 0 (no active record), stations after read the profile.
    const SUPERELEV_LATE_START: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="sl" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lateralProfile>
      <superelevation s="10.0" a="0.1" b="0.0" c="0.0" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn superelevation_starting_late_leaves_early_stations_flat() {
        let net = load_str(SUPERELEV_LATE_START).expect("import");
        let lane = &net.lanes()[0];
        // Profile is kept (later stations are banked), not collapsed.
        assert!(!lane.bank.is_empty());
        // Before the record: flat.
        assert!(
            lane.bank_at(2.0).abs() < 1e-4,
            "early bank {}",
            lane.bank_at(2.0)
        );
        assert!(
            lane.center.point_at(2.0).z.abs() < 1e-3,
            "early height should be flat, z {}",
            lane.center.point_at(2.0).z
        );
        // After the record starts: banked ~0.1.
        assert!(
            (lane.bank_at(18.0) - 0.1).abs() < 1e-3,
            "late bank {}",
            lane.bank_at(18.0)
        );
        assert!(
            lane.center.point_at(18.0).z < -0.1,
            "late height should be banked (t=-1.75), z {}",
            lane.center.point_at(18.0).z
        );
    }

    /// A straight 20 m road along +X, banked a constant 0.1 rad, carrying
    /// `objects` as its `<objects>` children.
    fn banked_road_with(objects: &str) -> String {
        format!(
            r#"<OpenDRIVE>
  <road name="o" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lateralProfile>
      <superelevation s="0.0" a="0.1" b="0.0" c="0.0" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
    <objects>{objects}</objects>
  </road>
</OpenDRIVE>"#
        )
    }

    /// A road of two lane sections, split at s = 10, each with lanes 1, -1
    /// and -2.
    fn two_section_road_with(objects: &str) -> String {
        let section = |s| {
            format!(
                r#"<laneSection s="{s}">
        <left><lane id="1" type="driving"><width sOffset="0.0" a="3"/></lane></left>
        <right>
          <lane id="-1" type="driving"><width sOffset="0.0" a="3"/></lane>
          <lane id="-2" type="sidewalk"><width sOffset="0.0" a="2"/></lane>
        </right>
      </laneSection>"#
            )
        };
        format!(
            r#"<OpenDRIVE>
  <road name="o" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lanes>{}{}</lanes>
    <objects>{objects}</objects>
  </road>
</OpenDRIVE>"#,
            section("0.0"),
            section("10.0")
        )
    }

    /// The `(section, <lane id>)` of each lane each object applies to.
    fn object_lanes(xml: &str) -> Vec<Vec<(usize, i32)>> {
        let (net, prov) = load_str_with_provenance(xml).expect("import");
        net.objects()
            .iter()
            .map(|o| {
                o.lanes
                    .iter()
                    .map(|id| {
                        let p = prov.lanes.iter().find(|p| p.lane == *id).unwrap();
                        (p.section, p.od_id)
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn an_object_applies_to_the_lanes_of_the_sections_it_spans() {
        let xml = two_section_road_with(
            r#"<object id="a" s="5" t="0"/>
               <object id="b" s="10" t="0">
                 <validity fromLane="-2" toLane="-1"/>
               </object>
               <object id="c" s="0" t="0">
                 <repeat s="2" length="16" distance="0" height="1"/>
                 <validity fromLane="1" toLane="1"/>
               </object>
               <object id="d" s="20" t="0">
                 <validity fromLane="1" toLane="1"/>
                 <validity fromLane="-2" toLane="-2"/>
               </object>"#,
        );
        assert_eq!(
            object_lanes(&xml),
            [
                // No validity: every lane of the section it stands in.
                vec![(0, 1), (0, -1), (0, -2)],
                // On the boundary, it is in the section that starts there.
                // The range runs either way round.
                vec![(1, -1), (1, -2)],
                // A sweep across the boundary is in both.
                vec![(0, 1), (1, 1)],
                // Each <validity> adds its range. At the very end of the road
                // it is in the last section.
                vec![(1, 1), (1, -2)],
            ]
        );
    }

    #[test]
    fn an_object_on_a_road_without_lanes_applies_to_none() {
        let xml = r#"<OpenDRIVE>
  <road length="20.0" id="1">
    <planView><geometry s="0" x="0" y="0" hdg="0" length="20"><line/></geometry></planView>
    <objects><object id="a" s="5" t="0"/></objects>
  </road>
  <road length="20.0" id="2">
    <planView><geometry s="0" x="0" y="9" hdg="0" length="20"><line/></geometry></planView>
    <lanes><laneSection s="0"><right><lane id="-1" type="driving"><width sOffset="0" a="3"/></lane></right></laneSection></lanes>
  </road>
</OpenDRIVE>"#;
        let net = load_str(xml).expect("import");
        assert_eq!(net.objects().len(), 1);
        assert!(net.objects()[0].lanes.is_empty());
    }

    #[test]
    fn a_marking_goes_to_the_outline_holding_its_corners() {
        let square = |id: &str| {
            let corners: String = [(0, 0), (1, 0), (1, 1), (0, 1)]
                .iter()
                .enumerate()
                .map(|(k, (u, v))| {
                    format!(r#"<cornerLocal u="{u}" v="{v}" height="0" id="{id}{k}"/>"#)
                })
                .collect();
            format!("<outline>{corners}</outline>")
        };
        let marking = |refs: &[&str], width: &str| {
            let refs: String = refs
                .iter()
                .map(|r| format!(r#"<cornerReference id="{r}"/>"#))
                .collect();
            format!(r#"<marking width="{width}">{refs}</marking>"#)
        };
        let xml = banked_road_with(&format!(
            r#"<object id="o" s="5" t="0"><outlines>{}{}</outlines><markings>{}{}{}{}{}{}</markings></object>"#,
            square("a"),
            square("b"),
            marking(&["a0", "a1"], "0.1"),
            marking(&["b1", "b2", "b3"], "0.1"),
            // Corners of two outlines, a corner of none, a single corner,
            // and no width.
            marking(&["a0", "b1"], "0.1"),
            marking(&["a0", "z9"], "0.1"),
            marking(&["a0"], "0.1"),
            marking(&["a0", "a1"], "0"),
        ));
        let objects = objects_of(&xml);
        let pieces = |o: &Object| {
            o.markings
                .iter()
                .map(|m| m.pieces.len())
                .collect::<Vec<_>>()
        };
        assert_eq!(pieces(&objects[0]), [1]);
        assert_eq!(pieces(&objects[1]), [2]);
    }

    #[test]
    fn a_border_goes_to_the_outline_it_names() {
        let square = |id: &str| {
            format!(
                r#"<outline id="{id}"><cornerLocal u="0" v="0"/><cornerLocal u="1" v="0"/><cornerLocal u="1" v="1"/></outline>"#
            )
        };
        let border =
            |outline: &str| format!(r#"<border width="0.2" {outline} useCompleteOutline="true"/>"#);
        let xml = banked_road_with(&format!(
            r#"<object id="o" s="5" t="0"><outlines>{}{}</outlines><borders>{}{}{}</borders></object>"#,
            square("a"),
            square("b"),
            border(r#"outlineId="b""#),
            border(""),
            border(r#"outlineId="z""#),
        ));
        let objects = objects_of(&xml);
        let pieces = |o: &Object| o.borders.iter().map(|b| b.pieces.len()).collect::<Vec<_>>();
        // A closed triangle has three edges. The border naming no outline is
        // on both, and the one naming an outline that is not there on none.
        assert_eq!(pieces(&objects[0]), [3]);
        assert_eq!(pieces(&objects[1]), [3, 3]);
    }

    #[test]
    fn a_hole_goes_to_the_closed_outline_round_it() {
        let square = |attrs: &str, (u, v): (f64, f64), size: f64| {
            let corners: String = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
                .map(|(du, dv)| {
                    let (u, v) = (u + du * size, v + dv * size);
                    format!(r#"<cornerLocal u="{u}" v="{v}"/>"#)
                })
                .concat();
            format!("<outline {attrs}>{corners}</outline>")
        };
        let hole = |at| square(r#"outer="false""#, at, 1.0);
        let xml = banked_road_with(&format!(
            r#"<object id="o" s="5" t="0"><outlines>{}{}{}{}{}{}</outlines></object>"#,
            // A hole inside the second outline, one in the open third, and
            // one inside none.
            hole((11.0, 1.0)),
            square("", (0.0, 0.0), 3.0),
            square("", (10.0, 0.0), 3.0),
            square(r#"closed="false""#, (20.0, 0.0), 3.0),
            hole((21.0, 1.0)),
            hole((30.0, 0.0)),
        ));
        let holes: Vec<usize> = objects_of(&xml)
            .iter()
            .map(|o| match &o.shape {
                Shape::Outline { holes, .. } => holes.len(),
                _ => panic!("an outline: {:?}", o.shape),
            })
            .collect();
        assert_eq!(holes, [0, 1, 0]);
    }

    #[test]
    fn a_structure_covers_each_section_it_spans_as_far_along_each_lane() {
        let xml = two_section_road_with(
            r#"<tunnel s="5" length="10" name="t"/>
               <bridge s="12" length="3" name="b"><validity fromLane="-2" toLane="-2"/></bridge>
               <tunnel s="5" name="no length"/>
               <bridge s="5" length="-1" name="negative"/>"#,
        );
        let (net, prov) = load_str_with_provenance(&xml).expect("import");
        let section_of = |id: LaneId| {
            let p = prov.lanes.iter().find(|p| p.lane == id).unwrap();
            (p.section, p.od_id)
        };
        let covered = |i: usize| {
            net.structures()[i]
                .lanes
                .iter()
                .map(|c| (section_of(c.lane), c.from, c.to))
                .collect::<Vec<_>>()
        };
        assert_eq!(net.structures().len(), 2);
        // The road is straight and flat, so a lane is as long as its section.
        // Section 1 starts at s = 10, and its lanes at 0.
        assert_eq!(
            covered(0),
            [
                ((0, 1), 5.0, 10.0),
                ((0, -1), 5.0, 10.0),
                ((0, -2), 5.0, 10.0),
                ((1, 1), 0.0, 5.0),
                ((1, -1), 0.0, 5.0),
                ((1, -2), 0.0, 5.0),
            ]
        );
        assert_eq!(covered(1), [((1, -2), 2.0, 5.0)]);
    }

    fn objects_of(xml: &str) -> Vec<Object> {
        load_str(xml).expect("import").objects().to_vec()
    }

    /// A solid's position, pitch, roll and extent.
    fn solid(object: &Object) -> (Point, f32, f32, Option<Extent>) {
        match object.shape {
            Shape::Solid {
                position,
                pitch,
                roll,
                extent,
                ..
            } => (position, pitch, roll, extent),
            ref other => panic!("not a solid: {other:?}"),
        }
    }

    #[test]
    fn an_object_rides_the_banked_surface_like_a_lane() {
        // Same pivot as a lane at t = 3: up by 3 sin 0.1, in to 3 cos 0.1.
        let objects = objects_of(&banked_road_with(
            r#"<object id="1" s="10" t="3" zOffset="0.5"/>"#,
        ));
        let (p, pitch, roll, _) = solid(&objects[0]);
        assert!((p.x - 10.0).abs() < 1e-5, "x {}", p.x);
        assert!((p.y - 3.0 * 0.1_f32.cos()).abs() < 1e-5, "y {}", p.y);
        assert!(
            (p.z - (3.0 * 0.1_f32.sin() + 0.5)).abs() < 1e-5,
            "z {}",
            p.z
        );
        // Pitch and roll are the file's, not the road's bank.
        assert_eq!((pitch, roll), (0.0, 0.0));
    }

    #[test]
    fn an_object_without_a_station_is_skipped() {
        let objects = objects_of(&banked_road_with(
            r#"<object id="1" t="3"/><object id="2" s="NaN" t="3"/><object id="3" s="4" t="0"/>"#,
        ));
        assert_eq!(objects.len(), 1);
        assert_eq!(solid(&objects[0]).0.x, 4.0);
    }

    #[test]
    fn instances_past_the_ends_of_the_road_are_dropped() {
        // s = 15, 20, 25, 30: only the two on the 20 m road survive, and a lone
        // object before its start goes too.
        let objects = objects_of(&banked_road_with(
            r#"<object id="1" s="0" t="0">
                 <repeat s="15" length="15" distance="5" tStart="0" tEnd="0"/>
               </object>
               <object id="2" s="-1" t="0"/>"#,
        ));
        let xs: Vec<f32> = objects.iter().map(|o| solid(o).0.x).collect();
        assert_eq!(xs, vec![15.0, 20.0]);
    }

    #[test]
    fn a_runaway_repeat_is_refused_rather_than_expanded() {
        // A billion posts is a malformed file, not a map; the road still loads.
        let objects = objects_of(&banked_road_with(
            r#"<object id="1" s="0" t="0">
                 <repeat s="0" length="20" distance="0.00000002"/>
               </object>"#,
        ));
        assert!(objects.is_empty());
    }

    #[test]
    fn each_repeat_of_an_object_contributes_its_own_row() {
        let objects = objects_of(&banked_road_with(
            r#"<object id="1" type="pole" s="0" t="0" radius="0.1">
                 <repeat s="0" length="10" distance="10" tStart="2" tEnd="2"/>
                 <repeat s="0" length="10" distance="10" tStart="-2" tEnd="-2"/>
               </object>"#,
        ));
        assert_eq!(objects.len(), 4);
        assert!(objects.iter().all(|o| o.kind == ObjectType::Pole));
        assert!(objects.iter().all(|o| solid(o).3
            == Some(Extent::Cylinder {
                radius: 0.1,
                height: 0.0
            })));
    }

    #[test]
    fn a_road_without_objects_has_none() {
        assert!(load_str(STRAIGHT).expect("import").objects().is_empty());
    }

    #[test]
    fn a_sweep_rises_straight_up_off_a_banked_road() {
        // The base follows the bank like a lane does. The top is `height`
        // above it in +Z, the same way zOffset raises a solid.
        let objects = objects_of(&banked_road_with(
            r#"<object id="1" s="0" t="0">
                 <repeat s="0" length="20" distance="0" tStart="3" tEnd="3" widthStart="1"
                         heightStart="2" zOffsetStart="0.5"/>
               </object>"#,
        ));
        let Shape::Sweep { sections } = &objects[0].shape else {
            panic!("not a sweep: {:?}", objects[0].shape);
        };
        assert_eq!(sections.len(), 11, "every 2 m over 20 m");
        let (sin, cos) = 0.1_f32.sin_cos();
        for section in sections {
            for (corner, t) in [(section.left, 3.5_f32), (section.right, 2.5)] {
                assert!(
                    (corner.base.y - t * cos).abs() < 1e-5,
                    "y {}",
                    corner.base.y
                );
                assert!((corner.base.z - (t * sin + 0.5)).abs() < 1e-5);
                assert_eq!(corner.top - corner.base, Vector::new(0.0, 0.0, 2.0));
            }
        }
    }

    #[test]
    fn a_sweep_starting_before_the_road_is_anchored_where_the_road_starts() {
        // s = -10..10 with t going 0..4: at s = 0 it is halfway, t = 2.
        let (_, prov) = load_str_with_provenance(&banked_road_with(
            r#"<object id="1" s="0" t="0">
                 <repeat s="-10" length="20" distance="0" tStart="0" tEnd="4"/>
               </object>"#,
        ))
        .expect("import");
        let p = &prov.objects[0];
        assert_eq!((p.s, p.t), (0.0, 2.0));
    }

    #[test]
    fn a_sweep_entirely_off_the_road_bakes_nothing() {
        let objects = objects_of(&banked_road_with(
            r#"<object id="1" s="0" t="0">
                 <repeat s="25" length="10" distance="0" tStart="3"/>
               </object>"#,
        ));
        assert!(objects.is_empty());
    }

    #[test]
    fn an_outline_straight_under_the_object_is_read_as_opendrive_1_4_writes_it() {
        let objects = objects_of(&banked_road_with(
            r#"<object id="1" s="5" t="0">
                 <outline><cornerRoad s="5" t="0"/><cornerRoad s="6" t="0"/></outline>
               </object>"#,
        ));
        assert!(matches!(
            &objects[0].shape,
            Shape::Outline { corners, closed: true, .. } if corners.len() == 2
        ));
    }

    #[test]
    fn an_outline_with_a_corner_off_the_road_is_dropped_whole() {
        // Keeping the three corners that fit would bake a triangle where the
        // file has a square.
        let objects = objects_of(&banked_road_with(
            r#"<object id="1" s="5" t="0"><outlines><outline>
                 <cornerRoad s="18" t="0"/><cornerRoad s="22" t="0"/>
                 <cornerRoad s="22" t="2"/><cornerRoad s="18" t="2"/>
               </outline></outlines></object>"#,
        ));
        assert!(objects.is_empty());
    }

    #[test]
    fn a_frame_turns_yaw_then_pitch_then_roll() {
        let frame = |yaw: f64, pitch: f64, roll: f64| Frame {
            origin: Point::new(1.0, 2.0, 3.0),
            yaw,
            pitch,
            roll,
        };
        let near = |got: Point, want: [f32; 3]| {
            assert!(
                (got - Point::from_array(want)).length() < 1e-6,
                "{got:?} != {want:?}"
            );
        };
        let half = std::f64::consts::FRAC_PI_2;
        // Each on its own: yaw takes u to +Y, pitch takes u down, roll takes
        // v up.
        near(
            frame(half, 0.0, 0.0).point([1.0, 0.0, 0.0]),
            [1.0, 3.0, 3.0],
        );
        near(
            frame(0.0, half, 0.0).point([1.0, 0.0, 0.0]),
            [1.0, 2.0, 2.0],
        );
        near(
            frame(0.0, 0.0, half).point([0.0, 1.0, 0.0]),
            [1.0, 2.0, 4.0],
        );
        // Together, roll acts first: it turns v up, pitch then tips up to
        // forward, and yaw turns forward to +Y.
        near(
            frame(half, half, half).point([0.0, 1.0, 0.0]),
            [1.0, 3.0, 3.0],
        );
    }
}
