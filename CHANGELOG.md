# Changelog

## Unreleased

### Road neighbors

- `RoadNetwork::road_neighbors` lists each road `<neighbor>`, an element of
  OpenDRIVE 1.4 that 1.9 no longer has, as a `RoadNeighbor`: the road beside
  a road, its `Side`, and whether it runs the same way. The crate derives no
  lane changes from it. libOpenDRIVE reads it too, as raw strings, and
  checks none of them.
- `Warning::NeighborDropped` names one naming no baked road, or with a
  `side` or `direction` 1.4 doesn't allow.
- The viewer's lane readout lists the roads beside the lane's road.

### Junction priority

- `RoadNetwork::priorities` lists each junction `<priority>` as a
  `Priority`: the road with priority and the road that gives way to it.
  `RoadNetwork::yields_to` reads them for one road, and
  `Provenance::priorities` names each one's junction. They serialize with
  the network.
- Breaking: `Provenance` has a new `priorities` field, so a `Provenance`
  literal needs it. Provenance serialized before loads with none.
- `Warning::PriorityDropped` names a `<priority>` whose `high` or `low` is
  no road the load baked, and its `road_id` is the one the load has.
- The viewer's lane readout says which roads the lane's road gives way to,
  and which give way to it.

### Lane direction

- `feat!`: a lane's `direction` overrides the way its side of the road runs.
  `reversed` runs it the other way, and `both` makes it a new
  `Direction::Both` lane. `Direction` gains the variant, so a `match` on it
  without a wildcard no longer compiles.
- A two-way lane's successors are the lanes off both of its ends, and its
  predecessors those that drive into it at either end. From one, only lanes
  whose traffic runs away from the joint count. In ASAM's
  `Ex_Bidirectional_Junction`, road 2 now leads back into roads 5 and 6.
  Two two-way lanes that join lead into each other both ways, even where
  only one names the other.
- A lane of the deprecated `type="bidirectional"` with no `direction` runs
  both ways, as the spec says `direction="both"` replaces it.
- A lane change is only ever to a lane running the same way.
  `RoadNetwork::route` drives a two-way lane toward the next lane on the
  route, and `RoadNetwork::advance` goes each way from one.
- `Warning::UnknownLaneDirection` names a `direction` other than
  `standard`, `reversed` or `both`, which the crate reads as `standard`.
- A signal still picks its lanes by side, not by the way each runs.
- The viewer's lane readout says `both` for a two-way lane, and its arrow
  points both ways. The marker steps each way along one.

### Links across a gap

- `Warning::RoadLengthMismatch` names a road whose `length` is more than
  1 cm from where its `<planView>` ends. The crate still bakes the road to
  its `length`. ASAM's `UC_T_Junction` road 6 is 80 m long with 111.6 m of
  geometry.
- `Warning::LinkGap` names a lane link whose lanes are more than 10 cm
  apart where one leaves off and the next begins, measured across both
  lanes from border to border. A lane that splits in two, or hands over to
  one opening beside it, meets it and raises none. The crate keeps the
  link. It catches the four ASAM examples that link lanes that don't meet,
  and `UC_5Road_Junction`'s 1 m step.
- The check measures the lanes as baked. In `Ex_CrossFall_LeftTurn` the
  file switches lanes' `level` at a seam, and the crate stacks level lanes'
  heights, so those lanes step 0.12 to 0.36 m there and warn too.

### Junctions

- Fix: a road that a common junction's connection names as its incoming
  road, but whose own `<link>` leaves the junction out, drives into it. The
  crate finds the end that meets the junction from the connecting road's
  link back to it, and raises `Warning::JunctionLinkMissing`, with the
  `RoadEnd` it linked. In ASAM's `UC_X_Junction`, road 82's three driving
  lanes into junction 1 no longer dead-end.

### Comparison

- `docs/comparison.md` sets each of the 671 elements and attributes in the
  OpenDRIVE 1.9 schema beside what this crate, esmini 3.8.2, libOpenDRIVE
  0.5.0 and CARLA 0.10.0 do with it, with the source line for each. 31
  rows have another library doing more, mostly names the others keep and
  signal logic. It also compares the queries each answers.

### Lane visibility

- `RoadNetwork::lane_visibility` and `lane_visibility_at` read each lane's
  `<visibility>`s as `Along` stretches of a `Visibility`: the distance a
  driver can see ahead, behind, left and right, in metres. Each holds to
  the next or to the end of its lane section.
- OpenDRIVE 1.9 does not define `<visibility>`. The crate reads it as
  CARLA does, and keeps the four distances as the file names them.
- `Warning::VisibilityDropped` names a `<visibility>` with a distance
  missing, not a number, or below 0. The crate drops it. CARLA reads a
  missing one as 0.
- The viewer's lane readout shows the visibility at the hovered point.

### Moving along the lanes

- `RoadNetwork::advance` moves a `LanePosition` a distance along the lanes,
  the way their traffic runs, or back for a negative distance. It gives
  every place the distance reaches, one `Advance` per branch, as CARLA
  does. A branch that runs out of lanes is an `Advance::DeadEnd` at the
  lane's end, with the distance left.
- The distance runs along the lanes' centerlines, so it is how far a
  vehicle travels. esmini and CARLA step by road `s`. The offset keeps its
  side of the traffic onto a road that runs the other way.
- The branches grow exponentially with the distance on a map with loops:
  from one lane of Town07, 72,912 places at 2 km. Step a few metres at a
  time. An infinite or NaN distance gives none.
- The crate doesn't pick a branch by a heading or a route, as esmini can.
  The caller gets them all and picks.
- `RoadNetwork::left_of` and `right_of` step to the lane beside, left and
  right of the traffic, at the same `s`.
- In the viewer, click a lane to drop a marker, and press `f` or `b` to
  step it along the lanes. It splits at a fork and turns red at a dead end.

### Lane positions

- `RoadNetwork::lane_point` turns a `LanePosition`, a lane with a road `s`
  and an offset from its center toward `+t`, into the point on the lane's
  surface. `RoadNetwork::lane_position` turns a point back, on the lane of
  any type whose borders hold its road `t`. Both are exact, and follow
  esmini's `SetLanePos`.
- `RoadNetwork::centerline_s` turns a lane position's road `s` into the
  distance along the lane's baked centerline, which `Projection`, the
  `Along` stretches and `Lane::sample_at` use. The two drift apart on a
  bend: by up to 21 % of the distance across the test corpus.
- `Projection` and the `Along` stretches stay on the centerline's `s`.
  `nearest_lane`'s point is within 1.1 cm of the exact center for 99 % of
  the points tried, so moving them to road `s` would break every caller
  for no gain in where they land.
- The viewer's lane readout shows the lane offset and the centerline's `s`
  beside the road `s` and `t`.

### Road coordinates

- The network keeps its roads. `RoadNetwork::roads`, `road` and
  `road_by_od_id` return a `Road`, with its `RoadId`, `<road id>` and
  length. `RoadNetwork::road_lane` gives a lane's road, lane section and
  `<lane id>` as a `RoadLane`.
- `RoadNetwork::road_point` turns a `RoadPosition`, a road with `s` and
  `t`, into the point on the road surface there, lane heights and lateral
  shape included. `None` off the ends of the road. Signals, objects and
  road marks stand on the road through the same call.
- `RoadNetwork::road_position` turns a point back. Of the roads whose
  lanes come near it, it takes the one whose surface is nearest in 3D, and
  solves for the `(s, t)` straight under or over the point. A point on a
  lane round-trips to within 0.2 mm on every map in the test corpus, or the
  `f32` step of its coordinates where that is larger. A station on a lane
  section seam is on the section that starts there. It takes about 5 us on
  Town07. `RoadNetwork::road_position_on` does the same on one road, for a
  caller that knows its road where roads overlap.
- Roads serialize with the network, as the records the file gives. They
  add 22 % to Town07's JSON and 1 to 3 % to esmini's maps. A network
  serialized before deserializes with none, and the road queries answer
  `None`. Network equality compares roads too, so such a network, or one
  rebuilt from its lanes alone, no longer equals a fresh import.
- Roads that don't match the network's lanes, from `with_roads` or a
  hand-edited serialized network, give `None` from the road queries.
- Import takes about 4 % longer, to index every lane's footprint.
- The viewer's lane readout shows the road `s` and `t` under the mouse.

### Geo reference

- `RoadNetwork::geo_reference` returns a `GeoReference`: the
  `<geoReference>` PROJ string, trimmed, and the `<offset>` as a
  `GeoOffset`. The importer applies neither, so points stay in the file's
  frame. A missing or unreadable offset attribute reads as 0.
- `RoadNetwork::with_geo_reference` sets it. It serializes with the network,
  and a network serialized before it deserializes with none.
- The viewer shows the PROJ string and offset under the map's name.

### Level lanes

- A lane with `level="true"` is kept out of the superelevation and the
  lateral shape. It starts at its inner neighbour's outer border and runs
  level, with its own `<height>`s on top, so its `Lane::bank` is 0. The
  lanes outside it stack on it. Objects, signals and road marks stand on
  it, also where no lane in its section has a `<height>`. The A9 Testfeld
  map has 16, beside banked roads.
- A level lane is `w / cos φ` wide in plan on a road superelevated by `φ`,
  as libOpenDRIVE builds it: 2.5 mm more on a 2 m lane at 5 %.
- Where the crate departs from the spec:
  - A lane outside a level lane is held level even where the file says it
    is not, and raises `Warning::LaneNotLevel`. The spec says it is level.
  - A level lane starts at its inner neighbour's outer height, `<height>`
    included. libOpenDRIVE starts it on the road.

### Lane rules, access and materials

- `RoadNetwork::lane_rules`, `lane_access` and `lane_materials` hold each
  lane's `<rule>`s, `<access>`es and `<material>`s as `Along` stretches.
  `lane_rule_at`, `lane_access_at` and `lane_material_at` read them at a
  point. Each holds from its `sOffset` to the next of its kind, or to the
  end of its lane section.
- A rule is the file's free text. Access is `Access::Allow` or
  `Access::Deny`, with the road users the file names. A material is the
  `Material` objects already use.
- Access in the form before 1.8, one `restriction` attribute per
  `<access>`, merges where several share an `sOffset` and a `rule`. A
  deny of `none` lifts the restrictions before it.
- `Warning::AccessDropped` names an `<access>` whose `rule` is neither
  `allow` nor `deny`. The spec makes `rule` optional, but without it an
  `<access>` doesn't say who may use the lane.
- The viewer's lane readout gives the rule, access and material at the
  hovered point.

### Speed limits and road types

- `RoadNetwork::speed_limits` and `RoadNetwork::road_types` say what holds
  along each lane, as `Along` stretches in metres along its centerline.
  `speed_limit_at` and `road_type_at` read them at a point. A limit is a
  `SpeedLimit`, in m/s or `Unlimited`, and a type a `RoadType`.
- A road's `<type>` and its `<speed>` hold to the next `<type>`. A lane's
  `<speed>` overrides its road's to the end of its lane section. A
  `<speed>` without a `unit` is in m/s, as the spec says. Limits come from
  `<speed>`s, not signs.
- `Warning::SpeedLimitDropped` names a `<speed>` the crate can't read.
- The viewer's lane readout gives the speed limit and road type at the
  hovered point.
- Where the crate departs from the spec:
  - A `<speed>` whose `max` is below 0 or not a number, or whose `unit` is
    not `m/s`, `km/h` or `mph`, is dropped with the warning. A road's
    `no limit` and `undefined` are read. A lane's are not, as the spec
    allows them only on a road.
  - A `<type>` without `s` is skipped. A lane `<speed>` without `sOffset`,
    or with a negative one, starts at its lane section.
  - Types and speeds out of order are sorted.
  - A `type` the spec does not name is `RoadType::Unknown`.

### Direct junctions

- Fix: a `type="direct"` junction's connections link its roads. The crate
  required a `connectingRoad` on every `<connection>`, so it dropped every
  direct connection, which names a `linkedRoad` instead. On esmini's
  `soderleden.xodr`, the six drivable lanes that meet at junction 8 now
  link across it.
- `Warning::ConnectionDropped` names a connection without an
  `incomingRoad`, or without the road it leads into. The crate used to
  drop it silently.
- Where the crate departs from the spec:
  - A direct connection also links its linked road back into its incoming
    road, as esmini reads it, unless the junction gives that connection
    itself. The spec does not say.

### Left-hand traffic

- Fix: a road with `rule="LHT"` drives its left lanes along `+s` and its
  right lanes against it. The crate used to read every road as right-hand
  traffic, so on esmini's `e6mini-lht.xodr` every `Lane::direction`,
  successor and predecessor was reversed. A signal's `orientation` picks
  its lanes by the same rule.
- Where the crate departs from the spec:
  - A `rule` other than `RHT` or `LHT` reads as `RHT`, the spec's default,
    and raises `Warning::UnknownTrafficRule`. esmini also reads `lht` as
    left-hand traffic.
- The viewer's travel arrows and link colours follow the rule, and its
  sidebar lists the new warning.

### Warnings

- Breaking: `Provenance` has a new `warnings` field, so a `Provenance`
  literal no longer compiles. It lists what a load dropped from a bad file,
  in file order, as `Warning`s. A clean file has none.
- `Warning` is an enum with one variant per kind, marked `#[non_exhaustive]`.
  Each names where in the file it happened, and `Display` gives the message.
  It derives `serde` under the feature.
- `Warning::RoadSkipped` names a road with no finite `length`, no
  `<planView>`, or no `<geometry>` the crate can bake. `RoadSkipReason`
  says which.
- `Warning::LaneDropped` names a lane with no `<width>`, which the crate
  drops along with its road marks. The spec draws those marks.
- Elements the crate does not read raise no warning.
- The viewer lists the warnings at the top of its sidebar. Picking one
  lights the lanes of the road it names.
- `Warning::road_id` gives the road any warning happened on.

### Lane borders

- A lane's `<border>`s give the `t` of its outer border, measured from the
  reference line. The lane's width is its border less its inner
  neighbour's outer border, so `Lane::width` and `Lane::widths` follow it,
  and a lane can open out of nothing. Such lanes used to be dropped.
- A width lane outside a border lane stacks on its border. Road marks run
  along a border, and links and lane-change neighbours hold as for any lane.
- `Warning::LaneDropped` now means a lane with neither a width nor a border.
- Where the crate departs from the spec:
  - A lane with widths follows them, and a lane with only borders follows
    those, even in a lane section where other lanes have widths. The spec
    uses the widths when a section has both. Raises
    `Warning::WidthAndBorder`.
  - A border ignores `<laneOffset>`. The spec forbids the two together.
    Raises `Warning::BorderWithLaneOffset` where the offset is not 0.
  - A border that crosses inside the lanes within it gives its lane 0
    width. The spec forbids it. Raises `Warning::BorderCrossesInnerLane`.
  - A border without `a` is skipped, and a missing `sOffset`, `b`, `c` or
    `d` is 0, as for a width. The spec requires all five.
  - Borders out of order are sorted rather than dropped.
- The viewer's lane readout gives the lane's width at the hovered point.

### Lateral shapes

- A road's `<lateralProfile><shape>`s give its cross-section, such as a
  crown. Each lane border stands at the shape under it plus the lane's
  `<height>` there, along the road's normal, and the lane goes straight
  across between them. Heights go linearly along `s` between profiles.
  So a crowned road no longer imports flat.
- A curve inside a lane is lost to the straight chord: `c w² / 4` at its
  middle, 5.5 mm on a 3.5 m lane of a crown falling 2.5 % at 7 m out.
- Each profile's `s` is a station of its lane sections.
- Objects, signals and road marks stand on the shape as the mesh has it,
  straight across each lane rather than on the spec's curve. A center
  line mark lies flat on a crown's ridge.
- The viewer's lane readout gives the cross slope at the hovered point, in
  percent.
- A tilted lane's `Lane::width` and `Lane::widths` are the chord across
  its surface, so its edges land on its borders. The plan width left each
  edge short: 1.3 mm on a 2 m lane rising 0.1 m, 7.4 cm on a 3.5 m lane
  at 30 %.
- Where the crate departs from the spec:
  - The first profile holds before it. The spec's default there is 0.
  - A profile that starts inside the road holds its first shape's value
    out to the edge, and raises `Warning::ShapeShortOfRoad`. The spec
    says each profile covers the road.
  - An `attached` CRG over a shaped road answers without the shape.
  - A shape without `s` or `t` is skipped, and a missing `a`, `b`, `c` or
    `d` is 0. The spec requires all six. Shapes out of order are sorted.

## 0.3.1 - 2026-09-27

### Docs

- The README is shorter. The element-by-element OpenDRIVE support moved to
  `docs/support.md`, and the coordinate frame, point types, meshes, `serde`,
  bad input and timings moved to `docs/design.md`.
- The `serde` snippet asks for version 0.3, not 0.2.
- The list of ignored elements no longer says signals are ignored.

## 0.3.0 - 2026-09-27

### Lane heights

- A lane's `<height>`s raise it off the road, along the road's normal. Its
  centerline rises by the height halfway across it, and `Lane::bank` adds
  the slope from its inner border to its outer one. So the surface mesh and
  `Lane::sample_at` see a raised sidewalk. The step up to it stays open, as
  libOpenDRIVE and esmini leave it.
- Objects and signals stand on the lane at their point, and a road mark on
  its own lane. So a sidewalk's outer mark rises with it, and the kerb's
  mark stays on the road.
- `LaneProvenance::heights` gives a lane's `LaneHeight`, inner and outer,
  at each centerline vertex.
- A lane section has a station wherever a lane's heights change pace, so a
  1 m kerb ramp bakes 1 m long.
- The mesh tilts a raised lane rather than moving each edge to its height.
  For a 0.1 m rise over 2 m, an edge is under 0.1 mm off its height and
  1.3 mm inside its border. A quad on a kerb ramp is twisted, and its
  triangles cut the corner by up to a quarter of the rise.
- Where the crate departs from the spec:
  - Heights go straight from one entry to the next, as libOpenDRIVE and
    esmini read them, and the last holds after it. The spec's rule for
    lane geometry holds each until the next.
  - Heights go straight across the lane from `inner` to `outer`, as both
    readers do. The spec gives only the two borders.
  - The first entry holds before it. The spec gives no height there.
  - A missing `sOffset`, `inner` or `outer` is 0, as libOpenDRIVE reads
    it. The spec requires all three. A negative `sOffset` is 0.
  - Entries out of order are sorted rather than dropped.
  - Heights on the center lane are ignored. The spec forbids them.
  - Objects, signals and road marks stand on a raised lane. The spec does
    not say whether they stand there or on the road below.
  - A point on the border between two lanes is on the inner one, as in
    libOpenDRIVE, and a point past the outermost lane takes its outer
    height. The spec says neither.
  - An `attached` CRG over a raised lane answers at road level. The spec
    measures a height from the road including its surface.
- The viewer's lane readout gives a raised lane's inner and outer height
  at the hovered point.

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
- Where the crate departs from the spec:
  - A mark without a `color` is `standard`, one without a `type` is
    `none`, and one without an `sOffset` starts at its section. The spec
    requires all three.
  - A negative `sOffset` on a mark or a line is 0. The spec says it is at
    least 0.
  - A mark without a `weight` is standard, and a line without a `rule` has
    none. The spec gives no default for either.
  - A width of 0 counts as none, so the line's, the type's, the mark's and
    the weight's apply in turn. The spec says a width is above 0.
  - A line with a `length` and `space` of 0 is continuous, as esmini means
    it. The spec would paint nothing.
  - A `none` mark paints nothing even with lines, as esmini draws it.
  - `tOffset` and a sway point along +t on both sides of the road, and a
    sway moves nothing before its `ds`. Dashes are measured along the
    reference line. The spec says none of this.
  - Marks out of order are sorted rather than dropped.
  - A lane with no `<width>` does not bake, so its marks go too. The spec
    would draw them on its inner border.
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
- Hovering a lane draws an arrow the way traffic drives, tints its
  successors green and its predecessors orange, and lists them in the
  readout. `viewer_export` writes each lane's `successors` and
  `predecessors`.
- The sidebar lists each road's objects and signals under its lanes, in
  groups that fold. Click one to select it, frame it and read it. The
  filter matches them too, and the header names the map.

### Breaking

- The minimum Rust version is 1.85, up from 1.82, for the `opencrg`
  dependency.
- `Provenance` has new `signals`, `controllers` and `road_marks` fields, so a
  `Provenance` literal needs them or `..Default::default()`. The `serde`
  form of `RoadNetwork` has new `signals`, `controllers` and `road_marks`
  keys, so JSON written before does not load.
- `LaneProvenance` has a new `heights` field, so a literal needs it. It no
  longer derives `Eq`, since the heights are `f32`s.

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
