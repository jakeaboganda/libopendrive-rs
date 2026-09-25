"""Generate objects.xodr: one arc road with objects placed along it.

    uv run --with scenariogeneration==0.16.6 tests/data/objects.py

The road is a 100 m left arc of radius 200 m starting at the origin heading
+X, climbing at 2 %, so every placement has a closed form to check against.
"""

from pathlib import Path

from scenariogeneration import xodr

road = xodr.create_road(xodr.Arc(0.005, length=100), id=0, left_lanes=1, right_lanes=1)
road.add_elevation(0, 1.0, 0.02, 0, 0)

road.add_object(
    [
        # A box, turned against the road.
        xodr.Object(s=40, t=-6, Type=xodr.ObjectType.building, id="1", name="Shed",
                    zOffset=0.5, hdg=0.3, length=8, width=4, height=3),
        # A cylinder, pitched and rolled.
        xodr.Object(s=60, t=5, Type=xodr.ObjectType.tree, id="2", radius=1.5, height=7,
                    pitch=0.1, roll=-0.2),
        # No size at all.
        xodr.Object(s=10, t=0, Type=xodr.ObjectType.none, id="3"),
        # A type OpenDRIVE does not define.
        xodr.Object(s=20, t=4, Type="guide-post", id="4", height=1.2),
    ]
)

# A row of posts drifting outward: s = 5, 15, ..., 85, t from -4 to -6.
posts = xodr.Object(s=0, t=0, Type=xodr.ObjectType.pole, id="5", radius=0.1, height=1)
posts.repeat(repeatLength=80, repeatDistance=10, sStart=5, tStart=-4, tEnd=-6,
             heightStart=1, heightEnd=2, zOffsetStart=0, zOffsetEnd=0)
road.add_object(posts)

# A continuous railing: a swept shape, not a row of placements.
rail = xodr.Object(s=0, t=7, Type=xodr.ObjectType.railing, id="6", height=0.8)
rail.repeat(repeatLength=100, repeatDistance=0, sStart=0, tStart=7, tEnd=7,
            heightStart=0.8, heightEnd=0.8, zOffsetStart=0, zOffsetEnd=0)
road.add_object(rail)

odr = xodr.OpenDrive("objects")
odr.add_road(road)
odr.adjust_roads_and_lanes()
odr.write_xml(str(Path(__file__).with_name("objects.xodr")))
