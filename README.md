# libopendrive

A pure-Rust OpenDRIVE (`.xodr`) importer. It reads a map and bakes it into
polyline lanes, a lane graph in the direction traffic drives, and a triangle
surface mesh. It also reads the OpenCRG (`.crg`) files a map lays on its
roads.

No C++ dependency, no bindings, no `unsafe`.

```rust
use libopendrive::{load_file, Point};

let net = load_file("maps/town07.xodr")?;

// Where is the road under this point, and which way does it lean?
let sample = net.sample_near(Point::new(12.0, -30.0, 0.0)).expect("on the map");
println!("{:?} banked {} rad", sample.point, sample.bank);

// Where is that on its road, in OpenDRIVE's own s and t?
let at = net.road_position(sample.point).expect("on a road");
println!("road {} s {} t {}", net.road(at.road).unwrap().od_id(), at.s, at.t);

// Drive somewhere.
let waypoints = net.route(sample.point, Point::new(280.0, 95.0, 0.0));

// Hand the surface to a collider or a renderer.
let mesh = net.surface_mesh();
mesh.validate()?;
```

## What it reads

- Road geometry, elevation, superelevation and lateral shape.
- Lanes of every type, with their widths, borders, offsets, heights and
  sections.
- Links and junctions, direct junctions included, as a lane graph, in
  right- or left-hand traffic.
- Road types and speed limits along each lane.
- Lane rules, access, materials and visibility along each lane.
- Objects, with repeats, outlines, markings and borders.
- Signals and the controllers that group them.
- Road marks.
- Tunnels and bridges.
- OpenCRG surfaces, for the height, normal and friction under a wheel.
- The geo reference: the PROJ string and offset, kept unapplied.

It keeps each road, so a caller can turn `(road, s, t)` or `(lane, s,
offset)` into a point on the road surface, and a point back, and move a
position along the lanes.

It loads a file whatever OpenDRIVE version it declares. Where it drops part
of a bad file, it says so in `Provenance::warnings`.
[docs/support.md](docs/support.md) covers each element, and what it skips.

## Viewer

The crate draws nothing. [`viewer/`](viewer/README.md) has a three.js page
that draws a baked map. Hover a lane, object or signal to read what the crate
knows about it.

```sh
cargo run --example viewer_export --features serde -- tests/data/*.xodr
cd viewer/web && python3 -m http.server 8000
```

Then open <http://localhost:8000> and pick a map from the `map` list.

![A traffic island's details in the viewer](viewer/objects.png)

It draws CRG heights as a heat map over the road. Here are ASAM's scanned
cobbles from `belgian_block.crg`, which `examples/crg_data.sh` downloads:

![Scanned cobbles from ASAM's belgian_block.crg](viewer/crg-cobbles.png)

## More

- [docs/design.md](docs/design.md): the coordinate frame, `Point` and
  `Vector`, mesh buffers, the `serde` feature, bad input, and timings. Lane
  lookups take about 0.4 us on CARLA's Town07.
- [docs/support.md](docs/support.md): what the importer does with each
  OpenDRIVE element.
- [CHANGELOG.md](CHANGELOG.md)

The published crate leaves out the map fixtures, tests and benchmarks, so run
them from a git checkout. `tests/data/README.md` says where each fixture comes
from.

MSRV is 1.85.

## License

MIT or Apache-2.0, at your option.
