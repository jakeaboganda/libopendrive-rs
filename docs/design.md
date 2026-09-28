# Design notes

How the baked network is laid out, and what the crate promises about it.

## Coordinate frame

Baked geometry is OpenDRIVE's own frame: right-handed, Z-up, metres, with the
reference line in the X-Y plane and elevation along +Z. A point imports
unchanged, so a coordinate you read out of the `.xodr` is the coordinate you
get back. An OpenDRIVE left turn curves toward +Y, and positive lane offset
`t` is to the left of the heading.

Travel direction follows each road's `rule`. Under right-hand traffic, the
default, negative-id lanes run with `+s` and positive-id lanes against it.
Under `rule="LHT"` it is the other way round.

## Coordinate types

A position is a `Point` and a direction is a `Vector`. They are separate types
with the arithmetic that relates them, so `point - point` is a `Vector`,
`point + vector` is a `Point`, and adding two positions does not compile.
Neither does `nearest_lane(sample.up)`, which passes a direction where a
position belongs. With one three-float type for both, it would compile.

Both are `#[repr(C)]` structs of three public `f32` fields, so handing one to
another math library is one call:

```rust
let v = glam::Vec3::from(point.to_array());
```

The required dependencies are `roxmltree`, `thiserror` and `opencrg`, which
has none of its own. Nothing here constrains which math or engine crate you
use, or its version.

## Meshes

`surface_mesh()` returns plain position, normal and index buffers with no
engine types in them, and a `LaneSpan` per lane saying which slice of those
buffers it owns. Buffers plus spans are enough to pick the lane under a
cursor, or give one lane its own material without re-tessellating. The viewer
uses the same spans to resolve a raycast hit to a lane.

`object_mesh()` does the same for objects: every box, cylinder, outline and
sweep tessellated into one mesh of outward-facing faces, with an `ObjectSpan`
per object.

## Serialization

The optional `serde` feature serializes the network and its mesh.
`examples/viewer_export.rs` uses it to bake maps to the JSON the viewer reads.
You can also use it to cache an import. A `RoadNetwork` serializes its lanes,
objects, structures, signals, road marks and CRG records, and rebuilds its
index when deserialized, so the result behaves like a freshly imported map.

```toml
libopendrive = { version = "0.3", features = ["serde"] }
```

## Untrusted input

A road the importer cannot interpret is skipped, not fatal. Losing a city map
to one junk road is the worse failure. `load_str` still errors if the document
yields no lanes at all.

`Provenance::warnings` says what the load did with a bad file. A `Warning` is
data, not a log line: one enum variant per kind, naming the road, lane section
or lane it happened at. A test can assert on it, and a caller can filter the
kinds it doesn't care about. The crate prints nothing and depends on no
logging crate. A clean file allocates no warnings.

The crate warns where it drops something, or reads a file that breaks a rule
of the spec. So far that is a skipped road, a lane with no `<width>` or
`<border>`, the lane borders the spec forbids, a lateral profile short
of the road, and a road `rule` the spec does not allow. It
doesn't warn for elements it doesn't read, such as `<userData>`, which real
maps are full of. [support.md](support.md) lists those.

The parser rejects non-finite attribute values as it reads them. Rust's float
parser accepts `NaN` and turns `1e400` into infinity, and one such value
poisons every point derived from it.

## Performance

Lane and surface lookups go through a ground-plane index, so their cost tracks
local road density rather than map size. On CARLA's Town07 (234 roads, 947
lanes, 673 of them driving):

| | per call |
| --- | --- |
| `nearest_lane` / `sample_near` | ~0.4 us |
| `route` (across the map) | ~29 us |
| `MeshSampler::height_at` | ~0.2 us |

`RoadSurface::sample` on a CRG takes ~0.3 us with a warm `SurfaceHint` and
~0.6 us without one, measured on `tests/data/crg.xodr`.

Import is ~15 ms for that map.

`cargo bench` reproduces these. `tests/budgets.rs` guards them in CI by racing
each indexed lookup against the scan it replaced. Racing needs no fixed
per-machine threshold, unlike timing a call directly.
