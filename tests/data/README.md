# Test maps

Fixtures for the integration tests. They are excluded from the published crate
(see `exclude` in `Cargo.toml`), so run these tests from a git checkout.

## Third-party

| File | Origin | Licence |
| --- | --- | --- |
| `town07.xodr` | CARLA [opendrive-test-files], `Town07.xodr` | MIT, notice preserved in the file |
| `e6mini.xodr` | esmini, `resources/xodr/e6mini.xodr` | MPL-2.0 |
| `straight_500m_signs.xodr` | esmini, `resources/xodr/straight_500m_signs.xodr` at `61b44a7` | MPL-2.0 |

[opendrive-test-files]: https://github.com/carla-simulator/opendrive-test-files

`town07.xodr` is the exercise for real exports: 234 roads, 31 junctions, 673
driving lanes, `laneOffset`, many lane sections, no spirals. `e6mini.xodr` is a
highway built from `paramPoly3`. `straight_500m_signs.xodr` is a straight
road with 19 speed and warning signs from four catalogues. Two of its ids
repeat, and some of its validities contradict their orientation.

## Written here

`demo.xodr` is the OpenDRIVE twin of the `demo_road` fixture in `src`.
`testtrack.xodr` is a purpose-built vehicle-tuning track: straight, crest, dip,
and three banked curves of known radius. The rest are minimal files aimed at
one behaviour each -- `spiral`, `banked_sweeper`, `right_banked_sweeper`,
`climbing_banked`, `mid_road_super`, and the malformed set (`no_geometry`,
`non_finite`, `zero_length`, `dangling_link`).

`crg.xodr` lays generated OpenCRG files on four roads, one per mode:
`crg_bumps.crg`, a 40 m elevation grid with a speed bump, a washboard, two
potholes and ruts, and `crg_grip.crg`, a friction grid with a wet patch.
`crg.py` writes all three. Regenerate with

    python3 tests/data/crg.py

`objects.xodr` is generated rather than written by hand: `objects.py` builds it
with [scenariogeneration], so the object fixture comes from a writer other than
this crate. Regenerate with

    uv run --with scenariogeneration==0.16.6 tests/data/objects.py

`signals.xodr` comes from `signals.py` the same way. It has signs and
traffic lights on three straight roads, with signal references and
controllers. Regenerate with

    uv run --with scenariogeneration==0.16.6 tests/data/signals.py

`road_marks.xodr` comes from `road_marks.py` the same way. It has every
road mark type on one straight, banked road with a laneOffset and two lane
sections. Regenerate with

    uv run --with scenariogeneration==0.16.6 tests/data/road_marks.py

[scenariogeneration]: https://github.com/pyoscx/scenariogeneration
