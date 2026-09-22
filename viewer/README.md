# OpenDRIVE viewer

A three.js viewer for a map baked by `libopendrive`. Hover a lane to read its
coordinates, browse roads and lanes in the sidebar, and filter them by id.

It has two parts: a Rust exporter (`examples/viewer_export.rs`) that bakes an
`.xodr` into `web/scene.json`, and a static page (`web/index.html`) that renders
it. The page is the only consumer of the crate's output; the crate itself does
no rendering.

## Run it

```sh
# 1. Bake a map to JSON (writes web/scene.json by default).
cargo run --example viewer_export --features serde -- tests/data/town07.xodr

# 2. Serve the folder and open it. fetch() needs HTTP, not file://.
cd viewer/web && python3 -m http.server 8000
# then open http://localhost:8000
```

Load a different scene without renaming it: `?scene=e6mini.json`.

## What you get

- **Hover** highlights the lane section under the cursor and shows a readout of
  `road id`, OpenDRIVE `lane id`, `s`, `section`, and `t`, plus the crate's
  internal `LaneId`.
- **Sidebar** lists every baked lane grouped by road. Click one to highlight
  and frame it.
- **Search** filters the list by road id or lane id.

## Coordinates

The frame is OpenDRIVE's own: right-handed, **Z-up**, metres. The camera's up
axis is +Z, so a coordinate on screen is the coordinate in the file.

`s` and `t` are measured against the **lane centerline**, matching
`Polyline::project`: `s` is arc length along the lane, `t` is signed lateral
offset, positive to the left of the lane's stored heading. This is not the
OpenDRIVE road-reference `t`; the baked network is lane-centric and does not
carry the reference line, so `t` here is offset from the lane, near zero at its
middle.

## How a hover resolves to a lane

`surface_mesh()` merges every lane into one buffer and records a `LaneSpan` per
lane: the slice of indices that lane owns. A raycast returns a triangle; the
viewer binary-searches the spans for the one whose index range contains it, and
that names the lane. `load_*_with_provenance` supplies the road id, section, and
original OpenDRIVE lane id for each `LaneId`.
