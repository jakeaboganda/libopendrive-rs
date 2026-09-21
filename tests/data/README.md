# Test maps

Fixtures for the integration tests. They are excluded from the published crate
(see `exclude` in `Cargo.toml`), so run these tests from a git checkout.

## Third-party

| File | Origin | Licence |
| --- | --- | --- |
| `town07.xodr` | CARLA [opendrive-test-files], `Town07.xodr` | MIT, notice preserved in the file |
| `e6mini.xodr` | esmini, `resources/xodr/e6mini.xodr` | MPL-2.0 |

[opendrive-test-files]: https://github.com/carla-simulator/opendrive-test-files

`town07.xodr` is the exercise for real exports: 234 roads, 31 junctions, 673
driving lanes, `laneOffset`, many lane sections, no spirals. `e6mini.xodr` is a
highway built from `paramPoly3`.

## Written here

`demo.xodr` is the OpenDRIVE twin of the `demo_road` fixture in `src`.
`testtrack.xodr` is a purpose-built vehicle-tuning track: straight, crest, dip,
and three banked curves of known radius. The rest are minimal files aimed at
one behaviour each -- `spiral`, `banked_sweeper`, `right_banked_sweeper`,
`climbing_banked`, `mid_road_super`, and the malformed set (`no_geometry`,
`non_finite`, `zero_length`, `dangling_link`).
