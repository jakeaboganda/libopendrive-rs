//! The OpenDRIVE parser: `.xodr` XML in, a baked [`RoadNetwork`] out.
//!
//! Reference geometry (`line`, `arc`, `spiral`, `paramPoly3`, `poly3`) is
//! evaluated to points at a fixed arc-length step, elevation and
//! superelevation profiles are applied per sample, and lane connectivity is
//! resolved in [`links`] once every lane has an id.
//!
//! Geometry is cross-checked against the reference C++
//! [libOpenDRIVE](https://github.com/pageldev/libOpenDRIVE).

use crate::coords::Point;
use crate::{Direction, Lane, LaneId, LaneType, Polyline, RoadNetwork};

mod links;
use links::{LaneMeta, RoadInfo, Topology};

/// Arc-length spacing (meters) at which curved geometry is baked to points.
const SAMPLE_STEP: f64 = 2.0;

/// Why an OpenDRIVE document did not import.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// The document is not well-formed XML.
    #[error("invalid OpenDRIVE XML: {0}")]
    Xml(#[from] roxmltree::Error),
    /// The XML parsed, but it is not a usable map. Either it was unreadable
    /// from disk, or it carries no driving lanes at all.
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

/// Load an OpenDRIVE file from disk and bake it into a `RoadNetwork`.
pub fn load_file(path: impl AsRef<std::path::Path>) -> Result<RoadNetwork, ImportError> {
    load_file_with_provenance(path).map(|(net, _)| net)
}

/// Bake an OpenDRIVE document (as a string) into a `RoadNetwork`.
pub fn load_str(xml: &str) -> Result<RoadNetwork, ImportError> {
    load_str_with_provenance(xml).map(|(net, _)| net)
}

/// Like [`load_file`], but also returns the OpenDRIVE [`LaneProvenance`] of
/// every baked lane, for a viewer or editor that must name the road, section,
/// and original lane id a baked lane came from.
pub fn load_file_with_provenance(
    path: impl AsRef<std::path::Path>,
) -> Result<(RoadNetwork, Vec<LaneProvenance>), ImportError> {
    let xml = std::fs::read_to_string(path.as_ref())
        .map_err(|e| ImportError::Malformed(format!("reading {:?}: {e}", path.as_ref())))?;
    load_str_with_provenance(&xml)
}

/// Like [`load_str`], but also returns the OpenDRIVE [`LaneProvenance`] of
/// every baked lane. The provenance is in baked-lane order, one entry per
/// lane, each carrying the [`LaneId`] it describes.
pub fn load_str_with_provenance(
    xml: &str,
) -> Result<(RoadNetwork, Vec<LaneProvenance>), ImportError> {
    let cleaned = sanitize(xml);
    let doc = roxmltree::Document::parse(cleaned.as_ref())?;
    let root = doc.root_element();
    let mut lanes = Vec::new();
    let mut topo = Topology::default();
    for road in root.children().filter(|n| n.has_tag_name("road")) {
        parse_road(road, &mut lanes, &mut topo);
    }
    if lanes.is_empty() {
        return Err(ImportError::Malformed("no driving lanes found".into()));
    }
    // Resolve connectivity once all lanes exist and are registered.
    topo.junctions = links::junctions(root);
    links::resolve(&mut lanes, &topo);
    // The parser already recorded each lane's OpenDRIVE origin while baking;
    // surface it rather than reconstructing lane-id order downstream.
    let provenance = topo
        .metas
        .iter()
        .map(|m| LaneProvenance {
            lane: m.id,
            road_id: m.road.clone(),
            section: m.section,
            od_id: m.od_id,
        })
        .collect();
    Ok((RoadNetwork::new(lanes), provenance))
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

/// One lane parsed from a `<laneSection>`. `kind` is the [`LaneType`] it emits
/// as, or `None` for a type the importer keeps only so its width offsets the
/// lanes outboard of it.
struct LaneDef {
    id: i32,
    kind: Option<LaneType>,
    widths: Vec<Cubic>,
    pred_link: Option<i32>,
    succ_link: Option<i32>,
}

/// The [`LaneType`] an OpenDRIVE `<lane>` `type` maps to, or `None` for a type
/// the importer does not emit. A new emitted lane type is one arm here.
fn lane_type(od_type: Option<&str>) -> Option<LaneType> {
    match od_type {
        Some("driving") => Some(LaneType::Driving),
        _ => None,
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

/// Bakes one `<road>`'s lanes into `out`.
///
/// A road the importer cannot interpret is *skipped*, not fatal. That covers
/// no length, no `<planView>`, no supported geometry, and no `<lanes>`. Real
/// exports carry the occasional junk road, and losing a whole city map to one
/// of them is the worse failure. Individual malformed lanes are already skipped the
/// same way. `load_str` still errors if the document as a whole yielded no
/// lanes at all, so a thoroughly broken file is never silently accepted.
fn parse_road(road: roxmltree::Node, out: &mut Vec<Lane>, topo: &mut Topology) {
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

    let Some(lanes_node) = child(road, "lanes") else {
        return;
    };
    let elevations = child(road, "elevationProfile")
        .map(|n| cubics_in(n, "elevation", "s"))
        .unwrap_or_default();
    // Superelevation: a roll of the whole cross-section about the reference
    // line, a cubic in road-s. Same `Cubic`/`active` machinery as elevation;
    // applied per lane by the reference-line pivot in `sample_lane`.
    let superelevations = child(road, "lateralProfile")
        .map(|n| cubics_in(n, "superelevation", "s"))
        .unwrap_or_default();
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
        return;
    }
    sections.sort_by(|a, b| {
        attr_f64(*a, "s")
            .unwrap_or(0.0)
            .total_cmp(&attr_f64(*b, "s").unwrap_or(0.0))
    });

    // Record the road's link targets + section count for connectivity.
    let (predecessor, successor) = links::road_link(road);
    topo.roads.insert(
        road_id.clone(),
        RoadInfo {
            sections: sections.len(),
            predecessor,
            successor,
        },
    );

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
        emit_section(
            *section,
            s_start,
            s_end,
            &geoms,
            &elevations,
            &superelevations,
            &lane_offsets,
            out,
            topo,
            &road_id,
            i,
        );
    }
}

/// Append each driving lane of one section as a `Lane` spanning `[s_start,
/// s_end]`. Malformed individual lanes are skipped, not fatal (real files).
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
            // Keep every lane, whatever its type. A shoulder or a `none` lane
            // still pushes the lanes outboard of it away from the reference
            // line, so its width has to enter the running offset even though
            // only some types are emitted. Omitting these lanes placed an
            // outboard driving lane too close to the reference line, and on a
            // curve gave it the wrong radius and length.
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
        // Lanes emitted on this side, center-outward, as (id, index in `out`).
        // Consecutive ones are lateral neighbors (lane-change edges).
        let mut emitted: Vec<(LaneId, usize)> = Vec::new();
        for (i, lane) in side.iter().enumerate() {
            let Some(kind) = lane.kind else {
                continue; // offset-only type: counted above, not emitted
            };
            let points = sample_lane(
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
            let Some(center) = Polyline::try_new(points) else {
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
            emitted.push((id, out.len()));
            out.push(Lane {
                id,
                kind,
                direction,
                center,
                width: width_at(lane, 0.0) as f32,
                // Road-level roll, shared across the section's lanes; parallel to
                // the sampled centerline. Empty when the road is flat.
                bank: bank.clone(),
                // Filled by links::resolve once all lanes are registered.
                successors: Vec::new(),
                predecessors: Vec::new(),
                neighbors: Vec::new(),
            });
        }
        // Consecutive same-side lanes are each other's lane-change neighbors.
        for k in 0..emitted.len() {
            let mut nbrs = Vec::new();
            if k > 0 {
                nbrs.push(emitted[k - 1].0);
            }
            if k + 1 < emitted.len() {
                nbrs.push(emitted[k + 1].0);
            }
            out[emitted[k].1].neighbors = nbrs;
        }
    }
}

/// Sample one lane's centerline to points in our coordinate frame.
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
) -> Vec<Point> {
    sample_s
        .iter()
        .map(|&s| {
            let s_lane = s - section_s;
            // Center offset: the road's laneOffset (shared by all lanes) plus
            // this lane's own: cumulative inner-lane widths + half its width.
            let base = active(lane_offsets, s).map(|o| o.eval(s)).unwrap_or(0.0);
            let inner_w: f64 = inner.iter().map(|l| width_at(l, s_lane)).sum();
            let t = base + sign * (inner_w + width_at(lane, s_lane) / 2.0);

            let g = geom_at(geoms, s);
            let (x, y, hdg) = g.pose(s);
            let elev = active(elevations, s).map(|e| e.eval(s)).unwrap_or(0.0);
            // Reference-line pivot: superelevation rolls the cross-section about
            // the reference line by phi, so a point at lateral t rides up by
            // t·sin phi (positive t = left edge, raised for phi > 0) and its
            // horizontal reach shrinks to t·cos phi. phi = 0 leaves this the
            // pure-horizontal offset it was.
            let phi = active(superelevations, s).map(|e| e.eval(s)).unwrap_or(0.0);
            let (sin_phi, cos_phi) = phi.sin_cos();
            let t_h = t * cos_phi;
            // The baked frame is OpenDRIVE's own, so the reference-line point
            // needs no mapping. Offset along the left-hand normal, which in the
            // reference line's plane is (-sin hdg, cos hdg).
            Point::new(
                (x - t_h * hdg.sin()) as f32,
                (y + t_h * hdg.cos()) as f32,
                (elev + t * sin_phi) as f32,
            )
        })
        .collect()
}

fn width_at(lane: &LaneDef, s_lane: f64) -> f64 {
    active(&lane.widths, s_lane)
        .map(|w| w.eval(s_lane).max(0.0))
        .unwrap_or(0.0)
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

    // A driving lane outboard of a shoulder. The shoulder is not a driving lane
    // and is not emitted, but its width still pushes the driving lane out.
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
        // Only the driving lane -2 is emitted, but it sits outboard of the 2.0 m
        // shoulder: its center is at t = -(2.0 + 3.0/2) = -3.5, so y = -3.5.
        // Skipping the shoulder's width would leave it at -1.5.
        let net = load_str(SHOULDER_INBOARD).expect("import");
        assert_eq!(net.driving_lanes().count(), 1, "only the driving lane emits");
        let y = net.lanes()[0].center.pose_at(0.0).position.y;
        assert!((y + 3.5).abs() < 0.05, "y {y}, expected -3.5");
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

    #[test]
    fn provenance_names_each_baked_lane_by_road_section_and_od_id() {
        let (net, prov) =
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
}
