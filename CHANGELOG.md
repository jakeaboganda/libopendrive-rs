# Changelog

## Unreleased

### Objects

`<object>`s now import. Each one bakes to one or more `Object`s on
`RoadNetwork::objects`, in world coordinates, with an `ObjectId`, an
`ObjectType`, a subtype, a name, whether it is dynamic, and a `Shape`:

- `Shape::Solid`: a plain object, placed on the road surface at its `(s, t)`
  and raised by `zOffset`. It carries its heading (the road's plus its own),
  its pitch and roll, and an `Extent`: a cylinder if it has a radius, or a
  box if it has any of a length, a width and a height.
- A `<repeat>` with a `distance` is one solid every `distance` metres, with
  `t`, `zOffset` and the dimensions interpolated along it. On esmini's
  e6mini that turns four objects into 794 posts.
- `Shape::Sweep`: a `<repeat>` with a `distance` of 0, a cross-section swept
  along the road, such as a guard rail or a wall.
- `Shape::Outline`: one per `<outline>`, a polygon of `cornerRoad` or
  `cornerLocal` corners, each with a base and a top. An object with outlines
  gets no solid of its own. The 1.4 layout, `<outline>` straight under
  `<object>`, is read too.

`RoadNetwork::object_mesh` tessellates them into one `Mesh` of
outward-facing faces, with an `ObjectSpan` per object in `Mesh::objects`, as
`surface_mesh` does for lanes. A closed outline gets a lid and a floor, and a
face with no area is left out, so a post given only a height has no span.

`RoadNetwork::object` looks one up by its id. Its road id, `<object id>`,
anchoring `(s, t)`, `orientation` and `validLength` are in an
`ObjectProvenance`, kept apart from the object as a lane's are.

An `<objectReference>` bakes the `<object>` it names, from any road, at the
reference's `s`, `t` and `zOffset`. The object's repeats and `cornerRoad`
outlines move with it. The provenance carries the reference's `orientation`
and `validLength`, and `referenced_from` names the road the original is on. A
reference to an id no object has is skipped.

`Object::lanes` is the lanes an object applies to: those alongside the
stretch of road it spans, narrowed by its `<validity>` `fromLane`-`toLane`
ranges, or all of them if it has none. A reference takes its own
`<validity>`, not its object's.

`Object::markings` is the paint on an outline, one `Marking` per
`<marking>` whose `<cornerReference>`s all name its corners. It carries the
marking's side, colour, width, line length and space length, and the painted
pieces: quads in world coordinates along the referenced edges, dashed where
the marking is. A marking with no corner references is skipped.

These placements follow libOpenDRIVE. Not imported yet: `<tunnel>`,
`<bridge>`, and an outline's `outer` flag, so a hole bakes as a solid.

- The `serde` form of `RoadNetwork` is now `{ "lanes": [...], "objects": [...] }`
  rather than a bare lane array. JSON written by 0.1.1 does not load.
- `From<RoadNetwork> for Vec<Lane>` is gone, since it would drop the objects.
  Read `lanes()` instead.
- `Mesh` has a new `objects` field, so a `Mesh` literal needs it or
  `..Default::default()`.
- `load_str_with_provenance` and `load_file_with_provenance` return a
  `Provenance`, with the lane records in `lanes` and the object records in
  `objects`, instead of a `Vec<LaneProvenance>`.

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
