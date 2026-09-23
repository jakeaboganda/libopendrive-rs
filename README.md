# libopendrive

A pure-Rust OpenDRIVE (`.xodr`) importer. Reads a map file and bakes it into a
road network of polyline lanes, a drive-direction lane graph, and a triangle
surface mesh.

No C++ dependency, no bindings, no `unsafe`.

```rust
use libopendrive::{load_file, Point};

let net = load_file("maps/town07.xodr")?;

// Where is the road under this point, and which way does it lean?
let sample = net.sample_near(Point::new(12.0, -30.0, 0.0)).expect("on the map");
println!("{:?} banked {} rad", sample.point, sample.bank);

// Drive somewhere.
let waypoints = net.route(sample.point, Point::new(280.0, 95.0, 0.0));

// Hand the surface to a collider or a renderer.
let mesh = net.surface_mesh();
mesh.validate()?;
```

## Which OpenDRIVE version

The importer does not read `<header>`. It never inspects `revMajor` or
`revMinor`, and it never rejects a file for its version. Whether a file loads
depends only on whether it uses the elements listed below.

Every one of those elements is in ASAM OpenDRIVE 1.9.0, the current revision.
`poly3` is deprecated there, still specified, and still read here. The test
suite imports real files declaring 1.4, 1.6 and 1.7.

## What it imports

- Reference geometry: `line`, `arc`, `spiral` (clothoid), `paramPoly3`,
  `poly3`.
- `<elevationProfile>`, and `<lateralProfile>` superelevation baked as a real
  cant. The cross-section rolls about the reference line, so an outer lane
  rides higher and its surface normal leans.
- Per-lane widths, `laneOffset`, and multiple lane sections.
- The whole lane cross-section, the carriageway included. `LaneType` names
  every function the format defines, among them sidewalks, kerbs, ramps and
  tram track, and an unrecognised name bakes as `LaneType::Unknown` rather
  than leaving a hole. `LaneType::is_drivable` separates the ones traffic
  uses.
- Lane widths that vary along a lane, so a gore area that opens out of a
  point is drawn as the wedge it is.
- Road/lane `<link>`s and `<junction>`s, resolved into a drive-direction lane
  graph.

For the exact element and attribute list, see
[the crate docs](https://docs.rs/libopendrive).

Geometry is cross-checked against the reference C++
[libOpenDRIVE](https://github.com/pageldev/libOpenDRIVE).

## What it ignores

Everything else in the file, silently, including `<objects>`, `<signals>`,
`<roadMark>`, and `<geoReference>`. Three omissions change the road you get
back rather than only dropping detail around it:

- `<shape>`, the other lateralProfile child, so a crowned or cambered
  cross-section imports flat across its width.
- `<border>`. A lane whose extent comes from a border rather than a width
  element has nothing to sample, so the importer drops it.
- `<center>`, so lane 0 never becomes a `Lane`.

## Coordinate frame

Baked geometry is OpenDRIVE's own frame: right-handed, Z-up, metres, with the
reference line in the X-Y plane and elevation along +Z. A point imports
unchanged, so a coordinate you read out of the `.xodr` is the coordinate you
get back. An OpenDRIVE left turn curves toward +Y, and positive lane offset `t`
is to the left of the heading.

Travel direction follows right-hand traffic: negative-id lanes run with `+s`,
positive-id lanes against it.

## Untrusted input

A road the importer cannot interpret is skipped, not fatal. Losing a city map
to one junk road is the worse failure. `load_str` still errors if the document
yielded no lanes at all. Non-finite attribute values are rejected at parse:
Rust's float parser accepts `NaN` and turns `1e400` into infinity, and one such
value poisons every point derived from it.

## Visualization

Rendering is not in scope here, but the output is shaped for it.
`surface_mesh()` returns plain position, normal, and index buffers with no
engine types in them, and a `LaneSpan` per lane saying which slice of those
buffers it owns. That is enough to pick the lane under a cursor, or give one
lane its own material without re-tessellating.

The optional `serde` feature serializes the network and its mesh, for a viewer
in another process or a cached import. A `RoadNetwork` sends its lanes alone
and rebuilds its index on arrival, so what arrives behaves like a freshly
imported map.

```toml
libopendrive = { version = "0.1", features = ["serde"] }
```

## Coordinate types

A position is a `Point` and a direction is a `Vector`. They are separate types
with the arithmetic that relates them, so `point - point` is a `Vector`,
`point + vector` is a `Point`, and adding two positions does not compile. One
three-float type used for everything makes `nearest_lane(sample.up)` legal,
which it is not.

Both are `#[repr(C)]` structs of three public `f32` fields, so handing one to
another math library is one call:

```rust
let v = glam::Vec3::from(point.to_array());
```

There are no required dependencies beyond `roxmltree` and `thiserror`, so
nothing here constrains which math or engine crate you use, or its version.

## Performance

Lane and surface lookups are answered off a ground-plane index, so their cost
tracks local road density rather than map size. On CARLA's Town07 (234 roads,
920 lanes, 673 of them driving):

| | per call |
| --- | --- |
| `nearest_lane` / `sample_near` | ~0.9 us |
| `route` (across the map) | ~28 us |
| `MeshSampler::height_at` | ~0.2 us |

Import is ~9 ms for that map, three quarters of it XML parsing.

`cargo bench` reproduces these. `tests/budgets.rs` guards them in CI by racing
each indexed lookup against the scan it replaced, which needs no fixed
per-machine threshold.

## Testing

Map fixtures, tests, and benchmarks are excluded from the published crate, so
run them from a git checkout. See `tests/data/README.md` for fixture
provenance.

MSRV is 1.82.

## License

MIT or Apache-2.0, at your option.
