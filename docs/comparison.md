# Comparison with esmini, libOpenDRIVE and CARLA

This table compares what this crate does with each element and attribute of
ASAM OpenDRIVE 1.9.0 against three other open-source readers. It was drawn
up on 2026-09-29, from these releases:

| Library | Release | Commit | Files read |
| --- | --- | --- | --- |
| this crate | the commit that adds this file | | `src/parse/` |
| [esmini](https://github.com/esmini/esmini/tree/v3.8.2) | v3.8.2 | `61b44a7` | `EnvironmentSimulator/Modules/RoadManager/RoadManager.cpp` |
| [libOpenDRIVE](https://github.com/pageldev/libOpenDRIVE/tree/0.5.0) | 0.5.0 | `d88e6ab` | `src/`, mainly `OpenDriveMap.cpp` |
| [CARLA](https://github.com/carla-simulator/carla/tree/0.10.0) | 0.10.0 | `ada75f9` | `LibCarla/source/carla/opendrive/parser/`, and `road/` for what it builds |

The rows are every element and attribute reachable from `<OpenDRIVE>` in the
1.9.0 XSD schema files, 671 of them. The right and left lanes share a row,
as do a link's predecessor and successor. `<dataQuality>`, `<include>` and
`<userData>`, which the schema allows under almost every element, have a row
each at the end.

Each cell says what the library does with the row:

- **builds**: it computes geometry or behaviour from it: road and lane
  positions, heights, meshes, the lane graph, travel direction, junction
  links, where an object or a signal stands, or a limit or rule resolved
  onto each lane.
- **stores**: it reads the value and keeps it for the caller, but derives
  nothing from it.
- **-**: it never reads it.

An element builds if anything under it builds. A cell cites the source line
that reads the value. This crate's column comes from the element table in
the crate docs, which `tests/documented_support.rs` checks against the
parser. The other three columns were read from each library's parser source,
and a sample of rows checked by hand against it. A parser read that only
logs that the element isn't supported counts as ignored.

The claim this crate aims at, reading more of OpenDRIVE than any other
open-source library, holds when no row has another library doing more. It
does not hold yet: 31 rows do. They are listed below.

## Summary

Rows each library reads, of the 671 in the table. An element and each of its attributes is its own row.

| | builds | stores | ignores |
| --- | --- | --- | --- |
| this crate | 341 | 73 | 257 |
| esmini | 176 | 54 | 441 |
| libOpenDRIVE | 146 | 57 | 468 |
| CARLA | 124 | 76 | 471 |

## Where another library does more

These are the rows where at least one other library builds or stores more
than this crate. Each is on the roadmap, or out of scope for a reason given
here:

- **Header offset.** esmini applies `<offset>` to every point. The crate
  keeps it for the caller and applies nothing, since reprojecting is out of
  its scope. See [Geo reference](https://docs.rs/libopendrive/latest/libopendrive/#geo-reference).
- **Signal meaning.** esmini looks a signal's `country`, `type`, `subtype`
  and `value` up in a catalogue, and esmini and CARLA pick traffic lights by
  them. The crate keeps the codes and looks nothing up.
- **Lane change.** CARLA lets a waypoint change lanes by the road mark's
  `laneChange`. The crate keeps it on the road mark. The lane-change query
  on the roadmap would build on it.
- **Names and descriptive attributes.** A road's and a junction's `name`,
  the header's `revMajor` and `revMinor`, a road mark's `material` and its
  type's `name`, an outline's `fillType` and `laneType`, and the center
  lane's `id`, `type`, `level` and `<link>`. Others keep them. The crate
  doesn't yet.
- **Object `type` and `name` as signals.** CARLA turns RoadRunner objects
  named `Speed_*` or `Stencil_STOP` into signals. That is one exporter's
  convention, not OpenDRIVE.
- **`<userData>`.** esmini textures, colours and models objects and
  tunnels from it. The crate keeps an object's pairs and draws nothing.

| Element | This crate | Another library |
| --- | --- | --- |
| &lt;header&gt; | stores | esmini builds `RoadManager.cpp:3791` (offset applied) |
| &lt;header&gt; `@revMajor` | - | esmini stores `RoadManager.cpp:3794` |
| &lt;header&gt; `@revMinor` | - | esmini stores `RoadManager.cpp:3795` |
| &lt;header&gt; &lt;offset&gt; | stores | esmini builds `RoadManager.cpp:3808` (offset shifts geometry) |
| &lt;header&gt; &lt;offset&gt; `@x` | stores | esmini builds `RoadManager.cpp:3808` |
| &lt;header&gt; &lt;offset&gt; `@y` | stores | esmini builds `RoadManager.cpp:3809` |
| &lt;header&gt; &lt;offset&gt; `@z` | stores | esmini builds `RoadManager.cpp:3810` |
| &lt;header&gt; &lt;offset&gt; `@hdg` | stores | esmini builds `RoadManager.cpp:3811` |
| &lt;road&gt; `@name` | - | esmini stores `RoadManager.cpp:3829`; libOpenDRIVE stores `OpenDriveMap.cpp:164`; CARLA stores `RoadParser.cpp:124` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@material` | - | libOpenDRIVE stores `OpenDriveMap.cpp:433`; CARLA stores `LaneParser.cpp:68` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@laneChange` | stores | CARLA builds `LaneParser.cpp:70` (lane change permission for waypoints) |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; `@name` | - | esmini stores `RoadManager.cpp:4515`; libOpenDRIVE stores `OpenDriveMap.cpp:442`; CARLA stores `LaneParser.cpp:79` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; &lt;lane&gt; `@id` | - | esmini builds `RoadManager.cpp:4288`; libOpenDRIVE builds `OpenDriveMap.cpp:383`; CARLA builds `RoadParser.cpp:206` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; &lt;lane&gt; `@type` | - | esmini stores `RoadManager.cpp:4190`; libOpenDRIVE stores `OpenDriveMap.cpp:388`; CARLA stores `RoadParser.cpp:207` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; &lt;lane&gt; `@level` | - | libOpenDRIVE builds `OpenDriveMap.cpp:388` (lane 0 level only matters exactly on center line); CARLA stores `RoadParser.cpp:208` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; &lt;lane&gt; &lt;link&gt; | - | esmini builds `RoadManager.cpp:4305`; libOpenDRIVE builds `OpenDriveMap.cpp:391` (same lane-link code as side lanes); CARLA stores `RoadParser.cpp:211` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@type` | stores | CARLA builds `ObjectParser.cpp:34` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@name` | stores | CARLA builds `ObjectParser.cpp:35` (name prefixes create signals) |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; `@fillType` | - | libOpenDRIVE stores `OpenDriveMap.cpp:594` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; `@laneType` | - | libOpenDRIVE stores `OpenDriveMap.cpp:595` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@name` | stores | CARLA builds `SignalParser.cpp:50` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@dynamic` | stores | esmini builds `RoadManager.cpp:4856` (dynamic selects TrafficLight) |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@country` | stores | esmini builds `RoadManager.cpp:4897` (country for type lookup) |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@countryRevision` | stores | esmini builds `RoadManager.cpp:4901` (revision affects TrafficLight choice) |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@type` | stores | esmini builds `RoadManager.cpp:4917`; CARLA builds `SignalParser.cpp:55` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@subtype` | stores | esmini builds `RoadManager.cpp:4918` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@value` | stores | esmini builds `RoadManager.cpp:4919` |
| &lt;junction&gt; `@name` | - | esmini stores `RoadManager.cpp:5493`; libOpenDRIVE stores `OpenDriveMap.cpp:98`; CARLA stores `JunctionParser.cpp:47` |
| &lt;userData&gt; | stores | esmini builds `RoadManager.cpp:5397` (object userData: texture, color; tunnel generate3DModel) |
| &lt;userData&gt; `@code` | stores | esmini builds `RoadManager.cpp:5399` |
| &lt;userData&gt; `@value` | stores | esmini builds `RoadManager.cpp:5402` |

## Queries

What each library answers about the map it built, beyond reading it.

| Query | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| Road `(s, t)` to a point | `road_point`, `network.rs:604`, on the lane surface | `Position::SetTrackPos`, `RoadManager.cpp:10099` | `Road::get_surface_pt`, `Road.cpp:139` | - (a waypoint has no `t`) |
| A point to road `(s, t)` | `road_position`, `network.rs:620` | `Position::XYZ2TrackPos`, `RoadManager.cpp:9048` | reference line `s` only: `RefLine::match`, `RefLine.cpp:86` | - |
| Lane, `s` and offset to a point | `lane_point`, `network.rs:675` | `Position::SetLanePos`, `RoadManager.cpp:10757` | - | lane center only: `Map::ComputeTransform`, `Map.cpp:273` |
| A point to lane, `s` and offset | `lane_position`, `network.rs:695` | `Position::XYZ2TrackPos`, `RoadManager.cpp:9048` | lane from `(s, t)`: `LaneSection::get_lane`, `LaneSection.cpp:35` | snapped to a lane center: `Map::GetWaypoint`, `Map.cpp:212` |
| Move a distance along the lanes | every branch: `advance`, `advance.rs:58` | one branch: `Position::MoveAlongS`, `RoadManager.cpp:10547` | - | every branch: `Map::GetNext`, `Map.cpp:554` |
| Step to the lane beside | `left_of`, `right_of`, `advance.rs:124` | - (`SetLanePos` with another lane) | - | `Map::GetLeft`, `Map.cpp:636` |
| Route between two points | `route`, `route.rs:32` | `RoadPath::Calculate`, `RoadManager.cpp:6199` | lane keys only: `RoutingGraph::shortest_path`, `RoutingGraph.cpp:40` | - (not in `LibCarla/road`) |
| Road surface mesh | `surface_mesh`, `mesh.rs:276` | - (not in `RoadManager`) | `OpenDriveMap::get_road_network_mesh`, `OpenDriveMap.cpp:682` | `Map::GenerateMesh`, `Map.cpp:1005` |
| Object mesh | `object_mesh`, `object_mesh.rs:58` | - (not in `RoadManager`) | `Road::get_road_object_mesh`, in the network mesh | - |

## Elements from before 1.9

Two elements some readers take are not in the 1.9 schema, so they have no
row above:

- A road's `<link><neighbor>`, removed before 1.9. libOpenDRIVE reads it
  (`OpenDriveMap.cpp:202`), and so does the crate.
- A lane's `<visibility>`. CARLA reads it (`LaneParser.cpp:139`), and so
  does the crate.

## Every element and attribute

`-` means the library ignores it. A cell cites the line that reads it, in the file the column heading names.


### &lt;OpenDRIVE&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;OpenDRIVE&gt; | builds | builds `RoadManager.cpp:3762` | builds `OpenDriveMap.cpp:71` | builds `OpenDriveParser.cpp:38` |

### &lt;header&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;header&gt; | stores `mod.rs:982` | builds `RoadManager.cpp:3791` | stores `OpenDriveMap.cpp:73` | stores `GeoReferenceParser.cpp:66` |
| &lt;header&gt; `@revMajor` | - | stores `RoadManager.cpp:3794` | - | - |
| &lt;header&gt; `@revMinor` | - | stores `RoadManager.cpp:3795` | - | - |
| &lt;header&gt; `@name` | - | - | - | - |
| &lt;header&gt; `@version` | - | - | - | - |
| &lt;header&gt; `@date` | - | - | - | - |
| &lt;header&gt; `@north` | - | - | - | - |
| &lt;header&gt; `@south` | - | - | - | - |
| &lt;header&gt; `@east` | - | - | - | - |
| &lt;header&gt; `@west` | - | - | - | - |
| &lt;header&gt; `@vendor` | - | - | - | - |
| &lt;header&gt; &lt;geoReference&gt; | stores `mod.rs:987` | stores `RoadManager.cpp:3804` | stores `OpenDriveMap.cpp:74` | stores `GeoReferenceParser.cpp:66` |
| &lt;header&gt; &lt;offset&gt; | stores `mod.rs:992` | builds `RoadManager.cpp:3808` | - | - |
| &lt;header&gt; &lt;offset&gt; `@x` | stores `mod.rs:995` | builds `RoadManager.cpp:3808` | - | - |
| &lt;header&gt; &lt;offset&gt; `@y` | stores `mod.rs:996` | builds `RoadManager.cpp:3809` | - | - |
| &lt;header&gt; &lt;offset&gt; `@z` | stores `mod.rs:997` | builds `RoadManager.cpp:3810` | - | - |
| &lt;header&gt; &lt;offset&gt; `@hdg` | stores `mod.rs:998` | builds `RoadManager.cpp:3811` | - | - |
| &lt;header&gt; &lt;license&gt; | - | - | - | - |
| &lt;header&gt; &lt;license&gt; `@name` | - | - | - | - |
| &lt;header&gt; &lt;license&gt; `@spdxid` | - | - | - | - |
| &lt;header&gt; &lt;license&gt; `@text` | - | - | - | - |
| &lt;header&gt; &lt;license&gt; `@resource` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; `@type` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;speed&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;speed&gt; `@type` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;speed&gt; `@value` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;speed&gt; `@unit` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;lane&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;lane&gt; `@type` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;priority&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;priority&gt; `@type` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;prohibited&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;prohibited&gt; &lt;animal&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;prohibited&gt; &lt;person&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;prohibited&gt; &lt;person&gt; &lt;type&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;prohibited&gt; &lt;vehicle&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;prohibited&gt; &lt;vehicle&gt; &lt;type&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;warning&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;routing&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;streetname&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;parking&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;tourist&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryTime&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryTime&gt; `@type` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryTime&gt; `@value` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryAllows&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryAllows&gt; &lt;animal&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryAllows&gt; &lt;person&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryAllows&gt; &lt;vehicle&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryProhibits&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryProhibits&gt; &lt;animal&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryProhibits&gt; &lt;person&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryProhibits&gt; &lt;vehicle&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryDistance&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryDistance&gt; `@type` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryDistance&gt; `@value` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryDistance&gt; `@unit` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryEnvironment&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryEnvironment&gt; `@type` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryExplanatory&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;signalRegulations&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;signalRegulations&gt; `@type` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;signalRegulations&gt; `@subtype` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;signalRegulations&gt; &lt;semantics&gt; | - | - | - | - |

### &lt;road&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;road&gt; | builds `mod.rs:396` | builds `RoadManager.cpp:3830` | builds `OpenDriveMap.cpp:147` | builds `RoadParser.cpp:119` |
| &lt;road&gt; `@name` | - | stores `RoadManager.cpp:3829` | stores `OpenDriveMap.cpp:164` | stores `RoadParser.cpp:124` |
| &lt;road&gt; `@length` | builds `mod.rs:1026` | builds `RoadManager.cpp:3832` | builds `OpenDriveMap.cpp:162` | builds `RoadParser.cpp:125` |
| &lt;road&gt; `@id` | builds `mod.rs:426` | builds `RoadManager.cpp:3830` | builds `OpenDriveMap.cpp:150` | builds `RoadParser.cpp:123` |
| &lt;road&gt; `@junction` | builds `mod.rs:461` | builds `RoadManager.cpp:3836` | stores `OpenDriveMap.cpp:163` | builds `RoadParser.cpp:126` |
| &lt;road&gt; `@rule` | builds `mod.rs:963` | builds `RoadManager.cpp:3846` | stores `OpenDriveMap.cpp:155` | - |
| &lt;road&gt; &lt;link&gt; | builds `links.rs:261` | builds `RoadManager.cpp:3958` | builds `OpenDriveMap.cpp:175` | builds `RoadParser.cpp:129` |
| &lt;road&gt; &lt;link&gt; &lt;predecessor\|successor&gt; | builds `links.rs:277` | builds `RoadManager.cpp:3958` | builds `OpenDriveMap.cpp:175` | builds `RoadParser.cpp:129` |
| &lt;road&gt; &lt;link&gt; &lt;predecessor\|successor&gt; `@elementId` | builds `links.rs:272` | builds `RoadManager.cpp:2579` | builds `OpenDriveMap.cpp:179` | builds `RoadParser.cpp:132` |
| &lt;road&gt; &lt;link&gt; &lt;predecessor\|successor&gt; `@elementType` | builds `links.rs:266` | builds `RoadManager.cpp:2576` | builds `OpenDriveMap.cpp:181` | - |
| &lt;road&gt; &lt;link&gt; &lt;predecessor\|successor&gt; `@contactPoint` | builds `links.rs:253` | builds `RoadManager.cpp:2583` | builds `OpenDriveMap.cpp:190` | - |
| &lt;road&gt; &lt;link&gt; &lt;predecessor\|successor&gt; `@elementS` | - | - | - | - |
| &lt;road&gt; &lt;link&gt; &lt;predecessor\|successor&gt; `@elementDir` | - | - | - | - |
| &lt;road&gt; &lt;type&gt; | builds `properties.rs:76` | builds `RoadManager.cpp:3857` | stores `OpenDriveMap.cpp:213` | stores `RoadParser.cpp:140` |
| &lt;road&gt; &lt;type&gt; `@s` | builds `properties.rs:79` | builds `RoadManager.cpp:3921` | stores `OpenDriveMap.cpp:215` | stores `RoadParser.cpp:143` |
| &lt;road&gt; &lt;type&gt; `@type` | stores `properties.rs:76` | stores `RoadManager.cpp:3861` | stores `OpenDriveMap.cpp:216` | stores `RoadParser.cpp:144` |
| &lt;road&gt; &lt;type&gt; `@country` | - | - | - | - |
| &lt;road&gt; &lt;type&gt; &lt;speed&gt; | builds `properties.rs:83` | builds `RoadManager.cpp:3928` | stores `OpenDriveMap.cpp:221` | stores `RoadParser.cpp:147` |
| &lt;road&gt; &lt;type&gt; &lt;speed&gt; `@max` | builds `properties.rs:37` | builds `RoadManager.cpp:3928` | stores `OpenDriveMap.cpp:223` | stores `RoadParser.cpp:149` |
| &lt;road&gt; &lt;type&gt; &lt;speed&gt; `@unit` | builds `properties.rs:42` | builds `RoadManager.cpp:3929` | stores `OpenDriveMap.cpp:224` | stores `RoadParser.cpp:150` |
| &lt;road&gt; &lt;planView&gt; | builds `mod.rs:1028` | builds `RoadManager.cpp:3987` | builds `OpenDriveMap.cpp:232` | builds `GeometryParser.cpp:70` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; | builds `mod.rs:1030` | builds `RoadManager.cpp:3990` | builds `OpenDriveMap.cpp:232` | builds `GeometryParser.cpp:73` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; `@s` | builds `mod.rs:1034` | builds `RoadManager.cpp:3992` | builds `OpenDriveMap.cpp:234` | builds `GeometryParser.cpp:80` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; `@x` | builds `mod.rs:1035` | builds `RoadManager.cpp:3993` | builds `OpenDriveMap.cpp:235` | builds `GeometryParser.cpp:81` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; `@y` | builds `mod.rs:1036` | builds `RoadManager.cpp:3994` | builds `OpenDriveMap.cpp:236` | builds `GeometryParser.cpp:82` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; `@hdg` | builds `mod.rs:1037` | builds `RoadManager.cpp:3995` | builds `OpenDriveMap.cpp:237` | builds `GeometryParser.cpp:83` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; `@length` | builds `mod.rs:818` | builds `RoadManager.cpp:3996` | builds `OpenDriveMap.cpp:238` | builds `GeometryParser.cpp:84` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;line&gt; | builds `mod.rs:1080` | builds `RoadManager.cpp:4010` | builds `OpenDriveMap.cpp:245` | builds `GeometryParser.cpp:88` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;spiral&gt; | builds `mod.rs:1047` | builds `RoadManager.cpp:4021` | builds `OpenDriveMap.cpp:249` | builds `GeometryParser.cpp:91` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;spiral&gt; `@curvStart` | builds `mod.rs:1049` | builds `RoadManager.cpp:4021` | builds `OpenDriveMap.cpp:251` | builds `GeometryParser.cpp:92` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;spiral&gt; `@curvEnd` | builds `mod.rs:1049` | builds `RoadManager.cpp:4022` | builds `OpenDriveMap.cpp:252` | builds `GeometryParser.cpp:93` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;arc&gt; | builds `mod.rs:1042` | builds `RoadManager.cpp:4016` | builds `OpenDriveMap.cpp:276` | builds `GeometryParser.cpp:89` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;arc&gt; `@curvature` | builds `mod.rs:1043` | builds `RoadManager.cpp:4016` | builds `OpenDriveMap.cpp:278` | builds `GeometryParser.cpp:90` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;poly3&gt; | builds `mod.rs:1071` | builds `RoadManager.cpp:4027` | - | builds `GeometryParser.cpp:94` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;poly3&gt; `@a` | builds `mod.rs:1077` | builds `RoadManager.cpp:4027` | - | builds `GeometryParser.cpp:95` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;poly3&gt; `@b` | builds `mod.rs:1077` | builds `RoadManager.cpp:4028` | - | builds `GeometryParser.cpp:96` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;poly3&gt; `@c` | builds `mod.rs:1077` | builds `RoadManager.cpp:4029` | - | builds `GeometryParser.cpp:97` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;poly3&gt; `@d` | builds `mod.rs:1077` | builds `RoadManager.cpp:4030` | - | builds `GeometryParser.cpp:98` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; | builds `mod.rs:1057` | builds `RoadManager.cpp:4035` | builds `OpenDriveMap.cpp:281` | builds `GeometryParser.cpp:99` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@aU` | builds `mod.rs:1067` | builds `RoadManager.cpp:4035` | builds `OpenDriveMap.cpp:283` | builds `GeometryParser.cpp:100` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@bU` | builds `mod.rs:1067` | builds `RoadManager.cpp:4036` | builds `OpenDriveMap.cpp:284` | builds `GeometryParser.cpp:101` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@cU` | builds `mod.rs:1067` | builds `RoadManager.cpp:4037` | builds `OpenDriveMap.cpp:285` | builds `GeometryParser.cpp:102` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@dU` | builds `mod.rs:1067` | builds `RoadManager.cpp:4038` | builds `OpenDriveMap.cpp:286` | builds `GeometryParser.cpp:103` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@aV` | builds `mod.rs:1068` | builds `RoadManager.cpp:4039` | builds `OpenDriveMap.cpp:287` | builds `GeometryParser.cpp:104` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@bV` | builds `mod.rs:1068` | builds `RoadManager.cpp:4040` | builds `OpenDriveMap.cpp:288` | builds `GeometryParser.cpp:105` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@cV` | builds `mod.rs:1068` | builds `RoadManager.cpp:4041` | builds `OpenDriveMap.cpp:289` | builds `GeometryParser.cpp:106` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@dV` | builds `mod.rs:1068` | builds `RoadManager.cpp:4042` | builds `OpenDriveMap.cpp:290` | builds `GeometryParser.cpp:107` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@pRange` | builds `mod.rs:1062` | builds `RoadManager.cpp:4045` | builds `OpenDriveMap.cpp:293` | builds `GeometryParser.cpp:108` |
| &lt;road&gt; &lt;elevationProfile&gt; | builds `mod.rs:1093` | builds `RoadManager.cpp:4088` | builds `OpenDriveMap.cpp:313` | builds `ProfilesParser.cpp:56` |
| &lt;road&gt; &lt;elevationProfile&gt; &lt;elevation&gt; | builds `mod.rs:1094` | builds `RoadManager.cpp:4088` | builds `OpenDriveMap.cpp:325` | builds `ProfilesParser.cpp:60` |
| &lt;road&gt; &lt;elevationProfile&gt; &lt;elevation&gt; `@s` | builds `mod.rs:1094` | builds `RoadManager.cpp:4090` | builds `OpenDriveMap.cpp:328` | builds `ProfilesParser.cpp:68` |
| &lt;road&gt; &lt;elevationProfile&gt; &lt;elevation&gt; `@a` | builds `mod.rs:1947` | builds `RoadManager.cpp:4091` | builds `OpenDriveMap.cpp:329` | builds `ProfilesParser.cpp:69` |
| &lt;road&gt; &lt;elevationProfile&gt; &lt;elevation&gt; `@b` | builds `mod.rs:1948` | builds `RoadManager.cpp:4092` | builds `OpenDriveMap.cpp:330` | builds `ProfilesParser.cpp:70` |
| &lt;road&gt; &lt;elevationProfile&gt; &lt;elevation&gt; `@c` | builds `mod.rs:1949` | builds `RoadManager.cpp:4093` | builds `OpenDriveMap.cpp:331` | builds `ProfilesParser.cpp:71` |
| &lt;road&gt; &lt;elevationProfile&gt; &lt;elevation&gt; `@d` | builds `mod.rs:1950` | builds `RoadManager.cpp:4094` | builds `OpenDriveMap.cpp:332` | builds `ProfilesParser.cpp:72` |
| &lt;road&gt; &lt;lateralProfile&gt; | builds `mod.rs:1099` | builds `RoadManager.cpp:4108` | builds `OpenDriveMap.cpp:317` | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;superelevation&gt; | builds `mod.rs:1100` | builds `RoadManager.cpp:4111` | builds `OpenDriveMap.cpp:317` | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;superelevation&gt; `@s` | builds `mod.rs:1100` | builds `RoadManager.cpp:4114` | builds `OpenDriveMap.cpp:328` | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;superelevation&gt; `@a` | builds `mod.rs:1947` | builds `RoadManager.cpp:4115` | builds `OpenDriveMap.cpp:329` | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;superelevation&gt; `@b` | builds `mod.rs:1948` | builds `RoadManager.cpp:4116` | builds `OpenDriveMap.cpp:330` | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;superelevation&gt; `@c` | builds `mod.rs:1949` | builds `RoadManager.cpp:4117` | builds `OpenDriveMap.cpp:331` | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;superelevation&gt; `@d` | builds `mod.rs:1950` | builds `RoadManager.cpp:4118` | builds `OpenDriveMap.cpp:332` | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;shape&gt; | builds `mod.rs:688` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;shape&gt; `@s` | builds `mod.rs:692` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;shape&gt; `@t` | builds `mod.rs:694` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;shape&gt; `@a` | builds `mod.rs:695` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;shape&gt; `@b` | builds `mod.rs:696` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;shape&gt; `@c` | builds `mod.rs:697` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;shape&gt; `@d` | builds `mod.rs:698` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; | builds `mod.rs:1108` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;tOffset&gt; | builds `mod.rs:731` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;tOffset&gt; &lt;coefficients&gt; | builds `mod.rs:728` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;tOffset&gt; &lt;coefficients&gt; `@a` | builds `mod.rs:695` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;tOffset&gt; &lt;coefficients&gt; `@b` | builds `mod.rs:696` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;tOffset&gt; &lt;coefficients&gt; `@c` | builds `mod.rs:697` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;tOffset&gt; &lt;coefficients&gt; `@d` | builds `mod.rs:698` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;tOffset&gt; &lt;coefficients&gt; `@s` | builds `mod.rs:728` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; | builds `mod.rs:734` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; | builds `mod.rs:737` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; `@id` | builds `mod.rs:739` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; `@mode` | builds `mod.rs:747` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;width&gt; | builds `mod.rs:760` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;width&gt; &lt;coefficients&gt; | builds `mod.rs:728` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;constant&gt; | builds `mod.rs:762` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;constant&gt; &lt;coefficients&gt; | builds `mod.rs:728` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;linear&gt; | builds `mod.rs:763` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;linear&gt; &lt;coefficients&gt; | builds `mod.rs:728` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;quadratic&gt; | builds `mod.rs:764` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;quadratic&gt; &lt;coefficients&gt; | builds `mod.rs:728` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;cubic&gt; | builds `mod.rs:765` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;cubic&gt; &lt;coefficients&gt; | builds `mod.rs:728` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; | builds `mod.rs:1119` | builds `RoadManager.cpp:4132` | builds `OpenDriveMap.cpp:374` | builds `LaneParser.cpp:197` |
| &lt;road&gt; &lt;lanes&gt; `@layer` | - | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneOffset&gt; | builds `mod.rs:1120` | builds `RoadManager.cpp:4137` | builds `OpenDriveMap.cpp:314` | builds `RoadParser.cpp:158` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneOffset&gt; `@s` | builds `mod.rs:1120` | builds `RoadManager.cpp:4139` | builds `OpenDriveMap.cpp:328` | builds `RoadParser.cpp:160` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneOffset&gt; `@a` | builds `mod.rs:1947` | builds `RoadManager.cpp:4140` | builds `OpenDriveMap.cpp:329` | builds `RoadParser.cpp:161` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneOffset&gt; `@b` | builds `mod.rs:1948` | builds `RoadManager.cpp:4141` | builds `OpenDriveMap.cpp:330` | builds `RoadParser.cpp:162` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneOffset&gt; `@c` | builds `mod.rs:1949` | builds `RoadManager.cpp:4142` | builds `OpenDriveMap.cpp:331` | builds `RoadParser.cpp:163` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneOffset&gt; `@d` | builds `mod.rs:1950` | builds `RoadManager.cpp:4143` | builds `OpenDriveMap.cpp:332` | builds `RoadParser.cpp:164` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; | builds `mod.rs:1419` | builds `RoadManager.cpp:4148` | builds `OpenDriveMap.cpp:374` | builds `LaneParser.cpp:199` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; `@length` | - | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; `@s` | builds `mod.rs:1425` | builds `RoadManager.cpp:4148` | builds `OpenDriveMap.cpp:376` | builds `LaneParser.cpp:200` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; `@singleSide` | - | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; | builds `mod.rs:822` | builds `RoadManager.cpp:4163` | builds `OpenDriveMap.cpp:380` | builds `LaneParser.cpp:201` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; | builds `mod.rs:826` | builds `RoadManager.cpp:4179` | builds `OpenDriveMap.cpp:380` | builds `LaneParser.cpp:22` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; `@type` | builds `mod.rs:847` | builds `RoadManager.cpp:4190` | stores `OpenDriveMap.cpp:388` | builds `RoadParser.cpp:184` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; `@advisory` | - | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; `@direction` | builds `mod.rs:658` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; `@dynamicLaneType` | - | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; `@dynamicLaneDirection` | - | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; `@level` | builds `mod.rs:862` | - | builds `OpenDriveMap.cpp:388` | stores `RoadParser.cpp:185` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; `@roadWorks` | - | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; `@id` | builds `mod.rs:827` | builds `RoadManager.cpp:4288` | builds `OpenDriveMap.cpp:383` | builds `RoadParser.cpp:183` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;link&gt; | builds `links.rs:223` | builds `RoadManager.cpp:4305` | builds `OpenDriveMap.cpp:391` | builds `RoadParser.cpp:188` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;link&gt; &lt;predecessor\|successor&gt; | builds `links.rs:223` | builds `RoadManager.cpp:4305` | builds `OpenDriveMap.cpp:391` | builds `RoadParser.cpp:188` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;link&gt; &lt;predecessor\|successor&gt; `@id` | builds `links.rs:230` | builds `RoadManager.cpp:4311` | builds `OpenDriveMap.cpp:392` | builds `RoadParser.cpp:191` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;link&gt; &lt;predecessor\|successor&gt; `@layer` | - | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;border&gt; | builds `mod.rs:831` | - | - | stores `LaneParser.cpp:48` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;border&gt; `@sOffset` | builds `mod.rs:1965` | - | - | stores `LaneParser.cpp:49` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;border&gt; `@a` | builds `mod.rs:1966` | - | - | stores `LaneParser.cpp:50` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;border&gt; `@b` | builds `mod.rs:1967` | - | - | stores `LaneParser.cpp:51` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;border&gt; `@c` | builds `mod.rs:1968` | - | - | stores `LaneParser.cpp:52` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;border&gt; `@d` | builds `mod.rs:1969` | - | - | stores `LaneParser.cpp:53` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;width&gt; | builds `mod.rs:830` | builds `RoadManager.cpp:4321` | builds `OpenDriveMap.cpp:397` | builds `LaneParser.cpp:30` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;width&gt; `@sOffset` | builds `mod.rs:1965` | builds `RoadManager.cpp:4323` | builds `OpenDriveMap.cpp:399` | builds `LaneParser.cpp:31` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;width&gt; `@a` | builds `mod.rs:1966` | builds `RoadManager.cpp:4324` | builds `OpenDriveMap.cpp:400` | builds `LaneParser.cpp:32` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;width&gt; `@b` | builds `mod.rs:1967` | builds `RoadManager.cpp:4325` | builds `OpenDriveMap.cpp:401` | builds `LaneParser.cpp:33` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;width&gt; `@c` | builds `mod.rs:1968` | builds `RoadManager.cpp:4326` | builds `OpenDriveMap.cpp:402` | builds `LaneParser.cpp:34` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;width&gt; `@d` | builds `mod.rs:1969` | builds `RoadManager.cpp:4327` | builds `OpenDriveMap.cpp:403` | builds `LaneParser.cpp:35` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; | builds `road_marks.rs:128` | builds `RoadManager.cpp:4341` | builds `OpenDriveMap.cpp:422` | builds `LaneParser.cpp:61` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@sOffset` | builds `road_marks.rs:139` | builds `RoadManager.cpp:4344` | builds `OpenDriveMap.cpp:429` | stores `LaneParser.cpp:64` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@type` | builds `road_marks.rs:140` | builds `RoadManager.cpp:4360` | stores `OpenDriveMap.cpp:430` | stores `LaneParser.cpp:65` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@weight` | builds `road_marks.rs:141` | stores `RoadManager.cpp:4415` | builds `OpenDriveMap.cpp:431` | stores `LaneParser.cpp:66` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@color` | stores `road_marks.rs:145` | stores `RoadManager.cpp:1543` | stores `OpenDriveMap.cpp:432` | stores `LaneParser.cpp:67` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@material` | - | - | stores `OpenDriveMap.cpp:433` | stores `LaneParser.cpp:68` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@width` | builds `road_marks.rs:131` | builds `RoadManager.cpp:4483` | builds `OpenDriveMap.cpp:427` | stores `LaneParser.cpp:69` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@laneChange` | stores `road_marks.rs:148` | stores `RoadManager.cpp:4449` | stores `OpenDriveMap.cpp:434` | builds `LaneParser.cpp:70` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@height` | builds `road_marks.rs:147` | stores `RoadManager.cpp:4499` | stores `OpenDriveMap.cpp:428` | stores `LaneParser.cpp:71` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;sway&gt; | builds `road_marks.rs:154` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;sway&gt; `@ds` | builds `road_marks.rs:154` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;sway&gt; `@a` | builds `mod.rs:1947` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;sway&gt; `@b` | builds `mod.rs:1948` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;sway&gt; `@c` | builds `mod.rs:1949` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;sway&gt; `@d` | builds `mod.rs:1950` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; | builds `road_marks.rs:130` | builds `RoadManager.cpp:4511` | builds `OpenDriveMap.cpp:440` | stores `LaneParser.cpp:77` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; `@name` | - | stores `RoadManager.cpp:4515` | stores `OpenDriveMap.cpp:442` | stores `LaneParser.cpp:79` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; `@width` | builds `road_marks.rs:131` | stores `RoadManager.cpp:4516` | builds `OpenDriveMap.cpp:443` | stores `LaneParser.cpp:80` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; &lt;line&gt; | builds `road_marks.rs:136` | builds `RoadManager.cpp:4519` | builds `OpenDriveMap.cpp:445` | stores `LaneParser.cpp:99` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; &lt;line&gt; `@length` | builds `road_marks.rs:76` | builds `RoadManager.cpp:4521` | builds `OpenDriveMap.cpp:455` | stores `LaneParser.cpp:101` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; &lt;line&gt; `@space` | builds `road_marks.rs:60` | builds `RoadManager.cpp:4522` | builds `OpenDriveMap.cpp:456` | stores `LaneParser.cpp:102` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; &lt;line&gt; `@tOffset` | builds `road_marks.rs:86` | builds `RoadManager.cpp:4523` | builds `OpenDriveMap.cpp:457` | stores `LaneParser.cpp:103` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; &lt;line&gt; `@sOffset` | builds `road_marks.rs:85` | builds `RoadManager.cpp:4524` | builds `OpenDriveMap.cpp:458` | stores `LaneParser.cpp:104` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; &lt;line&gt; `@rule` | stores `road_marks.rs:87` | stores `RoadManager.cpp:4553` | stores `OpenDriveMap.cpp:460` | stores `LaneParser.cpp:105` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; &lt;line&gt; `@width` | builds `road_marks.rs:93` | builds `RoadManager.cpp:4575` | builds `OpenDriveMap.cpp:447` | stores `LaneParser.cpp:106` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; &lt;line&gt; `@color` | stores `road_marks.rs:94` | stores `RoadManager.cpp:4542` | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;explicit&gt; | builds `road_marks.rs:152` | builds `RoadManager.cpp:4601` | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;explicit&gt; &lt;line&gt; | builds `road_marks.rs:136` | builds `RoadManager.cpp:4601` | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;explicit&gt; &lt;line&gt; `@length` | builds `road_marks.rs:76` | builds `RoadManager.cpp:4603` | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;explicit&gt; &lt;line&gt; `@tOffset` | builds `road_marks.rs:86` | builds `RoadManager.cpp:4604` | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;explicit&gt; &lt;line&gt; `@sOffset` | builds `road_marks.rs:85` | builds `RoadManager.cpp:4605` | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;explicit&gt; &lt;line&gt; `@rule` | stores `road_marks.rs:87` | stores `RoadManager.cpp:4610` | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;explicit&gt; &lt;line&gt; `@width` | builds `road_marks.rs:93` | builds `RoadManager.cpp:4606` | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;material&gt; | stores `properties.rs:175` | stores `RoadManager.cpp:4744` | - | stores `LaneParser.cpp:123` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;material&gt; `@sOffset` | stores `properties.rs:162` | stores `RoadManager.cpp:4747` | - | stores `LaneParser.cpp:125` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;material&gt; `@surface` | stores `mod.rs:753` | - | - | stores `LaneParser.cpp:126` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;material&gt; `@friction` | stores `mod.rs:754` | stores `RoadManager.cpp:4750` | - | stores `LaneParser.cpp:127` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;material&gt; `@roughness` | stores `mod.rs:755` | - | - | stores `LaneParser.cpp:128` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;speed&gt; | builds `properties.rs:100` | - | - | stores `LaneParser.cpp:147` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;speed&gt; `@sOffset` | builds `properties.rs:102` | - | - | stores `LaneParser.cpp:148` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;speed&gt; `@max` | builds `properties.rs:37` | - | - | stores `LaneParser.cpp:149` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;speed&gt; `@unit` | builds `properties.rs:42` | - | - | stores `LaneParser.cpp:150` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;access&gt; | stores `properties.rs:182` | - | - | stores `LaneParser.cpp:157` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;access&gt; `@sOffset` | stores `properties.rs:162` | - | - | stores `LaneParser.cpp:158` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;access&gt; `@rule` | stores `properties.rs:133` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;access&gt; `@restriction` | stores `properties.rs:123` | - | - | stores `LaneParser.cpp:159` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;access&gt; &lt;restriction&gt; | stores `properties.rs:127` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;access&gt; &lt;restriction&gt; `@type` | stores `properties.rs:128` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;height&gt; | builds `mod.rs:782` | stores `RoadManager.cpp:4332` | builds `OpenDriveMap.cpp:411` | stores `LaneParser.cpp:166` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;height&gt; `@sOffset` | builds `mod.rs:784` | stores `RoadManager.cpp:4334` | builds `OpenDriveMap.cpp:413` | stores `LaneParser.cpp:167` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;height&gt; `@inner` | builds `mod.rs:785` | stores `RoadManager.cpp:4335` | builds `OpenDriveMap.cpp:414` | stores `LaneParser.cpp:168` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;height&gt; `@outer` | builds `mod.rs:786` | stores `RoadManager.cpp:4336` | builds `OpenDriveMap.cpp:415` | stores `LaneParser.cpp:169` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;rule&gt; | stores `properties.rs:170` | - | - | stores `LaneParser.cpp:176` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;rule&gt; `@sOffset` | stores `properties.rs:162` | - | - | stores `LaneParser.cpp:177` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;rule&gt; `@value` | stores `properties.rs:170` | - | - | stores `LaneParser.cpp:178` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; | builds `mod.rs:881` | builds `RoadManager.cpp:4163` | builds `OpenDriveMap.cpp:380` | builds `LaneParser.cpp:206` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; &lt;lane&gt; | builds `mod.rs:884` | builds `RoadManager.cpp:4179` | builds `OpenDriveMap.cpp:380` | builds `RoadParser.cpp:203` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; &lt;lane&gt; `@id` | - | builds `RoadManager.cpp:4288` | builds `OpenDriveMap.cpp:383` | builds `RoadParser.cpp:206` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; &lt;lane&gt; `@type` | - | stores `RoadManager.cpp:4190` | stores `OpenDriveMap.cpp:388` | stores `RoadParser.cpp:207` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; &lt;lane&gt; `@level` | - | - | builds `OpenDriveMap.cpp:388` | stores `RoadParser.cpp:208` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; &lt;lane&gt; &lt;link&gt; | - | builds `RoadManager.cpp:4305` | builds `OpenDriveMap.cpp:391` | stores `RoadParser.cpp:211` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; &lt;lane&gt; &lt;roadMark&gt; | builds `road_marks.rs:128` | builds `RoadManager.cpp:4341` | builds `OpenDriveMap.cpp:422` | stores `LaneParser.cpp:61` |
| &lt;road&gt; &lt;objects&gt; | builds `mod.rs:1140` | builds `RoadManager.cpp:5054` | builds `OpenDriveMap.cpp:523` | builds `ObjectParser.cpp:28` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; | builds `mod.rs:2127` | builds `RoadManager.cpp:5057` | builds `OpenDriveMap.cpp:523` | builds `ObjectParser.cpp:31` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@t` | builds `mod.rs:2166` | builds `RoadManager.cpp:5063` | builds `OpenDriveMap.cpp:536` | builds `ObjectParser.cpp:56` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@zOffset` | builds `mod.rs:2167` | builds `RoadManager.cpp:5111` | builds `OpenDriveMap.cpp:537` | builds `ObjectParser.cpp:57` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@type` | stores `mod.rs:2338` | stores `RoadManager.cpp:5088` | stores `OpenDriveMap.cpp:546` | builds `ObjectParser.cpp:34` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@validLength` | stores `mod.rs:2169` | - | stores `OpenDriveMap.cpp:539` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@orientation` | builds `mod.rs:2211` | builds `RoadManager.cpp:5069` | stores `OpenDriveMap.cpp:548` | builds `ObjectParser.cpp:61` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@subtype` | stores `mod.rs:2368` | stores `RoadManager.cpp:5090` | stores `OpenDriveMap.cpp:549` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@dynamic` | stores `mod.rs:2370` | - | stores `OpenDriveMap.cpp:530` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@hdg` | builds `mod.rs:2280` | builds `RoadManager.cpp:5113` | builds `OpenDriveMap.cpp:543` | builds `ObjectParser.cpp:58` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@name` | stores `mod.rs:2102` | stores `RoadManager.cpp:5065` | stores `OpenDriveMap.cpp:547` | builds `ObjectParser.cpp:35` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@pitch` | builds `mod.rs:2281` | builds `RoadManager.cpp:5114` | builds `OpenDriveMap.cpp:544` | builds `ObjectParser.cpp:59` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@id` | builds `mod.rs:2128` | stores `RoadManager.cpp:5064` | stores `OpenDriveMap.cpp:525` | builds `ObjectParser.cpp:78` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@roll` | builds `mod.rs:2282` | builds `RoadManager.cpp:5115` | builds `OpenDriveMap.cpp:545` | builds `ObjectParser.cpp:60` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@height` | builds `mod.rs:2277` | builds `RoadManager.cpp:5112` | builds `OpenDriveMap.cpp:542` | builds `ObjectParser.cpp:90` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@s` | builds `mod.rs:2165` | builds `RoadManager.cpp:5062` | builds `OpenDriveMap.cpp:535` | builds `ObjectParser.cpp:55` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@length` | builds `mod.rs:2023` | builds `RoadManager.cpp:5091` | builds `OpenDriveMap.cpp:538` | builds `ObjectParser.cpp:63` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@width` | builds `mod.rs:2276` | builds `RoadManager.cpp:5092` | builds `OpenDriveMap.cpp:540` | builds `ObjectParser.cpp:62` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@radius` | builds `mod.rs:2278` | builds `RoadManager.cpp:5093` | builds `OpenDriveMap.cpp:541` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@perpToRoad` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@invalidated` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@temporary` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; | builds `mod.rs:2308` | builds `RoadManager.cpp:5120` | builds `OpenDriveMap.cpp:560` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@s` | builds `mod.rs:2240` | builds `RoadManager.cpp:5123` | builds `OpenDriveMap.cpp:562` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@length` | builds `mod.rs:2241` | builds `RoadManager.cpp:5129` | builds `OpenDriveMap.cpp:563` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@distance` | builds `mod.rs:2494` | builds `RoadManager.cpp:5130` | builds `OpenDriveMap.cpp:564` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@tStart` | builds `mod.rs:2505` | builds `RoadManager.cpp:5131` | builds `OpenDriveMap.cpp:565` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@tEnd` | builds `mod.rs:2505` | builds `RoadManager.cpp:5132` | builds `OpenDriveMap.cpp:566` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@heightStart` | builds `mod.rs:2505` | builds `RoadManager.cpp:5133` | builds `OpenDriveMap.cpp:569` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@heightEnd` | builds `mod.rs:2505` | builds `RoadManager.cpp:5134` | builds `OpenDriveMap.cpp:570` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@zOffsetStart` | builds `mod.rs:2505` | builds `RoadManager.cpp:5135` | builds `OpenDriveMap.cpp:571` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@zOffsetEnd` | builds `mod.rs:2505` | builds `RoadManager.cpp:5136` | builds `OpenDriveMap.cpp:572` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@widthStart` | builds `mod.rs:2505` | builds `RoadManager.cpp:5138` | builds `OpenDriveMap.cpp:567` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@widthEnd` | builds `mod.rs:2505` | builds `RoadManager.cpp:5139` | builds `OpenDriveMap.cpp:568` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@lengthStart` | builds `mod.rs:2505` | builds `RoadManager.cpp:5140` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@lengthEnd` | builds `mod.rs:2505` | builds `RoadManager.cpp:5141` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@radiusStart` | builds `mod.rs:2505` | builds `RoadManager.cpp:5142` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@radiusEnd` | builds `mod.rs:2505` | builds `RoadManager.cpp:5143` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@detachFromReferenceLine` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@bT` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@cT` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@dT` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; | builds `mod.rs:2623` | - | builds `OpenDriveMap.cpp:591` | builds `ObjectParser.cpp:39` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; `@id` | builds `mod.rs:2592` | - | stores `OpenDriveMap.cpp:593` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; `@fillType` | - | - | stores `OpenDriveMap.cpp:594` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; `@outer` | builds `mod.rs:2411` | - | stores `OpenDriveMap.cpp:596` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; `@closed` | builds `mod.rs:2710` | - | stores `OpenDriveMap.cpp:597` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; `@laneType` | - | - | stores `OpenDriveMap.cpp:595` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerRoad&gt; | builds `mod.rs:2673` | - | builds `OpenDriveMap.cpp:614` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerRoad&gt; `@id` | builds `mod.rs:2758` | - | stores `OpenDriveMap.cpp:620` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerRoad&gt; `@s` | builds `mod.rs:2674` | - | builds `OpenDriveMap.cpp:616` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerRoad&gt; `@t` | builds `mod.rs:2674` | - | builds `OpenDriveMap.cpp:617` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerRoad&gt; `@dz` | builds `mod.rs:2680` | - | builds `OpenDriveMap.cpp:618` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerRoad&gt; `@height` | builds `mod.rs:2672` | - | builds `OpenDriveMap.cpp:622` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerLocal&gt; | builds `mod.rs:2737` | - | builds `OpenDriveMap.cpp:600` | builds `ObjectParser.cpp:42` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerLocal&gt; `@id` | builds `mod.rs:2758` | - | stores `OpenDriveMap.cpp:606` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerLocal&gt; `@u` | builds `mod.rs:2689` | - | builds `OpenDriveMap.cpp:602` | builds `ObjectParser.cpp:43` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerLocal&gt; `@v` | builds `mod.rs:2689` | - | builds `OpenDriveMap.cpp:603` | builds `ObjectParser.cpp:44` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerLocal&gt; `@z` | builds `mod.rs:2690` | - | builds `OpenDriveMap.cpp:604` | builds `ObjectParser.cpp:45` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerLocal&gt; `@height` | builds `mod.rs:2672` | - | builds `OpenDriveMap.cpp:608` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; `@id` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; `@u` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; `@v` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; `@z` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; `@height` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; `@length` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; `@hdg` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;arc&gt; | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;arc&gt; `@curvature` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;line&gt; | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@aU` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@bU` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@cU` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@dU` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@aV` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@bV` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@cV` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@dV` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@pRange` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; | builds `mod.rs:2744` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; | builds `mod.rs:2747` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@side` | builds `mod.rs:2781` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@weight` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@width` | builds `mod.rs:2799` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@color` | builds `mod.rs:2807` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@zOffset` | builds `mod.rs:2800` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@spaceLength` | builds `mod.rs:2803` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@lineLength` | builds `mod.rs:2802` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@startOffset` | builds `mod.rs:2813` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@stopOffset` | builds `mod.rs:2814` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; &lt;cornerReference&gt; | builds `mod.rs:2779` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; &lt;cornerReference&gt; `@id` | builds `mod.rs:2627` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outlines&gt; | builds `mod.rs:2620` | builds `RoadManager.cpp:5248` | builds `OpenDriveMap.cpp:590` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outlines&gt; &lt;outline&gt; | builds `mod.rs:2623` | builds `RoadManager.cpp:5251` | builds `OpenDriveMap.cpp:591` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;material&gt; | stores `mod.rs:2349` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;material&gt; `@surface` | stores `mod.rs:753` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;material&gt; `@friction` | stores `mod.rs:754` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;material&gt; `@roughness` | stores `mod.rs:755` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;material&gt; `@roadMarkColor` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;validity&gt; | builds `mod.rs:2203` | stores `RoadManager.cpp:5389` | stores `OpenDriveMap.cpp:38` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;validity&gt; `@fromLane` | builds `mod.rs:2204` | stores `RoadManager.cpp:5392` | stores `OpenDriveMap.cpp:40` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;validity&gt; `@toLane` | builds `mod.rs:2204` | stores `RoadManager.cpp:5393` | stores `OpenDriveMap.cpp:40` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;validity&gt; `@layer` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;parkingSpace&gt; | stores `mod.rs:2340` | stores `RoadManager.cpp:5290` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;parkingSpace&gt; `@access` | stores `mod.rs:2343` | stores `RoadManager.cpp:5295` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;parkingSpace&gt; `@restrictions` | stores `mod.rs:2344` | stores `RoadManager.cpp:5334` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;markings&gt; | builds `mod.rs:2744` | builds `RoadManager.cpp:5339` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;borders&gt; | builds `mod.rs:2836` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;borders&gt; &lt;border&gt; | builds `mod.rs:2841` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;borders&gt; &lt;border&gt; `@width` | builds `mod.rs:2847` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;borders&gt; &lt;border&gt; `@type` | builds `mod.rs:2606` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;borders&gt; &lt;border&gt; `@outlineId` | builds `mod.rs:2843` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;borders&gt; &lt;border&gt; `@useCompleteOutline` | builds `mod.rs:2848` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;borders&gt; &lt;border&gt; &lt;cornerReference&gt; | builds `mod.rs:2877` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;surface&gt; | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;surface&gt; &lt;CRG&gt; | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;surface&gt; &lt;CRG&gt; `@file` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;surface&gt; &lt;CRG&gt; `@hideRoadSurfaceCRG` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;surface&gt; &lt;CRG&gt; `@zScale` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; `@id` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexRoad&gt; | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexRoad&gt; `@dz` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexRoad&gt; `@radius` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexRoad&gt; `@intersectionPoint` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexRoad&gt; `@id` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexRoad&gt; `@s` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexRoad&gt; `@t` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexLocal&gt; | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexLocal&gt; `@radius` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexLocal&gt; `@intersectionPoint` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexLocal&gt; `@id` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexLocal&gt; `@u` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexLocal&gt; `@v` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexLocal&gt; `@z` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;objectReference&gt; | builds `mod.rs:2232` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;objectReference&gt; `@s` | builds `mod.rs:2191` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;objectReference&gt; `@t` | builds `mod.rs:2191` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;objectReference&gt; `@id` | builds `mod.rs:2233` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;objectReference&gt; `@zOffset` | builds `mod.rs:2188` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;objectReference&gt; `@validLength` | stores `mod.rs:2190` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;objectReference&gt; `@orientation` | builds `mod.rs:2211` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;objectReference&gt; &lt;validity&gt; | builds `mod.rs:2203` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; | builds `mod.rs:2061` | builds `RoadManager.cpp:5439` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; `@s` | builds `mod.rs:2072` | builds `RoadManager.cpp:5448` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; `@length` | builds `mod.rs:2072` | builds `RoadManager.cpp:5445` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; `@name` | stores `mod.rs:2102` | stores `RoadManager.cpp:5447` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; `@id` | stores `mod.rs:2108` | stores `RoadManager.cpp:5444` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; `@type` | stores `mod.rs:2063` | stores `RoadManager.cpp:5449` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; `@lighting` | stores `mod.rs:2064` | stores `RoadManager.cpp:5446` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; `@daylight` | stores `mod.rs:2065` | stores `RoadManager.cpp:5443` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; &lt;validity&gt; | builds `mod.rs:2203` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;bridge&gt; | builds `mod.rs:2067` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;bridge&gt; `@s` | builds `mod.rs:2072` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;bridge&gt; `@length` | builds `mod.rs:2072` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;bridge&gt; `@name` | stores `mod.rs:2102` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;bridge&gt; `@id` | stores `mod.rs:2108` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;bridge&gt; `@type` | stores `mod.rs:2068` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;bridge&gt; &lt;validity&gt; | builds `mod.rs:2203` | - | - | - |
| &lt;road&gt; &lt;signals&gt; | builds `signals.rs:50` | builds `RoadManager.cpp:4839` | builds `OpenDriveMap.cpp:637` | builds `SignalParser.cpp:44` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; | builds `signals.rs:58` | builds `RoadManager.cpp:4845` | builds `OpenDriveMap.cpp:637` | builds `SignalParser.cpp:46` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@id` | builds `signals.rs:235` | builds `RoadManager.cpp:4851` | stores `OpenDriveMap.cpp:639` | builds `SignalParser.cpp:49` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@name` | stores `signals.rs:112` | stores `RoadManager.cpp:4852` | stores `OpenDriveMap.cpp:648` | builds `SignalParser.cpp:50` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@dynamic` | stores `signals.rs:209` | builds `RoadManager.cpp:4856` | stores `OpenDriveMap.cpp:651` | stores `SignalParser.cpp:51` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@orientation` | builds `signals.rs:190` | builds `RoadManager.cpp:4875` | stores `OpenDriveMap.cpp:659` | builds `SignalParser.cpp:52` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@country` | stores `signals.rs:210` | builds `RoadManager.cpp:4897` | stores `OpenDriveMap.cpp:660` | stores `SignalParser.cpp:54` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@countryRevision` | stores `signals.rs:211` | builds `RoadManager.cpp:4901` | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@type` | stores `signals.rs:212` | builds `RoadManager.cpp:4917` | stores `OpenDriveMap.cpp:661` | builds `SignalParser.cpp:55` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@subtype` | stores `signals.rs:213` | builds `RoadManager.cpp:4918` | stores `OpenDriveMap.cpp:662` | stores `SignalParser.cpp:56` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@value` | stores `signals.rs:214` | builds `RoadManager.cpp:4919` | stores `OpenDriveMap.cpp:653` | stores `SignalParser.cpp:57` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@text` | stores `signals.rs:216` | stores `RoadManager.cpp:4960` | stores `OpenDriveMap.cpp:664` | stores `SignalParser.cpp:61` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@unit` | stores `signals.rs:215` | stores `RoadManager.cpp:4956` | stores `OpenDriveMap.cpp:663` | stores `SignalParser.cpp:58` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@height` | builds `signals.rs:230` | builds `RoadManager.cpp:4957` | builds `OpenDriveMap.cpp:654` | builds `SignalParser.cpp:59` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@length` | builds `signals.rs:228` | builds `RoadManager.cpp:4959` | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@hOffset` | builds `signals.rs:304` | builds `RoadManager.cpp:4961` | builds `OpenDriveMap.cpp:656` | builds `SignalParser.cpp:62` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@width` | builds `signals.rs:229` | builds `RoadManager.cpp:4958` | builds `OpenDriveMap.cpp:655` | builds `SignalParser.cpp:60` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@roll` | builds `signals.rs:306` | builds `RoadManager.cpp:4963` | builds `OpenDriveMap.cpp:658` | builds `SignalParser.cpp:64` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@pitch` | builds `signals.rs:305` | builds `RoadManager.cpp:4962` | builds `OpenDriveMap.cpp:657` | builds `SignalParser.cpp:63` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@invalidated` | stores `signals.rs:217` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@temporary` | stores `signals.rs:218` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@s` | builds `signals.rs:187` | builds `RoadManager.cpp:4849` | builds `OpenDriveMap.cpp:649` | builds `SignalParser.cpp:47` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@t` | builds `signals.rs:187` | builds `RoadManager.cpp:4850` | builds `OpenDriveMap.cpp:650` | builds `SignalParser.cpp:48` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@zOffset` | builds `signals.rs:303` | builds `RoadManager.cpp:4896` | builds `OpenDriveMap.cpp:652` | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;validity&gt; | builds `signals.rs:222` | stores `RoadManager.cpp:5028` | stores `OpenDriveMap.cpp:38` | builds `SignalParser.cpp:25` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;dependency&gt; | builds `signals.rs:255` | - | - | stores `SignalParser.cpp:109` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;dependency&gt; `@id` | builds `signals.rs:256` | - | - | stores `SignalParser.cpp:110` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;dependency&gt; `@type` | stores `signals.rs:259` | - | - | stores `SignalParser.cpp:111` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;reference&gt; | builds `signals.rs:263` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;reference&gt; `@elementType` | stores `signals.rs:265` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;reference&gt; `@elementId` | builds `signals.rs:264` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;reference&gt; `@type` | stores `signals.rs:275` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionInertial&gt; | builds `signals.rs:198` | - | - | stores `SignalParser.cpp:115` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionInertial&gt; `@x` | builds `signals.rs:315` | - | - | stores `SignalParser.cpp:116` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionInertial&gt; `@y` | builds `signals.rs:316` | - | - | stores `SignalParser.cpp:117` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionInertial&gt; `@z` | builds `signals.rs:317` | - | - | stores `SignalParser.cpp:118` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionInertial&gt; `@hdg` | builds `signals.rs:321` | - | - | stores `SignalParser.cpp:119` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionInertial&gt; `@pitch` | builds `signals.rs:305` | - | - | stores `SignalParser.cpp:120` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionInertial&gt; `@roll` | builds `signals.rs:306` | - | - | stores `SignalParser.cpp:121` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionRoad&gt; | builds `signals.rs:195` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionRoad&gt; `@roadId` | builds `signals.rs:196` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionRoad&gt; `@s` | builds `signals.rs:292` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionRoad&gt; `@t` | builds `signals.rs:292` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionRoad&gt; `@zOffset` | builds `signals.rs:303` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionRoad&gt; `@hOffset` | builds `signals.rs:304` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionRoad&gt; `@pitch` | builds `signals.rs:305` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionRoad&gt; `@roll` | builds `signals.rs:306` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;semantics&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;validity&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;dependency&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;reference&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@id` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@name` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@dynamic` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@orientation` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@country` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@countryRevision` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@type` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@subtype` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@value` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@text` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@unit` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@height` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@length` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@hOffset` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@width` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@roll` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@pitch` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@invalidated` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@temporary` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@v` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@z` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; &lt;validity&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; &lt;dependency&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; &lt;reference&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; &lt;positionInertial&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; &lt;positionRoad&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; &lt;semantics&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; `@displayType` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; `@displayHeight` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; `@displayWidth` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; `@v` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; `@z` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;validity&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;dependency&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;reference&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;displayArea&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;displayArea&gt; `@index` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;displayArea&gt; `@width` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;displayArea&gt; `@height` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;displayArea&gt; `@v` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;displayArea&gt; `@z` | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signalReference&gt; | builds `signals.rs:77` | - | - | builds `SignalParser.cpp:128` |
| &lt;road&gt; &lt;signals&gt; &lt;signalReference&gt; `@s` | builds `signals.rs:149` | - | - | builds `SignalParser.cpp:129` |
| &lt;road&gt; &lt;signals&gt; &lt;signalReference&gt; `@t` | builds `signals.rs:149` | - | - | builds `SignalParser.cpp:130` |
| &lt;road&gt; &lt;signals&gt; &lt;signalReference&gt; `@id` | builds `signals.rs:78` | - | - | builds `SignalParser.cpp:131` |
| &lt;road&gt; &lt;signals&gt; &lt;signalReference&gt; `@orientation` | builds `signals.rs:155` | - | - | builds `SignalParser.cpp:133` |
| &lt;road&gt; &lt;signals&gt; &lt;signalReference&gt; &lt;validity&gt; | builds `signals.rs:156` | - | - | builds `SignalParser.cpp:147` |
| &lt;road&gt; &lt;surface&gt; | builds `mod.rs:912` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; | builds `mod.rs:602` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@file` | builds `mod.rs:1375` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@sStart` | builds `mod.rs:1294` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@sEnd` | builds `mod.rs:1295` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@orientation` | builds `mod.rs:1303` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@mode` | builds `mod.rs:747` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@purpose` | builds `mod.rs:1363` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@sOffset` | builds `mod.rs:1058` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@tOffset` | builds `mod.rs:731` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@xOffset` | builds `mod.rs:1348` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@yOffset` | builds `mod.rs:1349` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@zOffset` | builds `mod.rs:1369` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@zScale` | builds `mod.rs:1370` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@hOffset` | builds `mod.rs:1316` | - | - | - |
| &lt;road&gt; &lt;railroad&gt; | - | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; | - | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; `@name` | - | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; `@id` | - | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; `@position` | - | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; &lt;mainTrack&gt; | - | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; &lt;mainTrack&gt; `@id` | - | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; &lt;mainTrack&gt; `@s` | - | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; &lt;mainTrack&gt; `@dir` | - | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; &lt;sideTrack&gt; | - | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; &lt;sideTrack&gt; `@id` | - | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; &lt;sideTrack&gt; `@s` | - | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; &lt;sideTrack&gt; `@dir` | - | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; &lt;partner&gt; | - | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; &lt;partner&gt; `@name` | - | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; &lt;partner&gt; `@id` | - | - | - | - |

### &lt;controller&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;controller&gt; | builds `signals.rs:94` | stores `RoadManager.cpp:5472` | - | builds `ControllerParser.cpp:23` |
| &lt;controller&gt; `@id` | builds `signals.rs:116` | stores `RoadManager.cpp:5474` | - | builds `ControllerParser.cpp:27` |
| &lt;controller&gt; `@name` | stores `signals.rs:112` | stores `RoadManager.cpp:5475` | - | stores `ControllerParser.cpp:28` |
| &lt;controller&gt; `@sequence` | stores `signals.rs:91` | stores `RoadManager.cpp:5476` | - | stores `ControllerParser.cpp:29` |
| &lt;controller&gt; &lt;control&gt; | builds `signals.rs:97` | stores `RoadManager.cpp:5479` | - | builds `ControllerParser.cpp:38` |
| &lt;controller&gt; &lt;control&gt; `@signalId` | builds `signals.rs:98` | stores `RoadManager.cpp:5483` | - | builds `ControllerParser.cpp:39` |
| &lt;controller&gt; &lt;control&gt; `@type` | stores `signals.rs:103` | stores `RoadManager.cpp:5484` | - | - |

### &lt;junction&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;junction&gt; | builds `mod.rs:514` | builds `RoadManager.cpp:5491` | builds `OpenDriveMap.cpp:92` | builds `JunctionParser.cpp:44` |
| &lt;junction&gt; `@name` | - | stores `RoadManager.cpp:5493` | stores `OpenDriveMap.cpp:98` | stores `JunctionParser.cpp:47` |
| &lt;junction&gt; `@id` | builds `mod.rs:515` | builds `RoadManager.cpp:5495` | builds `OpenDriveMap.cpp:95` | builds `JunctionParser.cpp:46` |
| &lt;junction&gt; `@type` | builds `links.rs:249` | builds `RoadManager.cpp:5494` | - | - |
| &lt;junction&gt; `@mainRoad` | - | - | - | - |
| &lt;junction&gt; `@sStart` | - | - | - | - |
| &lt;junction&gt; `@sEnd` | - | - | - | - |
| &lt;junction&gt; `@orientation` | - | - | - | - |
| &lt;junction&gt; &lt;connection&gt; | builds `links.rs:316` | builds `RoadManager.cpp:5510` | builds `OpenDriveMap.cpp:101` | builds `JunctionParser.cpp:50` |
| &lt;junction&gt; &lt;connection&gt; `@id` | builds `links.rs:323` | stores `RoadManager.cpp:5515` | stores `OpenDriveMap.cpp:110` | stores `JunctionParser.cpp:53` |
| &lt;junction&gt; &lt;connection&gt; `@incomingRoad` | builds `links.rs:318` | builds `RoadManager.cpp:5517` | builds `OpenDriveMap.cpp:114` | builds `JunctionParser.cpp:54` |
| &lt;junction&gt; &lt;connection&gt; `@contactPoint` | builds `links.rs:253` | builds `RoadManager.cpp:5548` | builds `OpenDriveMap.cpp:103` | - |
| &lt;junction&gt; &lt;connection&gt; &lt;laneLink&gt; | builds `links.rs:333` | builds `RoadManager.cpp:5564` | builds `OpenDriveMap.cpp:119` | builds `JunctionParser.cpp:58` |
| &lt;junction&gt; &lt;connection&gt; &lt;laneLink&gt; `@from` | builds `links.rs:336` | builds `RoadManager.cpp:5567` | builds `OpenDriveMap.cpp:121` | builds `JunctionParser.cpp:61` |
| &lt;junction&gt; &lt;connection&gt; &lt;laneLink&gt; `@to` | builds `links.rs:337` | builds `RoadManager.cpp:5568` | builds `OpenDriveMap.cpp:121` | builds `JunctionParser.cpp:62` |
| &lt;junction&gt; &lt;connection&gt; &lt;laneLink&gt; `@overlapZone` | - | - | - | - |
| &lt;junction&gt; &lt;connection&gt; &lt;laneLink&gt; `@fromLayer` | - | - | - | - |
| &lt;junction&gt; &lt;connection&gt; &lt;laneLink&gt; `@toLayer` | - | - | - | - |
| &lt;junction&gt; &lt;connection&gt; `@type` | - | - | - | - |
| &lt;junction&gt; &lt;connection&gt; `@connectingRoad` | builds `links.rs:313` | builds `RoadManager.cpp:5527` | builds `OpenDriveMap.cpp:115` | builds `JunctionParser.cpp:55` |
| &lt;junction&gt; &lt;connection&gt; &lt;predecessor&gt; | - | - | - | - |
| &lt;junction&gt; &lt;connection&gt; &lt;predecessor&gt; `@elementType` | - | - | - | - |
| &lt;junction&gt; &lt;connection&gt; &lt;predecessor&gt; `@elementId` | - | - | - | - |
| &lt;junction&gt; &lt;connection&gt; &lt;predecessor&gt; `@elementS` | - | - | - | - |
| &lt;junction&gt; &lt;connection&gt; &lt;predecessor&gt; `@elementDir` | - | - | - | - |
| &lt;junction&gt; &lt;connection&gt; &lt;successor&gt; | - | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; | builds `junction_areas.rs:63` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; `@crossingRoad` | builds `junction_areas.rs:64` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; `@id` | builds `junction_areas.rs:62` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; `@roadAtEnd` | builds `junction_areas.rs:93` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; `@roadAtStart` | builds `junction_areas.rs:92` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; &lt;startLaneLink&gt; | builds `junction_areas.rs:92` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; &lt;startLaneLink&gt; `@s` | builds `junction_areas.rs:69` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; &lt;startLaneLink&gt; `@from` | builds `junction_areas.rs:84` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; &lt;startLaneLink&gt; `@to` | builds `junction_areas.rs:86` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; &lt;endLaneLink&gt; | builds `junction_areas.rs:93` | - | - | - |
| &lt;junction&gt; &lt;priority&gt; | stores `mod.rs:516` | - | stores `OpenDriveMap.cpp:131` | - |
| &lt;junction&gt; &lt;priority&gt; `@high` | stores `mod.rs:517` | - | stores `OpenDriveMap.cpp:133` | - |
| &lt;junction&gt; &lt;priority&gt; `@low` | stores `mod.rs:517` | - | stores `OpenDriveMap.cpp:133` | - |
| &lt;junction&gt; &lt;controller&gt; | builds `signals.rs:125` | stores `RoadManager.cpp:5575` | stores `OpenDriveMap.cpp:137` | builds `JunctionParser.cpp:71` |
| &lt;junction&gt; &lt;controller&gt; `@id` | builds `signals.rs:126` | stores `RoadManager.cpp:5579` | stores `OpenDriveMap.cpp:139` | builds `JunctionParser.cpp:72` |
| &lt;junction&gt; &lt;controller&gt; `@type` | stores `signals.rs:133` | stores `RoadManager.cpp:5580` | stores `OpenDriveMap.cpp:142` | - |
| &lt;junction&gt; &lt;controller&gt; `@sequence` | stores `signals.rs:91` | stores `RoadManager.cpp:5581` | stores `OpenDriveMap.cpp:143` | - |
| &lt;junction&gt; &lt;surface&gt; | builds `mod.rs:599` | - | - | - |
| &lt;junction&gt; &lt;planView&gt; | builds `junction_areas.rs:127` | - | - | - |
| &lt;junction&gt; &lt;objects&gt; | - | - | - | - |
| &lt;junction&gt; &lt;connection&gt; `@linkedRoad` | builds `links.rs:311` | builds `RoadManager.cpp:5523` | - | - |
| &lt;junction&gt; &lt;roadSection&gt; | - | - | - | - |
| &lt;junction&gt; &lt;roadSection&gt; `@id` | - | - | - | - |
| &lt;junction&gt; &lt;roadSection&gt; `@roadId` | - | - | - | - |
| &lt;junction&gt; &lt;roadSection&gt; `@sStart` | - | - | - | - |
| &lt;junction&gt; &lt;roadSection&gt; `@sEnd` | - | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; | builds `junction_areas.rs:24` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; | builds `junction_areas.rs:176` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@roadId` | builds `junction_areas.rs:177` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@type` | builds `junction_areas.rs:178` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@boundaryLane` | builds `junction_areas.rs:274` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@sStart` | builds `junction_areas.rs:197` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@sEnd` | builds `junction_areas.rs:276` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@contactPoint` | builds `junction_areas.rs:301` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@jointLaneStart` | builds `junction_areas.rs:313` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@jointLaneEnd` | builds `junction_areas.rs:313` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@transitionLength` | - | - | - | - |
| &lt;junction&gt; &lt;elevationGrid&gt; | builds `junction_areas.rs:25` | - | - | - |
| &lt;junction&gt; &lt;elevationGrid&gt; `@sStart` | builds `junction_areas.rs:65` | - | - | - |
| &lt;junction&gt; &lt;elevationGrid&gt; `@gridSpacing` | builds `junction_areas.rs:128` | - | - | - |
| &lt;junction&gt; &lt;elevationGrid&gt; &lt;elevation&gt; | builds `junction_areas.rs:147` | - | - | - |
| &lt;junction&gt; &lt;elevationGrid&gt; &lt;elevation&gt; `@left` | builds `junction_areas.rs:73` | - | - | - |
| &lt;junction&gt; &lt;elevationGrid&gt; &lt;elevation&gt; `@center` | builds `junction_areas.rs:72` | - | - | - |
| &lt;junction&gt; &lt;elevationGrid&gt; &lt;elevation&gt; `@right` | builds `junction_areas.rs:74` | - | - | - |

### &lt;junctionGroup&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;junctionGroup&gt; | - | - | - | - |
| &lt;junctionGroup&gt; `@name` | - | - | - | - |
| &lt;junctionGroup&gt; `@id` | - | - | - | - |
| &lt;junctionGroup&gt; `@type` | - | - | - | - |
| &lt;junctionGroup&gt; &lt;junctionReference&gt; | - | - | - | - |
| &lt;junctionGroup&gt; &lt;junctionReference&gt; `@junction` | - | - | - | - |

### &lt;station&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;station&gt; | - | - | - | - |
| &lt;station&gt; `@name` | - | - | - | - |
| &lt;station&gt; `@id` | - | - | - | - |
| &lt;station&gt; `@type` | - | - | - | - |
| &lt;station&gt; &lt;platform&gt; | - | - | - | - |
| &lt;station&gt; &lt;platform&gt; `@name` | - | - | - | - |
| &lt;station&gt; &lt;platform&gt; `@id` | - | - | - | - |
| &lt;station&gt; &lt;platform&gt; &lt;segment&gt; | - | - | - | - |
| &lt;station&gt; &lt;platform&gt; &lt;segment&gt; `@roadId` | - | - | - | - |
| &lt;station&gt; &lt;platform&gt; &lt;segment&gt; `@sStart` | - | - | - | - |
| &lt;station&gt; &lt;platform&gt; &lt;segment&gt; `@sEnd` | - | - | - | - |
| &lt;station&gt; &lt;platform&gt; &lt;segment&gt; `@side` | - | - | - | - |

### &lt;vmsGroup&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;vmsGroup&gt; | - | - | - | - |
| &lt;vmsGroup&gt; `@id` | - | - | - | - |
| &lt;vmsGroup&gt; &lt;vmsBoardReference&gt; | - | - | - | - |
| &lt;vmsGroup&gt; &lt;vmsBoardReference&gt; `@signalId` | - | - | - | - |
| &lt;vmsGroup&gt; &lt;vmsBoardReference&gt; `@vmsIndex` | - | - | - | - |
| &lt;vmsGroup&gt; &lt;vmsBoardReference&gt; `@groupIndex` | - | - | - | - |

### &lt;dataQuality&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;dataQuality&gt; | - | - | - | - |
| &lt;dataQuality&gt; &lt;error&gt; | - | - | - | - |
| &lt;dataQuality&gt; &lt;error&gt; `@xyAbsolute` | - | - | - | - |
| &lt;dataQuality&gt; &lt;error&gt; `@zAbsolute` | - | - | - | - |
| &lt;dataQuality&gt; &lt;error&gt; `@xyRelative` | - | - | - | - |
| &lt;dataQuality&gt; &lt;error&gt; `@zRelative` | - | - | - | - |
| &lt;dataQuality&gt; &lt;rawData&gt; | - | - | - | - |
| &lt;dataQuality&gt; &lt;rawData&gt; `@date` | - | - | - | - |
| &lt;dataQuality&gt; &lt;rawData&gt; `@source` | - | - | - | - |
| &lt;dataQuality&gt; &lt;rawData&gt; `@sourceComment` | - | - | - | - |
| &lt;dataQuality&gt; &lt;rawData&gt; `@postProcessing` | - | - | - | - |
| &lt;dataQuality&gt; &lt;rawData&gt; `@postProcessingComment` | - | - | - | - |

### &lt;include&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;include&gt; | - | - | - | - |
| &lt;include&gt; `@file` | - | - | - | - |

### &lt;userData&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;userData&gt; | stores `mod.rs:2354` | builds `RoadManager.cpp:5397` | - | - |
| &lt;userData&gt; `@code` | stores `mod.rs:2358` | builds `RoadManager.cpp:5399` | - | - |
| &lt;userData&gt; `@value` | stores `mod.rs:2359` | builds `RoadManager.cpp:5402` | - | - |
