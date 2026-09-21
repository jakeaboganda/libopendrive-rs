# libopendrive

A pure-Rust OpenDRIVE (`.xodr`) importer. Reads a map file and bakes it into a
road network of polyline lanes, a drive-direction lane graph, and a triangle
surface mesh.

No C++ dependency, no bindings, no `unsafe`.

```rust
let net = libopendrive::load_file("maps/town07.xodr")?;

// Where is the road under this point, and which way does it lean?
let sample = net.sample_near(position).expect("on the map");

// Drive somewhere.
let waypoints = net.route(sample.point, destination).expect("a path");

// Hand the surface to a collider or a renderer.
let mesh = net.surface_mesh();
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

Baked geometry is right-handed, Y-up, metres, with the ground in the X-Z plane.
OpenDRIVE is right-handed Z-up, so the importer maps `(x, y, elev)` to
`(x, elev, -y)` and an OpenDRIVE left turn curves toward -Z.

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

## Public dependencies

`glam` is public: `Vec3` appears throughout the API. It is re-exported as
`libopendrive::glam` so you can match the version.

## Testing

Map fixtures are excluded from the published crate, so run the integration
tests from a git checkout. See `tests/data/README.md` for their provenance.

## License

MIT or Apache-2.0, at your option.
