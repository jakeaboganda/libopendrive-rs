# OpenDRIVE support

What the importer reads from an `.xodr` file, and what it skips. For the exact
element and attribute list, see [the crate docs](https://docs.rs/libopendrive).

Geometry is cross-checked against the reference C++
[libOpenDRIVE](https://github.com/pageldev/libOpenDRIVE).

## Which OpenDRIVE version

The importer never reads `<header>`, so it never inspects `revMajor` or
`revMinor` and never rejects a file for its declared version. Whether a file
loads depends only on whether it uses the elements listed below.

Every one of those elements is in ASAM OpenDRIVE 1.9.0, the current revision.
1.9.0 deprecates `poly3` in favour of `paramPoly3`, and 1.8 deprecated a
signal's `<positionRoad>` and `<positionInertial>`. The importer reads them
like any other element and prints no warning. The test suite imports real
files declaring 1.4, 1.6 and 1.7.

## Roads and lanes

- Reference geometry: `line`, `arc`, `spiral` (clothoid), `paramPoly3`,
  `poly3`.
- `<elevationProfile>`, and `<lateralProfile>` superelevation baked as a real
  cant. The cross-section rolls about the reference line, so an outer lane
  rides higher and its surface normal leans.
- Per-lane widths, `laneOffset`, and multiple lane sections.
- The whole lane cross-section, the carriageway included. `LaneType` names
  every function the format defines, among them sidewalks, kerbs, ramps and
  tram track. An unrecognised name bakes as `LaneType::Unknown` rather than
  leaving a hole. `LaneType::is_drivable` separates the ones traffic uses.
- Lane widths that vary along a lane, so a gore area that opens out of a
  point is drawn as the wedge it is.
- Lane heights, so a sidewalk stands above the road beside it. A lane's
  `<height>`s raise its centerline and tilt its surface, so the mesh and
  `Lane::sample_at` see the kerb. Poles, signs and road marks on a sidewalk
  stand on it. The step up to it is left open, as libOpenDRIVE and esmini
  leave it. `load_*_with_provenance` gives the heights at each centerline
  vertex.
- Road and lane `<link>`s and `<junction>`s, resolved into a lane graph in
  the direction traffic drives.

## Objects

`RoadNetwork::objects` has every `<object>`, in world coordinates on the road
surface.

- A plain object is a box or a cylinder with a heading, pitch and roll. It
  leans with the grade and bank of the road under it, as in libOpenDRIVE.
- A `<repeat>` with a `distance` becomes one object per step, so a row of
  posts arrives as posts. A `<repeat>` with a `distance` of 0, such as a guard
  rail, becomes one shape swept along the road. With a radius it is a round
  pipe.
- An `<outline>`, in road or local coordinates, becomes a footprint polygon
  with a height at every corner. An `outer="false"` outline is a hole cut out
  of the outline round it. Under a `<repeat>` with a `distance`, the outline
  repeats at every step.
- An `<objectReference>` places the object it names again, at the
  reference's own station.
- A `<marking>`, such as a crosswalk's stripes, becomes painted quads along
  the outline edges it names, or along one side of the object's box if it
  names none. A `<border>`, such as a kerb, becomes a band along outline
  edges.
- `<validity>` picks the lanes an object applies to.
- `<parkingSpace>`, `<material>` and `<userData>` are kept on the object as
  data.

Each object has an id to look it up by, a subtype, and whether it moves.
`load_*_with_provenance` gives its road, OpenDRIVE id, `(s, t)`, orientation
and valid length.

## Tunnels and bridges

`RoadNetwork::structures` has each `<tunnel>` and `<bridge>` as the stretch of
each lane it covers. Call `structures_over(lane)` to find out whether a lane
runs through a tunnel or over a bridge, and where.

## Signals

`RoadNetwork::signals` has each `<signal>`: its catalogue codes, `value`,
`unit` and `text` as the file gives them, the lanes it applies to, and the
pose and size of its board.

- A board stands upright even on a banked road, and faces the traffic it
  addresses.
- A `<positionRoad>` or `<positionInertial>` moves the board, such as onto a
  gantry. The signal still applies where its `(s, t)` says.
- A `<signalReference>` applies the signal on another road too, adding the
  lanes there.
- `RoadNetwork::controllers` groups the signals that always show the same
  state, such as the lights of one approach. Each signal lists its
  controllers.
- A signal keeps its `<dependency>` and `<reference>` links as the ids of the
  signals and objects they name.

`load_*_with_provenance` gives its road, OpenDRIVE id, `(s, t)` and
orientation.

## Road marks

`RoadNetwork::road_marks` has the `<roadMark>`s on every lane border and the
center line. Each mark names the lanes either side of it, and keeps its type,
weight, colour, width, height and which way traffic may cross it. Its lines
are quads lying on the road surface.

- A mark's `<type><line>`s and `<explicit><line>`s are its lines, each with
  its own width, colour, dashes, offset and rule. Its `<sway>`s move them
  sideways.
- A mark without them gets stand-ins. A `solid` or `broken` mark paints one
  line and a double type two, with widths from libOpenDRIVE and dashes from
  esmini.
- A kerb, a grass edge and the other types that aren't paint keep their
  meaning and paint nothing.

The crate docs list where it departs from the spec, such as reading a mark
without a colour. `load_*_with_provenance` gives its road, lane section, lane
and stretch.

## OpenCRG surfaces

The importer keeps `<surface><CRG>` records on roads and junctions, in all
four modes, for elevation and friction. `RoadSurface` loads the OpenCRG files
they name and gives a vehicle model the height, normal and friction under a
point in `f64`. Off the CRG it answers from the surface mesh, which the CRG
does not change.

The crate parses CRG files with [`opencrg`](https://crates.io/crates/opencrg)
and re-exports it, so `libopendrive::opencrg` also reads a `.crg` file on its
own.

```rust
use libopendrive::{load_file, opencrg::CrgGrid, RoadSurface, SurfaceHint};

let net = load_file("maps/track.xodr")?;
let mesh = net.surface_mesh();
let surface = RoadSurface::new(&net, &mesh, |file| {
    CrgGrid::from_path(format!("maps/{file}")).ok()
});
let mut wheel = SurfaceHint::default();
let ground = surface.sample(12.0, -30.0, &mut wheel);
```

Public maps that come with CRG files are hard to find, so
`examples/crg_to_xodr.rs` writes one for any CRG file: a road along the
file's reference line, with the file laid on it. `examples/crg_data.sh`
downloads five measured and test-course files from ASAM and Project Chrono,
and writes a map and a viewer scene for each. `examples/crg_profile.rs`
drives a wheel down a lane and writes what it rolls over as CSV.

```sh
sh examples/crg_data.sh
cargo run --release --example crg_profile -- target/crg/country_road.xodr > profile.csv
```

## What it ignores

The importer silently skips everything else in the file, `<geoReference>`
included. Three omissions change the road you get back, not only the detail
around it:

- `<shape>`, the other `<lateralProfile>` child, so a crowned or cambered
  cross-section imports flat across its width.
- A lane's `<border>`, as opposed to an object's. A lane whose extent comes
  from a border rather than a width element has nothing to sample, so the
  importer drops it.
- `<center>`, so lane 0 never becomes a `Lane`.
