//! Road marks, placed along the lane borders of each baked road.

use super::{
    attr_f64, width_at, BakedRoad, BakedSection, LaneDef, RoadMarkProvenance, MAX_REPEAT_INSTANCES,
};
use crate::coords::Point;
use crate::{
    LaneChange, LaneId, LinePattern, RoadMark, RoadMarkId, RoadMarkLine, RoadMarkType,
    RoadMarkWeight,
};

/// The width of a standard and a bold line where the map gives none, in
/// metres, as libOpenDRIVE has them.
const STANDARD_WIDTH: f64 = 0.12;
const BOLD_WIDTH: f64 = 0.25;

/// The dashes of a `broken` mark the map describes by its type alone, in
/// metres, as esmini draws them.
const BROKEN_LENGTH: f64 = 4.0;
const BROKEN_SPACE: f64 = 8.0;

/// A `<roadMark>` as the file gives it, before it is placed.
pub(super) struct MarkDef {
    /// Where it starts, in metres from the start of its lane section.
    s_offset: f64,
    kind: RoadMarkType,
    weight: RoadMarkWeight,
    color: String,
    /// Its width, if the map gives one above 0.
    width: Option<f64>,
    height: Option<f64>,
    lane_change: LaneChange,
}

/// The road marks baked so far, and the provenance of each, in step.
#[derive(Default)]
pub(super) struct RoadMarks {
    pub baked: Vec<RoadMark>,
    pub provenance: Vec<RoadMarkProvenance>,
}

/// The `<roadMark>`s of one `<lane>`, in order along it.
///
/// The spec says they come in ascending `sOffset`. Ones out of order are
/// sorted, as lane sections are, rather than dropped. A missing `sOffset` is
/// 0, a missing `type` is `none` and a missing `color` is `standard`, though
/// the spec requires all three.
pub(super) fn parse(lane: roxmltree::Node) -> Vec<MarkDef> {
    let mut marks: Vec<MarkDef> = lane
        .children()
        .filter(|n| n.has_tag_name("roadMark"))
        .map(|m| MarkDef {
            s_offset: attr_f64(m, "sOffset").unwrap_or(0.0),
            kind: mark_type(m.attribute("type")),
            weight: match m.attribute("weight") {
                Some("bold") => RoadMarkWeight::Bold,
                _ => RoadMarkWeight::Standard,
            },
            color: m.attribute("color").unwrap_or("standard").to_string(),
            width: attr_f64(m, "width").filter(|w| *w > 0.0),
            height: attr_f64(m, "height"),
            lane_change: lane_change(m.attribute("laneChange")),
        })
        .collect();
    marks.sort_by(|a, b| a.s_offset.total_cmp(&b.s_offset));
    marks
}

/// Bake the road marks of every lane section of every road in `roads`.
pub(super) fn place(roads: &[(roxmltree::Node, BakedRoad)]) -> RoadMarks {
    let mut out = RoadMarks::default();
    for (_, road) in roads {
        for section in &road.sections {
            place_section(road, section, &mut out);
        }
    }
    out
}

/// One lane border of a section, and the marks along it.
struct Border<'a> {
    /// The `<lane id>` whose marks these are, 0 for the center lane.
    od_lane_id: i32,
    marks: &'a [MarkDef],
    /// The lanes between the center and the border, and which way they
    /// stack: 1.0 toward +t, -1.0 toward -t.
    inner: &'a [LaneDef],
    sign: f64,
    /// Which way the inside of the road is, for ordering double lines: -t on
    /// the left side, and +t on the right side and the center lane, whose
    /// lines go left to right.
    inside: f64,
    left: Option<LaneId>,
    right: Option<LaneId>,
}

impl Border<'_> {
    /// The border's `t` at road station `s`.
    fn t(&self, road: &BakedRoad, section: &BakedSection, s: f64) -> f64 {
        let base = super::active(&road.lane_offsets, s).map_or(0.0, |o| o.eval(s));
        let width: f64 = self
            .inner
            .iter()
            .map(|l| width_at(l, s - section.start))
            .sum();
        base + self.sign * width
    }
}

/// Bake every mark along the borders of one section: the center lane's, then
/// each left lane's outer border, then each right lane's.
fn place_section(road: &BakedRoad, section: &BakedSection, out: &mut RoadMarks) {
    let (left, right) = (&section.def.left, &section.def.right);
    let baked = |lane: Option<&LaneDef>| {
        let lane = lane?;
        section
            .lanes
            .iter()
            .find(|l| l.od_id == lane.id)
            .map(|l| l.id)
    };
    let mut borders = vec![Border {
        od_lane_id: 0,
        marks: &section.def.center,
        inner: &[],
        sign: 0.0,
        inside: 1.0,
        left: baked(left.first()),
        right: baked(right.first()),
    }];
    for (i, lane) in left.iter().enumerate() {
        borders.push(Border {
            od_lane_id: lane.id,
            marks: &lane.marks,
            inner: &left[..=i],
            sign: 1.0,
            inside: -1.0,
            left: baked(left.get(i + 1)),
            right: baked(Some(lane)),
        });
    }
    for (i, lane) in right.iter().enumerate() {
        borders.push(Border {
            od_lane_id: lane.id,
            marks: &lane.marks,
            inner: &right[..=i],
            sign: -1.0,
            inside: 1.0,
            left: baked(Some(lane)),
            right: baked(right.get(i + 1)),
        });
    }
    for border in &borders {
        for (k, mark) in border.marks.iter().enumerate() {
            let start = (section.start + mark.s_offset).max(section.start);
            let end = border
                .marks
                .get(k + 1)
                .map_or(section.end, |next| section.start + next.s_offset)
                .min(section.end);
            if end - start < 1e-6 {
                continue;
            }
            let id = RoadMarkId(out.baked.len());
            out.baked
                .push(bake(road, section, border, mark, id, (start, end)));
            out.provenance.push(RoadMarkProvenance {
                road_mark: id,
                road_id: road.id.clone(),
                section: section.index,
                od_lane_id: border.od_lane_id,
                s: start,
                length: end - start,
            });
        }
    }
}

/// One mark along `border` over the stretch `[start, end]` of road.
fn bake(
    road: &BakedRoad,
    section: &BakedSection,
    border: &Border,
    mark: &MarkDef,
    id: RoadMarkId,
    (start, end): (f64, f64),
) -> RoadMark {
    let width = mark.width.unwrap_or(match mark.weight {
        RoadMarkWeight::Standard => STANDARD_WIDTH,
        RoadMarkWeight::Bold => BOLD_WIDTH,
    });
    let lines = stand_ins(mark.kind, width * border.inside)
        .into_iter()
        .map(|(t_offset, pattern)| RoadMarkLine {
            color: mark.color.clone(),
            width: width as f32,
            t_offset: t_offset as f32,
            pattern,
            pieces: paint(
                road,
                section,
                |s| border.t(road, section, s) + t_offset,
                width / 2.0,
                &painted(pattern, start, end),
            ),
        })
        .collect();
    RoadMark {
        id,
        kind: mark.kind,
        weight: mark.weight,
        color: mark.color.clone(),
        width: width as f32,
        height: mark.height.map(|h| h as f32),
        lane_change: mark.lane_change,
        left: border.left,
        right: border.right,
        lines,
    }
}

/// The lines esmini draws for a mark of type `kind` with no lines of its
/// own, each as its offset from the border and its pattern. A double type's
/// first line is `inside` from the border, and its second as far the other
/// way.
fn stand_ins(kind: RoadMarkType, inside: f64) -> Vec<(f64, LinePattern)> {
    let solid = LinePattern::Continuous;
    let broken = LinePattern::Dashed {
        length: BROKEN_LENGTH as f32,
        space: BROKEN_SPACE as f32,
    };
    let double = |first, second| vec![(inside, first), (-inside, second)];
    match kind {
        RoadMarkType::Solid => vec![(0.0, solid)],
        RoadMarkType::Broken => vec![(0.0, broken)],
        RoadMarkType::SolidSolid => double(solid, solid),
        RoadMarkType::SolidBroken => double(solid, broken),
        RoadMarkType::BrokenSolid => double(broken, solid),
        RoadMarkType::BrokenBroken => double(broken, broken),
        _ => Vec::new(),
    }
}

/// The stretches of road `[start, end]` a line with `pattern` paints, from
/// `start`. None for dashes of no length, or more than
/// [`MAX_REPEAT_INSTANCES`] of them.
fn painted(pattern: LinePattern, start: f64, end: f64) -> Vec<(f64, f64)> {
    match pattern {
        LinePattern::Continuous => vec![(start, end)],
        LinePattern::Dashed { length, space } => {
            let (length, space) = (f64::from(length), f64::from(space.max(0.0)));
            if length <= 0.0 || (end - start) / (length + space) > MAX_REPEAT_INSTANCES {
                return Vec::new();
            }
            (0..)
                .map(|k| start + k as f64 * (length + space))
                .take_while(|&a| a < end)
                .map(|a| (a, (a + length).min(end)))
                .collect()
        }
    }
}

/// Quads `half_width` either side of the line at `t(s)`, over each stretch of
/// road in `stretches`. The line's edges are placed on the road surface at
/// each of the section's stations, and straight between them, so the paint
/// lies on the facets of the lane mesh.
fn paint(
    road: &BakedRoad,
    section: &BakedSection,
    t: impl Fn(f64) -> f64,
    half_width: f64,
    stretches: &[(f64, f64)],
) -> Vec<[Point; 4]> {
    let stations = &section.stations;
    let edges = |i: usize| {
        let (s, t) = (stations[i], t(stations[i]));
        (
            road.surface(s, t - half_width).0,
            road.surface(s, t + half_width).0,
        )
    };
    let at = |s: f64| {
        let i = stations
            .partition_point(|&x| x <= s)
            .clamp(1, stations.len() - 1);
        let ((r0, l0), (r1, l1)) = (edges(i - 1), edges(i));
        let f = ((s - stations[i - 1]) / (stations[i] - stations[i - 1])) as f32;
        (r0.lerp(r1, f), l0.lerp(l1, f))
    };
    let mut pieces = Vec::new();
    for &(from, to) in stretches {
        let mut along = vec![from];
        along.extend(
            stations
                .iter()
                .copied()
                .filter(|&s| s > from + 1e-6 && s < to - 1e-6),
        );
        along.push(to);
        for w in along.windows(2) {
            let ((r0, l0), (r1, l1)) = (at(w[0]), at(w[1]));
            pieces.push([r0, r1, l1, l0]);
        }
    }
    pieces
}

fn mark_type(od_type: Option<&str>) -> RoadMarkType {
    match od_type {
        None | Some("none") => RoadMarkType::None,
        Some("solid") => RoadMarkType::Solid,
        Some("broken") => RoadMarkType::Broken,
        Some("solid solid") => RoadMarkType::SolidSolid,
        Some("solid broken") => RoadMarkType::SolidBroken,
        Some("broken solid") => RoadMarkType::BrokenSolid,
        Some("broken broken") => RoadMarkType::BrokenBroken,
        Some("botts dots") => RoadMarkType::BottsDots,
        Some("grass") => RoadMarkType::Grass,
        Some("curb") => RoadMarkType::Curb,
        Some("edge") => RoadMarkType::Edge,
        Some("custom") => RoadMarkType::Custom,
        Some(_) => RoadMarkType::Unknown,
    }
}

/// A missing `laneChange` is `both`, the spec's default.
fn lane_change(value: Option<&str>) -> LaneChange {
    match value {
        None | Some("both") => LaneChange::Both,
        Some("increase") => LaneChange::Increase,
        Some("decrease") => LaneChange::Decrease,
        Some("none") => LaneChange::None,
        Some(_) => LaneChange::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use crate::{load_str_with_provenance, RoadMark, RoadMarkType, RoadMarkWeight};

    /// A 20 m road heading +X, or along an arc of `curvature`, with a
    /// section whose `<right>` holds `right`.
    fn road(curvature: f64, right: &str) -> String {
        let shape = if curvature == 0.0 {
            "<line/>".to_string()
        } else {
            format!(r#"<arc curvature="{curvature}"/>"#)
        };
        format!(
            r#"<OpenDRIVE><road id="1" length="20" junction="-1">
              <planView><geometry s="0" x="0" y="0" hdg="0" length="20">{shape}</geometry></planView>
              <lanes><laneSection s="0">
                <center><lane id="0" type="none"/></center>
                <right>{right}</right>
              </laneSection></lanes>
            </road></OpenDRIVE>"#
        )
    }

    /// A driving lane `id` 3.5 m wide carrying `marks`.
    fn lane(id: i32, marks: &str) -> String {
        format!(r#"<lane id="{id}" type="driving"><width sOffset="0" a="3.5"/>{marks}</lane>"#)
    }

    /// The marks, each with where it starts and how long it is.
    fn marks_of(xml: &str) -> Vec<(RoadMark, f64, f64)> {
        let (net, prov) = load_str_with_provenance(xml).expect("the road loads");
        net.road_marks()
            .iter()
            .zip(&prov.road_marks)
            .map(|(m, p)| (m.clone(), p.s, p.length))
            .collect()
    }

    fn only(xml: &str) -> RoadMark {
        let marks = marks_of(xml);
        assert_eq!(marks.len(), 1, "marks");
        marks[0].0.clone()
    }

    #[test]
    fn a_mark_without_a_colour_is_standard() {
        let m = only(&road(
            0.0,
            &lane(-1, r#"<roadMark sOffset="0" type="solid"/>"#),
        ));
        assert_eq!(m.color, "standard");
        assert_eq!(m.lines[0].color, "standard");
    }

    #[test]
    fn a_mark_without_a_type_is_none_and_paints_nothing() {
        let m = only(&road(
            0.0,
            &lane(-1, r#"<roadMark sOffset="0" color="white"/>"#),
        ));
        assert_eq!(m.kind, RoadMarkType::None);
        assert!(m.lines.is_empty());
    }

    #[test]
    fn an_unrecognised_type_is_unknown_and_paints_nothing() {
        let m = only(&road(
            0.0,
            &lane(-1, r#"<roadMark sOffset="0" type="zigzag"/>"#),
        ));
        assert_eq!(m.kind, RoadMarkType::Unknown);
        assert!(m.lines.is_empty());
    }

    #[test]
    fn a_mark_without_an_s_offset_starts_at_its_section() {
        let marks = marks_of(&road(0.0, &lane(-1, r#"<roadMark type="solid"/>"#)));
        assert_eq!((marks[0].1, marks[0].2), (0.0, 20.0));
    }

    #[test]
    fn a_weight_the_map_does_not_give_is_standard() {
        let m = only(&road(
            0.0,
            &lane(-1, r#"<roadMark sOffset="0" type="solid"/>"#),
        ));
        assert_eq!((m.weight, m.width), (RoadMarkWeight::Standard, 0.12));
    }

    #[test]
    fn a_width_of_zero_falls_back_to_the_weight() {
        let mark = r#"<roadMark sOffset="0" type="solid" weight="bold" width="0"/>"#;
        let m = only(&road(0.0, &lane(-1, mark)));
        assert_eq!((m.width, m.lines[0].width), (0.25, 0.25));
    }

    #[test]
    fn marks_out_of_order_are_sorted_rather_than_dropped() {
        let marks = r#"<roadMark sOffset="12" type="broken"/><roadMark sOffset="0" type="solid"/>"#;
        let got: Vec<(RoadMarkType, f64, f64)> = marks_of(&road(0.0, &lane(-1, marks)))
            .into_iter()
            .map(|(m, s, length)| (m.kind, s, length))
            .collect();
        assert_eq!(
            got,
            [
                (RoadMarkType::Solid, 0.0, 12.0),
                (RoadMarkType::Broken, 12.0, 8.0)
            ]
        );
    }

    #[test]
    fn a_lane_without_a_width_takes_its_marks_with_it() {
        let lanes = lane(-1, r#"<roadMark sOffset="0" type="solid"/>"#)
            + r#"<lane id="-2" type="driving"><roadMark sOffset="0" type="solid"/></lane>"#;
        let (net, prov) = load_str_with_provenance(&road(0.0, &lanes)).expect("the road loads");
        let lanes: Vec<i32> = prov.road_marks.iter().map(|p| p.od_lane_id).collect();
        assert_eq!(lanes, [-1]);
        assert_eq!(net.road_marks()[0].right, None);
    }

    #[test]
    fn dashes_are_measured_along_the_reference_line() {
        // A left turn of radius 50 m, so the border of lane -1, 3.5 m to the
        // right, is on a radius of 53.5 m. A 4 m dash of s turns 0.08 rad.
        let m = only(&road(
            0.02,
            &lane(-1, r#"<roadMark sOffset="0" type="broken"/>"#),
        ));
        let pieces = &m.lines[0].pieces;
        let middle = |q: &[crate::Point; 4], a: usize, b: usize| q[a].lerp(q[b], 0.5);
        let first_dash = 1 + pieces
            .windows(2)
            .take_while(|w| w[0][1].distance_to(w[1][0]) < 1e-4)
            .count();
        let start = middle(&pieces[0], 0, 3);
        let end = middle(&pieces[first_dash - 1], 1, 2);
        let chord = 2.0 * 53.5 * 0.04_f32.sin();
        assert!(
            (start.distance_to(end) - chord).abs() < 1e-2,
            "dash chord {} want {chord}",
            start.distance_to(end)
        );
    }
}
