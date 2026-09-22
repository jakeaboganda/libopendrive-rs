# Changelog

## 0.1.0 -- unreleased

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
