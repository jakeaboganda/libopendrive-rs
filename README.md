# libopendrive

A pure-Rust OpenDRIVE (`.xodr`) importer. Reads a map file and bakes it into a
road network of polyline lanes, a drive-direction lane graph, and a triangle
surface mesh.

No C++ dependency, no bindings, no `unsafe`.

```rust
use libopendrive::{glam::Vec3, load_file};

let net = load_file("maps/town07.xodr")?;

// Where is the road under this point, and which way does it lean?
let sample = net.sample_near(Vec3::new(12.0, 0.0, -30.0)).expect("on the map");
println!("{:?} banked {} rad", sample.point, sample.bank);

// Drive somewhere.
let waypoints = net.route(sample.point, Vec3::new(280.0, 0.0, 95.0));

// Hand the surface to a collider or a renderer.
let mesh = net.surface_mesh();
mesh.validate()?;
```

## What it imports

- Reference geometry: `line`, `arc`, `spiral` (clothoid), `paramPoly3`,
  `poly3`.
- `<elevationProfile>`, and `<lateralProfile>` superelevation baked as a real
  cant -- the cross-section rolls about the reference line, so an outer lane
  rides higher and its surface normal leans.
- Per-lane widths, `laneOffset`, and multiple lane sections.
- Road/lane `<link>`s and `<junction>`s, resolved into a drive-direction lane
  graph.

Not yet: `<lateralProfile>` `<shape>` (per-`t` crowning and camber), and lane
types other than `driving`.

Geometry is cross-checked against the reference C++
[libOpenDRIVE](https://github.com/pageldev/libOpenDRIVE).

## Coordinate frame

Baked geometry is OpenDRIVE's own frame: right-handed, Z-up, metres, with the
reference line in the X-Y plane and elevation along +Z. A point imports
unchanged, so a coordinate you read out of the `.xodr` is the coordinate you
get back. An OpenDRIVE left turn curves toward +Y, and positive lane offset `t`
is to the left of the heading.

A renderer that wants Y-up has to rotate on the way in.

Travel direction follows right-hand traffic: negative-id lanes run with `+s`,
positive-id lanes against it.

## Untrusted input

A road the importer cannot interpret is skipped, not fatal -- losing a city map
to one junk road is the worse failure. `load_str` still errors if the document
yielded no lanes at all. Non-finite attribute values are rejected at parse:
Rust's float parser accepts `NaN` and turns `1e400` into infinity, and one such
value poisons every point derived from it.

## Visualization

Rendering is not in scope here, but the output is shaped for it.
`surface_mesh()` returns plain position, normal, and index buffers with no
engine types in them, and a `LaneSpan` per lane saying which slice of those
buffers it owns -- enough to pick the lane under a cursor or give one lane its
own material without re-tessellating.

The optional `serde` feature serializes the network and its mesh, for a viewer
in another process or a cached import. A `RoadNetwork` sends its lanes alone
and rebuilds its index on arrival, so what arrives behaves like a freshly
imported map.

```toml
libopendrive = { version = "0.1", features = ["serde"] }
```

## Public dependencies

`glam` is public: `Vec3` appears throughout the API. It is re-exported as
`libopendrive::glam` so you can match the version.

## Performance

Lane and surface lookups are answered off a ground-plane index, so their cost
tracks local road density rather than map size. On CARLA's Town07 (234 roads,
673 driving lanes):

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
