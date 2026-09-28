//! Roads in road coordinates: each road's reference line, the profiles along
//! it, and the geometry of its lane sections, and what places a road station
//! `(s, t)` in the world.

use crate::coords::{Point, Vector};
use crate::crg::RefPoint;
use crate::{Direction, LaneId};

/// One road as the import keeps it: its reference line, the profiles along
/// it, and where its lanes lie across it.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct Road {
    pub od_id: String,
    pub length: f64,
    pub rule: TrafficRule,
    pub geoms: Vec<GeomRec>,
    pub elevations: Vec<Cubic>,
    pub superelevations: Vec<Cubic>,
    pub shapes: Vec<ShapeProfile>,
    pub lane_offsets: Vec<Cubic>,
    pub sections: Vec<RoadSection>,
}

/// One lane section of a road: the stretch of road it covers, where its
/// lanes lie across it, and the lanes it baked to.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct RoadSection {
    /// Its position among the road's `<laneSection>`s, ordered by `s`.
    pub index: usize,
    pub start: f64,
    pub end: f64,
    /// The stations its lanes are sampled at. Each baked lane has one
    /// centerline point at each.
    pub stations: Vec<f64>,
    /// The left lanes, ordered from the center outward: 1, 2, 3, ...
    pub left: Vec<LaneGeom>,
    /// The right lanes, ordered from the center outward: -1, -2, -3, ...
    pub right: Vec<LaneGeom>,
    /// Each baked lane's `<lane id>` and [`LaneId`].
    pub lanes: Vec<(i32, LaneId)>,
}

/// Where one lane lies across its road: its `<width>`s or `<border>`s, its
/// `<height>`s, and whether it is level.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct LaneGeom {
    pub id: i32,
    pub extent: LaneExtent,
    /// Its `<height>`s, in order along it.
    pub heights: Vec<HeightDef>,
    pub level: bool,
}

/// How far across the road a lane reaches: its `<width>`s, or where it has
/// none, its `<border>`s. Each list is sorted by `sOffset`.
///
/// A border is the `t` of the lane's outer border, measured from the
/// reference line, so `<laneOffset>` does not move it. The spec forbids the
/// two together. The lane's width is its border less its inner neighbour's
/// outer border, signed by side and 0 where the border crosses inside it,
/// which the spec also forbids.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) enum LaneExtent {
    Width(Vec<Cubic>),
    Border(Vec<Cubic>),
}

/// A lane's `<height>`: how far its surface stands off the road, along the
/// road's normal, at its inner and outer border, from `s_offset` metres into
/// its lane section.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct HeightDef {
    pub s_offset: f64,
    pub inner: f64,
    pub outer: f64,
}

/// A `<lateralProfile>`'s `<shape>`s at one `s`: the height off the
/// reference plane across the road, a cubic in `dt` from each shape's `t`,
/// held until the next. Each [`Cubic`] here starts at its `t`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct ShapeProfile {
    pub s: f64,
    pub shapes: Vec<Cubic>,
}

/// Which side of a road drives along `+s`, from its `<road rule>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) enum TrafficRule {
    RightHand,
    LeftHand,
}

impl TrafficRule {
    /// How the lanes on the left side of the road travel, or on the right.
    pub fn direction(self, left: bool) -> Direction {
        if left == (self == Self::LeftHand) {
            Direction::Forward
        } else {
            Direction::Backward
        }
    }

    /// The `<lane id>`s of the side whose traffic travels `direction`.
    pub fn side(self, direction: Direction) -> (i32, i32) {
        if self.direction(true) == direction {
            (1, i32::MAX)
        } else {
            (i32::MIN, -1)
        }
    }
}

// --- Reference-line geometry --------------------------------------------------

/// One `<geometry>` record: its start pose on the reference line plus its
/// shape. A spiral or a paramPoly3 has no elementary arc-length form, so it
/// is baked to `(ds, x, y, hdg)` samples when built, which [`GeomRec::pose`]
/// interpolates. It serializes without them, and rebakes them on the way
/// back in.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(from = "GeomDef", into = "GeomDef")
)]
pub(crate) struct GeomRec {
    pub s: f64,
    pub x: f64,
    pub y: f64,
    pub hdg: f64,
    pub length: f64,
    pub shape: GeomShape,
    samples: Vec<(f64, f64, f64, f64)>,
}

/// The shape of a `<geometry>`. A `<poly3>` is the paramPoly3 with
/// `u(p) = p` over `p` in `[0, length]`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) enum GeomShape {
    Line,
    Arc {
        curvature: f64,
    },
    Spiral {
        curv_start: f64,
        curv_end: f64,
    },
    ParamPoly3 {
        u: [f64; 4],
        v: [f64; 4],
        p_max: f64,
    },
}

/// What a [`GeomRec`] serializes as: its definition, without the samples.
#[cfg(feature = "serde")]
#[derive(serde::Serialize, serde::Deserialize)]
struct GeomDef {
    s: f64,
    x: f64,
    y: f64,
    hdg: f64,
    length: f64,
    shape: GeomShape,
}

#[cfg(feature = "serde")]
impl From<GeomDef> for GeomRec {
    fn from(d: GeomDef) -> Self {
        Self::new(d.s, d.x, d.y, d.hdg, d.length, d.shape)
    }
}

#[cfg(feature = "serde")]
impl From<GeomRec> for GeomDef {
    fn from(g: GeomRec) -> Self {
        Self {
            s: g.s,
            x: g.x,
            y: g.y,
            hdg: g.hdg,
            length: g.length,
            shape: g.shape,
        }
    }
}

impl GeomRec {
    /// The record starting at `s` from `(x, y)` heading `hdg`, `length`
    /// long.
    pub fn new(s: f64, x: f64, y: f64, hdg: f64, length: f64, shape: GeomShape) -> Self {
        let samples = match shape {
            GeomShape::Spiral {
                curv_start,
                curv_end,
            } => bake_spiral(x, y, hdg, curv_start, curv_end, length),
            GeomShape::ParamPoly3 { u, v, p_max } => {
                bake_param_poly3(x, y, hdg, u, v, p_max, length)
            }
            GeomShape::Line | GeomShape::Arc { .. } => Vec::new(),
        };
        Self {
            s,
            x,
            y,
            hdg,
            length,
            shape,
            samples,
        }
    }

    /// Position `(x, y)` and heading at road arc-length `s` within this record.
    pub fn pose(&self, s: f64) -> (f64, f64, f64) {
        let ds = s - self.s;
        match &self.shape {
            GeomShape::Arc { curvature } if curvature.abs() > 1e-9 => {
                let h = self.hdg + curvature * ds;
                let x = self.x + (h.sin() - self.hdg.sin()) / curvature;
                let y = self.y - (h.cos() - self.hdg.cos()) / curvature;
                (x, y, h)
            }
            GeomShape::Spiral { .. } | GeomShape::ParamPoly3 { .. } => {
                let samples = &self.samples;
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

pub(crate) fn geom_at(geoms: &[GeomRec], s: f64) -> &GeomRec {
    geoms
        .iter()
        .rev()
        .find(|g| g.s <= s + 1e-9)
        .unwrap_or(&geoms[0])
}

// --- Profiles -----------------------------------------------------------------

/// One cubic record (elevation, or a lane width), evaluated relative to its
/// own start offset: `a + b*ds + c*ds^2 + d*ds^3`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct Cubic {
    pub start: f64,
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
}

impl Cubic {
    pub fn eval(&self, s: f64) -> f64 {
        let ds = s - self.start;
        self.a + self.b * ds + self.c * ds * ds + self.d * ds * ds * ds
    }

    /// The rate of change at `s`.
    pub fn slope(&self, s: f64) -> f64 {
        let ds = s - self.start;
        self.b + 2.0 * self.c * ds + 3.0 * self.d * ds * ds
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
pub(crate) fn active(records: &[Cubic], s: f64) -> Option<&Cubic> {
    records.iter().rev().find(|r| r.start <= s + 1e-9)
}

/// The road's lateral shape at `(s, t)`: its height off the reference
/// plane there. 0 on a road with no `<shape>`.
///
/// Between two profiles the height at a `t` goes linearly along `s`, as the
/// spec has it. Before the first profile the first holds, and after the
/// last the last. The spec gives 0 before the first. Across a profile, the
/// last shape runs on to the road's edge, and before the first shape's `t`
/// its value there holds, flat.
pub(crate) fn shape_at(profiles: &[ShapeProfile], s: f64, t: f64) -> f64 {
    let across = |p: &ShapeProfile| active(&p.shapes, t).map_or(p.shapes[0].a, |c| c.eval(t));
    let next = profiles.partition_point(|p| p.s <= s + 1e-9);
    match (
        next.checked_sub(1).map(|i| &profiles[i]),
        profiles.get(next),
    ) {
        (None, None) => 0.0,
        (Some(p), None) | (None, Some(p)) => across(p),
        (Some(a), Some(b)) => {
            let f = (s - a.s) / (b.s - a.s);
            across(a) + (across(b) - across(a)) * f
        }
    }
}

/// A lane's height off the road at its inner and outer border, `s_lane`
/// metres into its section. 0 for a lane with no `<height>`.
///
/// The spec's rule for lane geometry holds a height until the next one. The
/// crate goes straight from one to the next instead, as libOpenDRIVE and
/// esmini read them, and holds the last after it. Before the first, the
/// first holds. The spec gives no height there, and libOpenDRIVE carries the
/// first ramp on backwards.
pub(crate) fn height_at(lane: &LaneGeom, s_lane: f64) -> (f64, f64) {
    let heights = &lane.heights;
    let next = heights.partition_point(|h| h.s_offset <= s_lane + 1e-9);
    match (next.checked_sub(1).map(|i| &heights[i]), heights.get(next)) {
        (None, None) => (0.0, 0.0),
        (Some(h), None) | (None, Some(h)) => (h.inner, h.outer),
        (Some(a), Some(b)) => {
            let f = (s_lane - a.s_offset) / (b.s_offset - a.s_offset);
            (
                a.inner + (b.inner - a.inner) * f,
                a.outer + (b.outer - a.outer) * f,
            )
        }
    }
}

// --- Lanes across the road ----------------------------------------------------

/// Where one lane lies across the road at one station: the `t` of its inner
/// and outer border, and its width between them.
#[derive(Clone, Copy)]
pub(crate) struct LaneBorders {
    pub inner: f64,
    pub outer: f64,
    pub width: f64,
}

/// Each lane of `side` across the road `s_lane` metres into its section,
/// from the center outward. `sign` is 1.0 for the left side and -1.0 for the
/// right, and `base` the `t` of the center lane, from `<laneOffset>`. Every
/// place that needs a lane's borders reads them here.
///
/// A lane is 0 wide before its first `<width>` or `<border>`, and where
/// either would make it narrower than 0. See [`LaneExtent`].
pub(crate) fn side_borders(
    side: &[LaneGeom],
    sign: f64,
    base: f64,
    s_lane: f64,
) -> impl Iterator<Item = LaneBorders> + '_ {
    side.iter().scan(base, move |t, lane| {
        let inner = *t;
        let width = match &lane.extent {
            LaneExtent::Width(widths) => active(widths, s_lane).map_or(0.0, |w| w.eval(s_lane)),
            LaneExtent::Border(borders) => {
                active(borders, s_lane).map_or(0.0, |b| sign * (b.eval(s_lane) - inner))
            }
        }
        .max(0.0);
        *t += sign * width;
        Some(LaneBorders {
            inner,
            outer: *t,
            width,
        })
    })
}

/// Each lane of `side`'s height off the road at its inner and outer border
/// at station `s`, `s_lane` into its section, from the center outward.
/// `borders` is where each lies, from [`side_borders`], `sign` which way it
/// stacks, `base` the `t` of the center lane, and `phi` the superelevation.
///
/// A lane's heights are its `<height>`s plus the road's lateral shape under
/// each border. A lane with `level="true"`, and every lane outside it, is
/// kept out of the shape and the superelevation instead. It starts at its
/// inner neighbour's outer height, `<height>` and all, and runs level from
/// there, with its own `<height>`s on top. Measured along the road's tilted
/// normal, level is `w tan phi` lower at its outer border, which gives it a
/// bank of 0.
pub(crate) fn side_heights<'a>(
    side: &'a [LaneGeom],
    sign: f64,
    shapes: &'a [ShapeProfile],
    (s, s_lane): (f64, f64),
    base: f64,
    phi: f64,
    borders: impl Iterator<Item = LaneBorders> + 'a,
) -> impl Iterator<Item = (f64, f64)> + 'a {
    let start = (shape_at(shapes, s, base), false);
    side.iter()
        .zip(borders)
        .scan(start, move |(outer, level), (lane, b)| {
            let (own_inner, own_outer) = height_at(lane, s_lane);
            *level |= lane.level;
            let heights = if *level {
                (
                    *outer + own_inner,
                    *outer - sign * b.width * phi.tan() + own_outer,
                )
            } else {
                (
                    own_inner + shape_at(shapes, s, b.inner),
                    own_outer + shape_at(shapes, s, b.outer),
                )
            };
            *outer = heights.1;
            Some(heights)
        })
}

/// One lane across the road at a station: the `t` of its inner and outer
/// border, and its height at each, lateral shape included.
pub(crate) struct Across {
    pub od_id: i32,
    pub inner_t: f64,
    pub outer_t: f64,
    pub inner_height: f64,
    pub outer_height: f64,
}

impl Across {
    /// The lane's height at `t`, straight from its inner border to its outer
    /// one, and on past them.
    pub fn height(&self, t: f64) -> f64 {
        let width = self.outer_t - self.inner_t;
        let f = if width == 0.0 {
            0.0
        } else {
            (t - self.inner_t) / width
        };
        self.inner_height + (self.outer_height - self.inner_height) * f
    }
}

impl RoadSection {
    /// Whether any of its lanes stands off the road's cross-section: has a
    /// `<height>`, or is level.
    pub fn raised(&self) -> bool {
        self.left
            .iter()
            .chain(&self.right)
            .any(|l| !l.heights.is_empty() || l.level)
    }
}

// --- The road surface ---------------------------------------------------------

impl Road {
    /// The reference line at `s`.
    pub fn station(&self, s: f64) -> RefPoint {
        let (x, y, heading) = geom_at(&self.geoms, s).pose(s);
        let elevation = active(&self.elevations, s);
        let bank = active(&self.superelevations, s);
        RefPoint {
            s,
            x,
            y,
            heading,
            z: elevation.map_or(0.0, |e| e.eval(s)),
            grade: elevation.map_or(0.0, |e| e.slope(s)),
            bank: bank.map_or(0.0, |e| e.eval(s)),
            bank_rate: bank.map_or(0.0, |e| e.slope(s)),
        }
    }

    /// The `t` of the center lane at `s`, from `<laneOffset>`.
    pub fn base(&self, s: f64) -> f64 {
        active(&self.lane_offsets, s).map_or(0.0, |o| o.eval(s))
    }

    /// The surface at a station `(s, t)`, on the lane there, and the
    /// reference line's heading. A point on the border between two lanes is
    /// on the inner one, as libOpenDRIVE finds it and as a road mark there
    /// belongs to it. A point past the outermost lane takes that lane's outer
    /// height. libOpenDRIVE carries the lane's slope on instead. A point on
    /// no lane takes the lateral shape under it.
    pub fn surface(&self, s: f64, t: f64) -> (Point, f64) {
        let height = self
            .section_at(s)
            .filter(|section| section.raised() || !self.shapes.is_empty())
            .and_then(|section| {
                self.across(section, s)
                    .filter(|a| f64::from(a.od_id.signum()) * (t - a.inner_t) > 0.0)
                    .last()
            })
            .map_or_else(
                || shape_at(&self.shapes, s, t),
                |a| a.height(t.clamp(a.inner_t.min(a.outer_t), a.inner_t.max(a.outer_t))),
            );
        self.raised(s, t, height)
    }

    /// The surface of the lane `od_id` of `section` at `(s, t)`, straight
    /// across the lane and on past its borders, so paint on its border lies
    /// in it. For the center lane, the lateral shape on the center line, held
    /// flat across, so its paint lies on a crown's ridge rather than under it.
    pub fn lane_surface(&self, section: &RoadSection, od_id: i32, s: f64, t: f64) -> (Point, f64) {
        if !section.raised() && self.shapes.is_empty() {
            return self.raised(s, t, 0.0);
        }
        let height = self
            .across(section, s)
            .find(|a| a.od_id == od_id)
            .map_or_else(|| shape_at(&self.shapes, s, self.base(s)), |a| a.height(t));
        self.raised(s, t, height)
    }

    /// The road surface at `(s, t)`, stood off along the road's normal there
    /// by `height`, and the reference line's heading.
    pub fn raised(&self, s: f64, t: f64, height: f64) -> (Point, f64) {
        let (point, hdg) = self.surface_at(s, t);
        if height == 0.0 {
            return (point, hdg);
        }
        let up = self.axes(s)[2];
        (
            point + Vector::from_array(up.map(|c| (c * height) as f32)),
            hdg,
        )
    }

    /// The road surface at station `s`, lateral offset `t` from the
    /// reference line, and the reference line's heading there.
    fn surface_at(&self, s: f64, t: f64) -> (Point, f64) {
        let (x, y, hdg) = geom_at(&self.geoms, s).pose(s);
        let elev = active(&self.elevations, s).map_or(0.0, |e| e.eval(s));
        // Reference-line pivot: superelevation rolls the cross-section about
        // the reference line by phi, so a point at lateral t rides up by
        // t·sin phi (positive t = left edge, raised for phi > 0) and its
        // horizontal reach shrinks to t·cos phi.
        let phi = active(&self.superelevations, s).map_or(0.0, |e| e.eval(s));
        let (sin_phi, cos_phi) = phi.sin_cos();
        let t_h = t * cos_phi;
        let point = Point::new(
            (x - t_h * hdg.sin()) as f32,
            (y + t_h * hdg.cos()) as f32,
            (elev + t * sin_phi) as f32,
        );
        (point, hdg)
    }

    /// The lane section at `s`. On a boundary, the one starting there.
    pub fn section_at(&self, s: f64) -> Option<&RoadSection> {
        self.sections
            .iter()
            .rev()
            .find(|section| section.start <= s + 1e-9)
            .or(self.sections.first())
    }

    /// Each lane of `section` across the road at `s`, from the center
    /// outward, the left side first.
    pub fn across<'a>(
        &'a self,
        section: &'a RoadSection,
        s: f64,
    ) -> impl Iterator<Item = Across> + 'a {
        let base = self.base(s);
        let s_lane = s - section.start;
        let phi = active(&self.superelevations, s).map_or(0.0, |e| e.eval(s));
        [(&section.left, 1.0), (&section.right, -1.0)]
            .into_iter()
            .flat_map(move |(side, sign)| {
                let borders: Vec<LaneBorders> = side_borders(side, sign, base, s_lane).collect();
                let heights: Vec<(f64, f64)> = side_heights(
                    side,
                    sign,
                    &self.shapes,
                    (s, s_lane),
                    base,
                    phi,
                    borders.iter().copied(),
                )
                .collect();
                side.iter().zip(borders).zip(heights).map(
                    |((lane, borders), (inner_height, outer_height))| Across {
                        od_id: lane.id,
                        inner_t: borders.inner,
                        outer_t: borders.outer,
                        inner_height,
                        outer_height,
                    },
                )
            })
    }

    /// The road's own axes at station `s`, as libOpenDRIVE has them: along
    /// the reference line up its grade, across it tilted by the
    /// superelevation, and square to both. Unit length and square to each
    /// other, right-handed.
    pub fn axes(&self, s: f64) -> [[f64; 3]; 3] {
        let (_, _, hdg) = geom_at(&self.geoms, s).pose(s);
        let grade = active(&self.elevations, s).map_or(0.0, |e| e.slope(s));
        let phi = active(&self.superelevations, s).map_or(0.0, |e| e.eval(s));
        let (sin_h, cos_h) = hdg.sin_cos();
        let (sin_p, cos_p) = phi.sin_cos();
        let along = unit([cos_h, sin_h, grade]);
        let up = unit(cross(along, [-sin_h * cos_p, cos_h * cos_p, sin_p]));
        [along, cross(up, along), up]
    }

    pub fn on_road(&self, s: f64) -> bool {
        (0.0..=self.length).contains(&s)
    }

    /// The lanes alongside the stretch `[from, to]` whose `<lane id>` is in
    /// one of the `validity` ranges, or all of them if there are no ranges.
    /// A section starting exactly at `to` counts only if the stretch is a
    /// single station, so a station on a section boundary is in the section
    /// that starts there.
    pub fn lanes(&self, (from, to): (f64, f64), validity: &[(i32, i32)]) -> Vec<LaneId> {
        let last = self.sections.len().saturating_sub(1);
        self.sections
            .iter()
            .enumerate()
            .filter(|(i, sec)| {
                sec.start <= to && (from < sec.end || (*i == last && from <= sec.end))
            })
            .flat_map(|(_, sec)| &sec.lanes)
            .filter(|(od_id, _)| is_valid(*od_id, validity))
            .map(|&(_, id)| id)
            .collect()
    }
}

/// Whether the lane `od_id` is in one of the `validity` ranges, or there are
/// none.
pub(crate) fn is_valid(od_id: i32, validity: &[(i32, i32)]) -> bool {
    validity.is_empty()
        || validity
            .iter()
            .any(|&(a, b)| (a.min(b)..=a.max(b)).contains(&od_id))
}

fn cross([a, b, c]: [f64; 3], [x, y, z]: [f64; 3]) -> [f64; 3] {
    [b * z - c * y, c * x - a * z, a * y - b * x]
}

fn unit(v: [f64; 3]) -> [f64; 3] {
    let length = v.iter().map(|c| c * c).sum::<f64>().sqrt();
    v.map(|c| c / length)
}
