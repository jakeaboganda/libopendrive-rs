# Changelog

## Unreleased

### Road marks

- Each `<roadMark>` imports as a `RoadMark` on `RoadNetwork::road_marks`,
  with a `RoadMarkId`. It runs along its lane's outer border, or the line
  between the two sides for the center lane, to the lane's next mark or
  the end of its lane section. `RoadMark::left` and `RoadMark::right` name
  the lanes either side of it.
- A mark keeps its `type` as a `RoadMarkType`, its `laneChange` as a
  `LaneChange`, its `weight`, width and height, and its `color` as the
  file's text.
- Its `lines` are quads in the road surface. A mark described by its type
  alone paints what esmini draws for it: one line for `solid`, 4 m dashes
  8 m apart for `broken`, and two lines one width either side of the border
  for a double type. The other types paint nothing. A line is 0.12 m wide,
  or 0.25 m bold, as in libOpenDRIVE. The spec gives none of these.
- A mark's `<type><line>`s replace its stand-ins. Each `RoadMarkLine`
  keeps its width, colour, `LinePattern`, `sOffset`, `tOffset` and
  `LineRule`. A line without a width or colour takes its type's or mark's.
- An `<explicit><line>` paints once, as `LinePattern::Single`. Each
  `<sway>` moves the mark's lines sideways by its cubic, from its `ds`.
- `RoadMarkProvenance` gives each mark's road, lane section, `<lane id>`,
  `s` and length.
- Where the crate departs from the spec: a mark without a `color` is
  `standard`, one without a `type` is `none`, and one without an `sOffset`
  starts at its section, where the spec requires all three. A negative
  `sOffset` on a mark or a line is 0, where the spec says it is at least
  0. One without a
  `weight` is standard, and a line without a `rule` has none, which the
  spec gives no default for. A width of 0 counts as none, where the spec
  says it is above 0, so the line's, the type's, the mark's and the
  weight's apply in turn. A line with a `length` and `space` of 0 is
  continuous, as esmini means it, where the spec would paint nothing. A
  `none` mark paints nothing even with lines, as esmini draws it.
  `tOffset` and a sway point along +t on both sides of the road, which
  the spec does not say, and a sway moves nothing before its `ds`. Dashes are measured
  along the reference line, which the spec does not say. Marks out of
  order are sorted rather than dropped. A lane with no `<width>` does not
  bake, so its marks go too, where the spec would draw them on its inner
  border.
- The viewer paints each road mark's lines in their colour. Hover one to
  read the mark, each of its lines with its pattern and rule, and light up
  the lanes either side. `m` toggles road marks.

### Signals

- Each `<signal>` imports as a `Signal` on `RoadNetwork::signals`, with a
  `SignalId`. It keeps the file's `country`, `countryRevision`, `type`,
  `subtype`, `value`, `unit`, `text`, and the 1.9 flags `invalidated` and
  `temporary`. `type` and `subtype` stay the file's strings.
- A signal's board stands `zOffset` straight above the road at its
  `(s, t)`, faces the traffic its `orientation` names, and is turned by
  `hOffset`. Pitch and roll are against the horizontal, so a board on a
  banked road stays upright. Angles wrap into `(-π, π]`.
- `Signal::lanes` is the side of the road its `orientation` names, or the
  lanes its `<validity>` ranges name.
- A `<positionRoad>` or `<positionInertial>` moves the board, and the signal
  still applies at its own `(s, t)`. `Signal::applies_at` is where it takes
  effect.
- A `<signalReference>` adds its road's lanes and its point to the signal it
  names. `SignalProvenance::references` records each one.
- Each top-level `<controller>` imports as a `Controller` on
  `RoadNetwork::controllers`, with the signals it controls.
  `Signal::controllers` names a signal's controllers, and
  `ControllerProvenance` lists the junctions that sync each one.
- `Signal::dependencies` and `Signal::references` keep a signal's
  `<dependency>` and `<reference>` links, as the `SignalId`s and
  `ObjectId`s they name.
- `SignalProvenance` gives each signal's road, OpenDRIVE id, `(s, t)` and
  orientation.
- The viewer draws each signal as a board, white when static and dark when
  dynamic, with its front lit. Hover one to read it, see its lanes, and
  follow a dashed line to each place it applies, which may be on another
  road. The readout names its controllers and the other signals in each,
  and the signals and objects it depends on or refers to.
  `s` toggles signals.

### OpenCRG road surfaces

- Each `<CRG>` under a road's or a junction's `<surface>` imports as a
  `CrgSurface` in `RoadNetwork::crg_surfaces`. The importer reads all four
  modes, `attached`, `attached0`, `genuine` and `global`, and both purposes,
  elevation and friction.
- `RoadSurface` loads the files through a closure. `sample(x, y, hint)`
  returns the height, up-normal, CRG grid height and friction under a point,
  in `f64`. Where no CRG covers the point, the surface mesh answers. Give each
  moving point its own `SurfaceHint`, and its next search starts where the
  last one ended.
- Friction is the grid value exactly as the file has it. OpenCRG shifts
  heights to start at 0 and leaves friction alone.
- The crate re-exports `opencrg`.
- The viewer draws the CRG heights as a heat map over the road, centred on
  their median. Hover it to read the height and friction under the cursor.
- `examples/crg_to_xodr.rs` writes a map for an OpenCRG file.
  `examples/crg_profile.rs` drives a wheel down a lane and writes the
  surface under it as CSV. `examples/crg_data.sh` downloads five CRG files
  from ASAM and Project Chrono and prepares each for the viewer.

### Viewer

- A `map` picker in the toolbar switches between every map you have baked.
  The page opens on the map `?scene=` names, and asks for one without it.
- `viewer_export` takes any number of maps, such as `tests/data/*.xodr`,
  and names each output after its map instead of `scene.json`.

### Breaking

- The minimum Rust version is 1.85, up from 1.82, for the `opencrg`
  dependency.
- `Provenance` has new `signals`, `controllers` and `road_marks` fields, so a
  `Provenance` literal needs them or `..Default::default()`. The `serde`
  form of `RoadNetwork` has new `signals`, `controllers` and `road_marks`
  keys, so JSON written before does not load.

## 0.2.1 - 2026-09-26

### Performance

- `nearest_lane` and `sample_near` index each lane in spans of 4 segments
  rather than whole, and project only onto the spans near the point. Their
  cost no longer grows with how finely a lane is sampled, or how long it is.
  On Town07 they take about 0.4 us a call, where 0.2.0 took 1.4 us and 0.1.1
  took 0.7 us. The answers are the same, to the bit.

### Fixes

- 0.2.0 sampled a whole lane section every 25 cm if any part of it turned
  sharply, and probed every section every 25 cm to find out. Only the road
  near a tight curve is sampled finely now, easing back out to 2 m, and
  sections are probed every metre. On Town07 that is 14,209 lane samples
  where 0.2.0 had 19,518 and 0.1.1 had 8,818, and `nearest_lane` and
  `surface_mesh` are about a quarter faster than in 0.2.0. Import is
  23% faster on Town07 and 3.9 times faster on the test track. Tight
  curves are as round as in 0.2.0.

## 0.2.0 - 2026-09-26

### Objects

`<object>`s now import. Each one bakes to one or more `Object`s on
`RoadNetwork::objects`, in world coordinates, with an `ObjectId`, an
`ObjectType`, a subtype, a name, whether it is dynamic, and a `Shape`:

- `Shape::Solid`: a plain object, placed on the road surface at its `(s, t)`
  and raised by `zOffset`. It carries its heading, pitch and roll, and an
  `Extent`: a cylinder if it has a radius, or a box if it has any of a
  length, a width and a height. As in libOpenDRIVE, an object leans with
  the grade and bank of the road under it, and `zOffset` raises it square to
  the surface. So does an outline in `cornerLocal` corners. Sweeps and
  `cornerRoad` corners still rise straight up.
- A `<repeat>` with a `distance` is one solid every `distance` metres, with
  `t`, `zOffset` and the dimensions interpolated along it. On esmini's
  e6mini that turns four objects into 794 posts.
- `Shape::Sweep`: a `<repeat>` with a `distance` of 0, a cross-section swept
  along the road, such as a guard rail or a wall. Its sections are at most
  10 m apart, and closer where it bends, so its walls stay within 1 cm of the
  road. With a radius the sweep is `round`, a pipe, and the mesh is a
  16-sided tube.
- `Shape::Outline`: one per `<outline>`, a polygon of `cornerRoad` or
  `cornerLocal` corners, each with a base and a top. An object with outlines
  gets no solid of its own. The 1.4 layout, `<outline>` straight under
  `<object>`, is read too. A `<repeat>` with a `distance` bakes the outlines
  at every step, where libOpenDRIVE bakes them once.
- An `<outline>` with `outer="false"` is a hole in `Shape::Outline::holes`
  of the closed outline round it. The mesh cuts it out of the lid and the
  floor and walls it facing in.

`RoadNetwork::object_mesh` tessellates them into one `Mesh` of
outward-facing faces, with an `ObjectSpan` per object in `Mesh::objects`, as
`surface_mesh` does for lanes. A closed outline gets a lid and a floor in the
plane of its corners, so one standing on its edge, such as a sign, gets them
too. A face with no area is left out, so a post given only a height has no
span.

`RoadNetwork::object` looks one up by its id. Its road id, `<object id>`,
anchoring `(s, t)`, `orientation` and `validLength` are in an
`ObjectProvenance`, kept apart from the object as a lane's are.

An `<objectReference>` bakes the `<object>` it names, from any road, at the
reference's `s`, `t` and `zOffset`. The object's repeats and `cornerRoad`
outlines move with it. Its provenance has the reference's `orientation` and
`validLength`, and `referenced_from` names the road the original is on. The
importer skips a reference to an id no object has.

`Object::lanes` lists the lanes an object applies to. These are the lanes
alongside the stretch of road it spans, narrowed to the `fromLane`-`toLane`
range of each `<validity>` it has. A reference uses its own `<validity>`, not
its object's.

`Object::markings` has one `Marking` per `<marking>`. Each one
has the marking's side, colour, width, line length and space length. Its
pieces are world-space quads along the edges its `<cornerReference>`s name,
cut into dashes if the marking is dashed. A marking with no corner
references paints its `side` of a solid's box, such as a parking bay's
lines.

`Object::borders` has one `Border` per `<border>`, such as a traffic island's
kerb. Each one has the border's type and width, and one world-space quad per
edge. With `useCompleteOutline` the band runs along every edge of the
outline. Otherwise it follows the `<cornerReference>`s. Marking and border
quads lie in the surface under their edges, so on a banked road they tilt
with the bank.

`Object::parking_space` holds a `<parkingSpace>`'s `access`, such as
`handicapped`, and its free-text `restrictions`. `Object::materials` has one
`Material` per `<material>`, with its `surface`, `friction` and `roughness`.
`Object::user_data` keeps each `<userData>` `code` and `value` as text.

These placements follow libOpenDRIVE.

Breaking changes:

- The `serde` form of `RoadNetwork` is now
  `{ "lanes": [...], "objects": [...], "structures": [...] }` rather than a
  bare lane array. JSON written by 0.1.1 does not load.
- `From<RoadNetwork> for Vec<Lane>` is gone, since it would drop the objects.
  Read `lanes()` instead.
- `Mesh` has a new `objects` field, so a `Mesh` literal needs it or
  `..Default::default()`.
- `load_str_with_provenance` and `load_file_with_provenance` return a
  `Provenance`, with the lane records in `lanes`, the object records in
  `objects` and the structure records in `structures`, instead of a
  `Vec<LaneProvenance>`.

### Tunnels and bridges

Each `<tunnel>` and `<bridge>` bakes to a `Structure` on
`RoadNetwork::structures`. A `Structure` has a `StructureId`, a name and a
`StructureKind`. A tunnel's kind has its type, lighting and daylight, and a
bridge's has its type.

OpenDRIVE describes neither a tube nor a deck, so a structure has no
geometry. It lists the lanes it covers instead. Each `Coverage` says where
along one lane the structure starts and ends, in metres along that lane. On
a bend that differs from the road's `s`. `<validity>` narrows the lanes.

`RoadNetwork::structures_over(lane)` returns the structures over one lane.
`StructureProvenance` has each structure's road id, OpenDRIVE id, `s` and
`length`.

## 0.1.1 - 2026-09-23

### Lane types

`LaneType` named one function, `Driving`, so a map imported as its carriageway
and nothing else. It now names every function the format defines: sidewalks,
shoulders, kerbs, medians, parking, cycle lanes, bus and taxi lanes, the ramp
family, tram and rail, the unnamed `none` surface, and the three vendor-defined
`special` types. A name the crate does not recognise bakes as
`LaneType::Unknown`, so no lane is dropped for its type and the importer
cannot leave a hole in a map without naming it.

- `surface_mesh` tessellates every lane, not only the drivable ones, so a
  collider built from it now has the footway and the median in it. The mesh is
  bigger: Town07 goes from 673 lanes to 947.
- `driving_lanes`, `nearest_lane`, `sample_near` and `route` are unchanged, and
  still see `LaneType::Driving` alone. `LaneType::is_drivable` is the wider
  predicate, and it decides which lanes are lane-change neighbours, so a route
  can no longer be planned across a sidewalk.

### Lane widths that vary

`Lane` carried one width, sampled where the lane began. A lane that opens out
of a point, such as the gore area at an off-ramp, therefore reported a width of
0 m and tessellated to nothing. It was in the lane list and it owned a slice of
the mesh, but it covered no area. Town07 has four of those, and 64 lanes whose
width varies.

`Lane::widths` now holds a width per centerline vertex, parallel to
`Lane::bank` and empty on the constant-width lanes that are most of them.
`Lane::width_at` reads it, and the tessellator uses it per rib. `Lane::width`
stays, but now means the widest the lane gets rather than the width where it
starts.

### Fixed

- A lane section ending centimetres past its last sample left a pinched rib
  beside a full-width one with no room between them, and the quad folded. What
  counts as a stub is now relative to the samples around it rather than a fixed
  10 cm, which catches the case without flattening a finely sampled curve.
- Welding a stub vertex away moved the surviving vertex onto the lane's true
  endpoint but left it holding the stub's width, which opened a seam of half a
  millimetre at some section joints. Welding now selects vertices rather than
  rewriting them, so position, tangent, bank and width cannot drift apart.
- Lanes were sampled every 2 m however sharply they turned, so a tight
  junction curve came out as a polygon, turning 35° at a sample. A section's
  step now keeps every lane edge within 0.05 rad a sample, following the
  reference line and any widening or narrowing lane, down to 25 cm.

### Viewer

- Hovering reads out the surface point's `x`, `y`, `z`, the lane heading there,
  and the lane type. The surface normal joins them behind a toggle, which also
  draws a normal hair at every mesh vertex.
- Lane centerlines and lane boundaries each draw as their own overlay, the
  centerlines in a colour of their own. Boundaries come from the mesh itself:
  a lane's vertices alternate left rib and right rib, which `LaneSpan` now
  states as a promise.
- The surface is coloured by lane type, with a legend of the types the loaded
  map contains, and the lane filter matches on type. An `unknown` lane, or one
  of a type this page has no colour for, draws in magenta so a gap in either
  table is visible rather than silently shaded like a driving lane.

### API

- `Lane::widths` and `Lane::width_at`. `Lane::width` changes meaning from the
  width at the lane's start to the widest the lane gets; the two agree on any
  lane of constant width.
- `LaneType::is_drivable` and `LaneType::as_str`, plus `Display`.
- `Polyline::tangents` is public, so a consumer can reproduce `pose_at`'s
  heading at a vertex instead of re-deriving one from the chords.

## 0.1.0 - 2026-09-22

First release. Extracted from a simulator that had grown it in-tree, and
reworked for standalone use.

### Imports

- Reference geometry: `line`, `arc`, `spiral` (clothoid), `paramPoly3`,
  `poly3`.
- `<elevationProfile>`, and `<lateralProfile>` superelevation baked as a real
  cant about the reference line.
- Per-lane widths, `laneOffset`, and multiple lane sections.
- Road/lane `<link>`s and `<junction>`s, resolved into a drive-direction lane
  graph.

### Coordinates

Baked geometry is OpenDRIVE's own frame: right-handed, Z-up, metres, with the
reference line in the X-Y plane. A point imports verbatim.

Positions are `Point` and directions are `Vector`, both defined here. There is
no math crate in the public API, and no required dependency beyond `roxmltree`
and `thiserror`.

### Queries

- `nearest_lane`, `sample_near`, and `route` over the lane graph.
- `surface_mesh` tessellation, with `Mesh::validate` and `Mesh::height_at`.

### Changes since the in-tree version

- `glam::Vec3` no longer appears in the API. A position is a `Point` and a
  direction is a `Vector`, which makes `nearest_lane(sample.up)` and adding
  two positions compile errors rather than silent nonsense. glam is gone as a
  dependency, so it no longer constrains a consumer's version of it.
- The baked frame is Z-up, matching OpenDRIVE, where the in-tree version
  rotated to Y-up for its renderer. Every coordinate moves: `(x, y, z)`
  becomes `(x, -z, y)`. Note that `height_at`'s second argument changed
  meaning from `z` to `y` without changing type, so a call site that compiles
  is not evidence it is right.
- `RoadNetwork`'s lane list is private behind `new()` and `lanes()`, so the
  network can index itself with no way for the index to go stale.
- `nearest_lane` and the new `Mesh::sampler` answer off a ground-plane grid
  rather than scanning the map. On Town07: `nearest_lane` 67x faster,
  `sample_near` 64x, `route` 4.7x, `height_at` 168x.
- `Mesh::height_at` no longer reports a `NaN` height for a query far enough
  out to overflow its barycentric arithmetic.
- `Mesh` carries a `LaneSpan` per lane, naming the slice of the buffers that
  lane contributed.
- Optional `serde` feature for the network and its mesh.

### Not yet

`<lateralProfile>` `<shape>` (per-`t` crowning and camber), lane types other
than `driving`, and visualization.
