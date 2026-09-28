# OpenDRIVE viewer

A three.js page that draws maps baked by `libopendrive`. Hover a lane, an
object, a signal, a road mark, a tunnel, a bridge or a CRG heat map to read
what the crate knows about it.

The viewer has two parts. `examples/viewer_export.rs` bakes `.xodr` maps
into JSON in `web/`, and lists them in `web/scenes.json`. `web/index.html`
draws the one you pick. The crate itself does no rendering.

![Hovering a lane in Town07, with its successor in green and its predecessor in orange](town07.png)

## Run it

1. Bake maps to JSON. Each goes to `viewer/web/`, named after its map, so
   this writes `viewer/web/town07.json`. Name as many maps as you like, such
   as `tests/data/*.xodr`.

   ```sh
   cargo run --example viewer_export --features serde -- tests/data/town07.xodr
   ```

2. Serve the folder. The page loads the JSON with `fetch()`, which needs
   HTTP, so opening `index.html` as a file won't work.

   ```sh
   cd viewer/web && python3 -m http.server 8000
   ```

3. Open <http://localhost:8000> and pick a map from the `map` list in the
   toolbar.

The list holds every map baked into `viewer/web/`. The URL names the map on
screen, as `?scene=town07.json`, so a link opens the same one. To bake to
another path, give one map and the output:

```sh
cargo run --example viewer_export --features serde -- tests/data/objects.xodr /tmp/objects.json
```

A map that fails to load is reported, the others still bake, and the
exporter exits with an error.

`tests/data/objects.xodr` is a small test map with one of everything the
viewer draws, so use it to try the features below.

## Controls

| Input | Action |
| --- | --- |
| drag | orbit |
| right-drag | pan |
| scroll | zoom |
| `w` | cycle the wireframe |
| `n` | toggle normals |
| `o` | toggle objects |
| `s` | toggle signals |
| `m` | toggle road marks |
| `c` | toggle the CRG heat map |
| pick in the `map` list | load another baked map |
| click a sidebar entry | select it and move the camera to it, or clear it if selected |
| click a lane | drop a marker there |
| `f` / `b` | step the marker 5 m along the lanes, or back |
| `Esc` | clear the marker |
| type in the filter box | filter lanes by road id, lane id or lane type, objects by type, subtype, name or id, signals by name, country, type, subtype or id, tunnels and bridges by name, kind or road, and warnings by their text |

The checkboxes along the top toggle centerlines (green), lane boundaries
(cream), normals (a hair at every mesh vertex), objects, tunnels, bridges,
signals, road marks, and the CRG heat map.

## Sidebar

The sidebar lists each road with its lanes, objects and signals, and the
tunnels and bridges above them. A group of 12 or fewer starts open. So on a
small map everything is listed, and on town07 you see 234 closed roads.
Filtering opens every group that matches.

At the top, in amber, are the load's warnings: what the crate dropped from a
bad file, such as a road with no geometry or a lane with no `<width>`. Click
one to light the lanes of the road it names and frame them. A skipped road
has no lanes, so the readout says so instead. A clean map has no warnings
group.

Click an object or a signal to select it. The camera frames it, and the
readout opens beside it. It stays outlined, with its lanes lit and a
signal's dashed lines drawn, while you hover other things. Click it again
to clear it.

Under the map's name, the sidebar shows its `<geoReference>` PROJ string
and `<offset>` as the file gives them. A map without either shows neither.

## Lanes

Hover a lane to highlight its lane section. A white arrow on its centerline
points the way traffic drives, so on a `backward` lane it runs against the
heading below. On a lane whose `direction` is `both` it points both ways. On a left-hand-traffic road the left lanes are the
`forward` ones. The lanes it leads to are green and the lanes that lead into
it are orange. While a lane is selected in the sidebar, its own links stay
lit as you hover others. The readout shows:

- the road id, the OpenDRIVE lane id and the lane type
- the surface point `x, y, z`
- the road `s` and `t` there, in OpenDRIVE's road coordinates
- the lane position: the offset from the lane's center toward `+t`, and how
  far along the lane's centerline the point is
- the lane's width at that point, which grows along a lane that opens out
  of nothing, whether its widths or its `<border>`s shape it. A tilted
  lane's is measured across its surface.
- the lane's heading
- its cross slope at that point, in percent up or down toward +t, from
  superelevation, lateral shape and lane heights together
- on a lane raised by its `<height>`s, such as a sidewalk, how far it stands
  off the road at its inner and outer border at that point
- the speed limit at that point, in km/h and mph, or `none` for no limit,
  and the road type, where the map gives them
- which roads the lane's road gives way to in its junction, and which give
  way to it, where the map gives junction priorities
- the roads the lane's road names beside it, on which side and which way
  they run, where the map gives `<neighbor>`s
- the lane's rule, who may use it (`only` the users an allow names, or
  `all but` those a deny names) and its material, where the map gives them
- how far a driver can see from the lane, forward, back, left and right,
  where the map gives its `<visibility>`
- its successors and predecessors, by road, lane section and OpenDRIVE
  lane id
- the crate's internal `LaneId`
- the surface normal, if normals are on

The surface is coloured by lane type, and the legend lists the types in this
map. A lane type the page has no colour for is magenta, so it stands out
instead of passing for a driving lane.

A raised sidewalk stands above the road beside it, with the step up to it
left open. Here, in esmini's `fabriksgatan_traffic_lights.xodr`, the
sidewalk is 0.12 m up and the traffic light stands on it.

![Hovering a sidewalk raised 0.12 m, with a traffic light standing on it](lane-heights.png)

## Moving along the lanes

Click a lane to drop a marker on it, and press `f` to step it 5 m along the
lanes the way their traffic runs, or `b` to step it back. At a fork it
splits, one blue square per branch, and a branch that runs out of lanes
turns red at the lane's end. The box at the top right lists each branch:
its road, lane, lane section and centerline `s`.

The page steps the marker as `RoadNetwork::advance` does, along the lanes'
centerlines and onto each successor or predecessor, from the lane lengths,
directions and links the exporter writes. It keeps the marker's offset to
the left of the traffic. On a lane whose `direction` is `both`, the marker
splits and steps each way.

## Objects

Each object is drawn where the crate places it, coloured by type. Anything
with an area comes from the crate's `object_mesh()`: boxes, cylinders,
outlines and sweeps. So a hole in an outline shows as a gap. The mesh leaves
out a post given only a height, which is drawn as a thin post, and a solid
with no size, which is a small diamond. An object type the page has no
colour for is cyan.

Hover an object to outline it and light up the lanes it applies to. The
readout shows:

- its type, subtype and name
- its road, OpenDRIVE id, `s`, `t`, orientation and valid length
- the lanes it applies to, by lane section and OpenDRIVE lane id
- its parking access and restrictions, materials, `<userData>` pairs,
  markings and border types, if it has them
- for a solid, its position, heading, pitch, roll and size
- for an outline or a sweep, the point under the cursor and how many corners
  or sections it has, how many holes an outline has, and whether a sweep is
  round

![Hovering a traffic island](objects.png)

An object placed by an `<objectReference>` draws the same as the original.
Its readout adds the road and id of the original.

Parking spaces are coloured by who may park there, their `access`, and have
their own legend group.

Markings are painted over the outline in their own colour, cut into dashes
where the map dashes them. Hovering paint picks the object under it. In the
picture below, the crosswalk has a dashed white marking on one edge and a
solid yellow one on the other. The two blue boxes are parking spaces.

![A crosswalk's markings and two parking spaces](markings.png)

Borders, such as a kerb, are bands along the outline, coloured by type.
Hover one for its type and width. Where two borders share an edge, the one
later in the file is drawn over the other.

## Tunnels and bridges

The viewer tints the part of each lane a tunnel or bridge covers, purple for
tunnels and cyan for bridges. Hover a tinted lane to add the tunnel's or
bridge's name, type and `s` range to the readout.

![Hovering a lane inside a tunnel](tunnel.png)

The sidebar lists tunnels and bridges above the lanes. Click one to
highlight its lanes and frame the stretch it covers.

## Junctions

Where a junction gives a `<boundary>` or an `<elevationGrid>`, the viewer
draws its ground in grey under the roads, and its boundary in amber. The
ground comes from `JunctionArea::mesh`, at the grid's height, so the
gaps between a junction's connecting roads are filled. Hover the ground to
read its junction, the point and its height. The `junctions` checkbox
hides both.

The crate does not move the junction's roads onto the grid, so where a
file's roads and grid disagree, the ground shows above or below the roads.

## Signals

Each signal is a board of its `width` and `height`, standing on the middle
of its bottom edge. Its front, the face turned to the traffic it addresses,
is drawn flat in the board's colour, and its back and sides are grey. A
static sign is white and a dynamic signal, such as a traffic light, is a
dark box. A board the map gives no size is 0.6 m square. There is no post,
since the map doesn't say how a signal is mounted. A pole is a separate
`<object>` when the map has one.

Hover a signal to outline it, light up the lanes it applies to, and draw a
dashed line from the board to the road where it takes effect. That is
straight down, unless the map stands the board somewhere else with a
`<positionRoad>` or `<positionInertial>`, such as on a gantry or beside
another road. Then the line runs across to where it applies. A
`<signalReference>` applies the signal on another road too. Its lanes light
up with the rest, and another dashed line runs to it. The readout shows:

- its name, and whether it is dynamic, invalidated or temporary
- its country, `type` and `subtype`, as the map spells them
- its value and unit, and its text
- its road, OpenDRIVE id, `s`, `t` and orientation
- each controller it belongs to, with its priority, the junctions that sync
  it, and the other signals it switches together with this one
- the signals it depends on, and the signals and objects it refers to, by
  name, OpenDRIVE id and link type
- the road, `s`, `t` and orientation of each `<signalReference>`
- the lanes it applies to, by road where there are several, lane section
  and OpenDRIVE lane id
- where it applies, if the board stands somewhere else
- the board's position, heading, pitch, roll and size

![Hovering a traffic light that also applies on another road](signals.png)

`tests/data/signals.xodr` has signs and lights with every feature the viewer
draws. Bake it and pick `signals`:

```sh
cargo run --example viewer_export --features serde -- tests/data/signals.xodr
```

## Road marks

Each road mark's lines are painted on the road in their colour, cut into
dashes where the crate dashes them, and moved sideways where the mark
sways. A mark that isn't paint, such as a kerb,
or a mark of type `none`, draws nothing. The legend lists the colours in
this map. A colour the page doesn't know is magenta.

Hover a line to outline its mark and light up the lanes either side of it.
The readout shows:

- its type and colour
- its road, lane section, and the OpenDRIVE lane whose border it runs
  along, or the center lane
- the stretch of road it covers, as `s` along the reference line
- its width, weight and height
- which way traffic may cross it
- the lanes on its left and right, looking along the road's `+s`, or the
  road edge where there is none
- each of its lines: continuous, its dash and gap lengths, or its length
  if it paints once, how far along the mark it starts and how far off the
  border it sits if not 0, its width and colour, and its rule unless that
  is `none`

![Hovering the double center line of the road mark test map](road-marks.png)

`tests/data/road_marks.xodr` has every road mark type on one road, a road
of `<type><line>` marks, and a road with `<explicit>` lines and a sway.
Bake it and pick `road_marks`:

```sh
cargo run --example viewer_export --features serde -- tests/data/road_marks.xodr
```

## OpenCRG surfaces

Where the map lays OpenCRG files on its roads, the exporter loads them from
beside the `.xodr`, and warns about any it cannot read. It samples
`RoadSurface` over every lane a CRG covers, and the viewer draws the result
as a heat map over the road. The colour is the CRG grid's own height, times
`zScale`, without the file's reference-line height or bank. White is the
median of those heights, blue is below it and red above. The scale saturates
at the 99.9th percentile of the distance from the median, and the legend
shows its ends. Grey is where only a friction CRG covers the road.

The viewer drapes the heat map 5 mm above the road mesh, so the road never
hides it. Its shape is the road mesh's, not the CRG surface's. Hover it to
read the CRG height, the surface height `z` and the friction under the
cursor, and the CRG files on the lane.

The grid is as fine as the finest CRG file, or coarser to keep to about
250,000 cells per file.

`tests/data/crg.xodr` lays the same generated file on four roads, one per
mode, and a friction file on the first. Bake it and pick `crg`:

```sh
cargo run --example viewer_export --features serde -- tests/data/crg.xodr
```

![The CRG test map from above](crg.png)

For measured surfaces, `examples/crg_data.sh` downloads five CRG files, writes
a map for each, and exports it to `viewer/web/NAME.json`. The scenes are 12
to 38 MB each. Pick `belgian_block` for a scan of cobbles, or `country_road`
for 569 m of a country road.

```sh
sh examples/crg_data.sh
```

![Scanned cobbles from ASAM's belgian_block.crg](crg-cobbles.png)

## Coordinates

The frame is OpenDRIVE's own: right-handed, Z-up, metres. The camera's up
axis is +Z, so a coordinate on screen is the coordinate in the file.

The heading is the lane's stored geometry direction at the hovered point,
not its travel direction. On a `backward` lane the two are opposite. The page
interpolates the exported tangents the way `Polyline::pose_at` does, so it
agrees with the crate at every vertex and at both ends.

The normal is the baked up-normal interpolated across the hit triangle, the
value `Mesh::height_at` reports. It varies smoothly across facet edges
instead of jumping at each one.

The road `s` and `t` are OpenDRIVE's own: `s` along the road's reference
line and `t` across it, positive to the left. The exporter gives each
centerline vertex its road `s` and `t` from `RoadNetwork::road_position_on`,
on the lane's own road. The page interpolates them along the lane, and adds
the hovered point's offset from the centerline, tilted with the lane.

A lane position shares the road's `s`. Its offset is from the lane's
center toward `+t`, as `RoadNetwork::lane_position` gives it, so it is near
zero in the middle of a lane. The centerline `s` is the distance along the
lane's baked centerline, as `Polyline::project` measures it and
`RoadNetwork::centerline_s` gives it. The speed limits and other stretches
along a lane use it. On a bend it drifts away from the road's `s`.

## How a hover finds its lane

`surface_mesh()` merges every lane into one buffer and records a `LaneSpan`
per lane, the slice of indices that lane owns. Sidewalks and medians get a
span too, so every lane is pickable. A raycast returns a triangle, and the
page binary-searches the spans for the one containing it. That span names
the lane. `load_*_with_provenance` supplies the road id, section and
OpenDRIVE lane id for each `LaneId`.

## Where the lane boundaries come from

The page builds them from the mesh it already has, not from the exporter. A
`LaneSpan`'s vertices alternate between the left and right edge, one pair per
cross-section. The even vertices trace the left boundary and the odd ones
the right. Exporting the same lines again would double the lane data for
nothing.
