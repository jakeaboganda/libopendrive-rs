"""Generate objects.xodr: an arc road with objects placed along it, and a
straight road that places some of them again by reference.

    uv run --with scenariogeneration==0.16.6 tests/data/objects.py

Road 0 is a 100 m left arc of radius 200 m starting at the origin heading +X,
climbing at 2 %. Road 1 is a flat 60 m straight from (0, -40) heading +X. So
every placement has a closed form to check against.

scenariogeneration has no `<objectReference>`, so those are added to its
output afterwards, as text.
"""

from pathlib import Path

from scenariogeneration import xodr

road = xodr.create_road(xodr.Arc(0.005, length=100), id=0, left_lanes=1, right_lanes=1)
road.planview.set_start_point(0, 0, 0)
road.add_elevation(0, 1.0, 0.02, 0, 0)

# A box, turned against the road, with a subtype, valid for traffic along +s
# over 8 m, and only for lane -1.
shed = xodr.Object(s=40, t=-6, Type=xodr.ObjectType.building, subtype="garage", id="1",
                   name="Shed", zOffset=0.5, hdg=0.3, length=8, width=4, height=3,
                   orientation=xodr.Orientation.positive, validLength=8)
shed.add_validity(-1, -1)
road.add_object(
    [
        shed,
        # A cylinder, pitched and rolled.
        xodr.Object(s=60, t=5, Type=xodr.ObjectType.tree, id="2", radius=1.5, height=7,
                    pitch=0.1, roll=-0.2),
        # No size at all.
        xodr.Object(s=10, t=0, Type=xodr.ObjectType.none, id="3"),
        # A type OpenDRIVE does not define.
        xodr.Object(s=20, t=4, Type="guide-post", id="4", height=1.2),
        # Something that moves, for traffic along -s.
        xodr.Object(s=90, t=-3, Type=xodr.ObjectType.barrier, subtype="boom", id="10",
                    name="Gate", dynamic=xodr.Dynamic.yes, length=4, width=0.2, height=1,
                    orientation=xodr.Orientation.negative),
    ]
)

# A row of posts drifting outward: s = 5, 15, ..., 85, t from -4 to -6.
posts = xodr.Object(s=0, t=0, Type=xodr.ObjectType.pole, id="5", radius=0.1, height=1)
posts.repeat(repeatLength=80, repeatDistance=10, sStart=5, tStart=-4, tEnd=-6,
             heightStart=1, heightEnd=2, zOffsetStart=0, zOffsetEnd=0)
road.add_object(posts)

# A continuous railing with no width: a wall swept along the road.
rail = xodr.Object(s=0, t=7, Type=xodr.ObjectType.railing, id="6", height=0.8)
rail.repeat(repeatLength=100, repeatDistance=0, sStart=0, tStart=7, tEnd=7,
            heightStart=0.8, heightEnd=0.8, zOffsetStart=0, zOffsetEnd=0)
road.add_object(rail)

# A continuous barrier that widens, moves outward and rises over 40 m, and
# runs 10 m past the end of the road.
barrier = xodr.Object(s=0, t=0, Type=xodr.ObjectType.barrier, id="7")
barrier.repeat(repeatLength=40, repeatDistance=0, sStart=70, tStart=-8, tEnd=-9,
               widthStart=0.5, widthEnd=1.0, heightStart=1, heightEnd=1,
               zOffsetStart=0, zOffsetEnd=0.4)
road.add_object(barrier)

# A building outlined in its own frame: a 6 x 4 m footprint, 5 m tall,
# turned 0.2 rad against the road.
house = xodr.Object(s=30, t=10, Type=xodr.ObjectType.building, id="8", name="House",
                    zOffset=0.1, hdg=0.2)
footprint = xodr.Outline(closed=True)
for u, v in [(0, 0), (6, 0), (6, 4), (0, 4)]:
    footprint.add_corner(xodr.CornerLocal(u, v, 0, 5))
house.add_outline(footprint)
road.add_object(house)

# A fence outlined in road coordinates, open at the end.
fence = xodr.Object(s=50, t=-12, Type=xodr.ObjectType.barrier, id="9", name="Fence")
line = xodr.Outline(closed=False)
for s, t in [(50, -12), (60, -12), (60, -14)]:
    line.add_corner(xodr.CornerRoad(s, t, 0, 1.5))
fence.add_outline(line)
road.add_object(fence)

straight = xodr.create_road(xodr.Line(60), id=1, left_lanes=1, right_lanes=1)
straight.planview.set_start_point(0, -40, 0)

odr = xodr.OpenDrive("objects")
odr.add_road(road)
odr.add_road(straight)
odr.adjust_roads_and_lanes()
out = Path(__file__).with_name("objects.xodr")
odr.write_xml(str(out))


def insert(xml, after, text):
    """`xml` with `text` spliced in straight after the first `after` in road 1."""
    road = xml.index('<road rule="RHT" id="1"')
    at = xml.index(after, road) + len(after)
    return xml[:at] + text + xml[at:]


# Road 1 places objects from road 0 again: the shed with its own zOffset,
# orientation, validLength and validity; the row of posts, which moves with the
# reference; the house, outlined in its own frame; the fence, outlined in road
# coordinates, which moves too; and an id that no object has.
references = [
    ('id="1" s="10" t="-5" zOffset="0.2" orientation="-" validLength="3"',
     '<validity fromLane="1" toLane="1"/>'),
    ('id="5" s="0" t="-1"', ""),
    ('id="8" s="30" t="8"', ""),
    ('id="9" s="20" t="12"', ""),
    ('id="99" s="40" t="0"', ""),
]


def reference(attrs, children):
    if not children:
        return f"\n            <objectReference {attrs}/>"
    return (f"\n            <objectReference {attrs}>"
            f"\n                {children}"
            "\n            </objectReference>")


xml = out.read_text()
xml = insert(xml, "</lanes>", "\n        <objects>" + "".join(
    reference(*r) for r in references) + "\n        </objects>")
out.write_text(xml)
