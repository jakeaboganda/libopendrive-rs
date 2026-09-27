"""Generate road_marks.xodr: every road mark type on one straight road.

    uv run -q --with scenariogeneration==0.16.6 tests/data/road_marks.py

Road 0 is a 100 m straight from the origin heading +X, banked at 0.05 rad,
with a laneOffset of 0.5 m and three 3 m lanes each side. It has two lane
sections, at s = 0 and s = 50. So every lane border is at a constant t, and
every mark has a closed form to check against.

Section 0 gives each border a different keyword type, and the center lane's
mark changes from solid to solid solid at s = 25. Section 1 gives the rest
of the types, and varies width, weight, colour, height and laneChange.
"""

from pathlib import Path

from scenariogeneration import xodr

T = xodr.RoadMarkType
LC = xodr.LaneChange
COLOR = xodr.RoadMarkColor


def mark(kind, soffset=0, **kwargs):
    return xodr.RoadMark(kind, soffset=soffset, **kwargs)


def section(s, marks):
    """A lane section at `s` with lanes 3 to -3, each 3 m wide. `marks`
    maps a lane id to the road marks on it."""
    center = xodr.Lane(lane_type=xodr.LaneType.none)
    for m in marks.get(0, []):
        center.add_roadmark(m)
    ls = xodr.LaneSection(s, center)
    for side, ids in ((ls.add_left_lane, (1, 2, 3)), (ls.add_right_lane, (-1, -2, -3))):
        for i in ids:
            lane = xodr.Lane(a=3)
            for m in marks.get(i, []):
                lane.add_roadmark(m)
            side(lane)
    return ls


first = section(0, {
    0: [mark(T.solid, laneChange=LC.none), mark(T.solid_solid, soffset=25, laneChange=LC.none)],
    1: [mark(T.broken)],
    2: [mark(T.solid_broken, color=COLOR.yellow, marking_weight=xodr.RoadMarkWeight.bold)],
    3: [mark(T.curb)],
    -1: [mark(T.broken_broken)],
    -2: [mark(T.broken_solid)],
    -3: [mark(T.edge)],
})
second = section(50, {
    0: [mark(T.botts_dots)],
    1: [mark(T.none)],
    2: [mark(T.grass)],
    3: [mark(T.custom)],
    -1: [mark(T.solid, width=0.2, laneChange=LC.increase)],
    -2: [mark(T.solid, color=COLOR.yellow, laneChange=LC.decrease)],
    -3: [mark(T.broken, height=0.02)],
})

lanes = xodr.Lanes()
lanes.add_laneoffset(xodr.LaneOffset(0, 0.5, 0, 0, 0))
lanes.add_lanesection(first)
lanes.add_lanesection(second)

planview = xodr.PlanView(0, 0, 0)
planview.add_geometry(xodr.Line(100))
road = xodr.Road(0, planview, lanes)
road.add_superelevation(0, 0.05, 0, 0, 0)

odr = xodr.OpenDrive("road_marks")
odr.add_road(road)
odr.adjust_roads_and_lanes()
odr.write_xml(str(Path(__file__).with_suffix(".xodr")))
