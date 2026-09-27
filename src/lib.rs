#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(rustdoc::broken_intra_doc_links)]
//! A pure-Rust OpenDRIVE (`.xodr`) importer.
//!
//! OpenDRIVE describes roads analytically: clothoids, arcs, cubic elevation
//! and width profiles, lane links. This crate evaluates all of it once, at
//! load, and hands back a [`RoadNetwork`] of plain polylines. Nothing
//! downstream touches OpenDRIVE again. Consumers sample points, walk the
//! lane graph, and tessellate a surface mesh.
//!
//! No C++ dependency, no bindings, no `unsafe`, and no math crate in the
//! public API.
//!
//! # Coordinate types
//!
//! A position is a [`Point`] and a direction or displacement is a [`Vector`].
//! They are separate types carrying the arithmetic that relates them:
//! `point - point` is a `Vector`, `point + vector` is a `Point`, and adding
//! two positions does not compile. One three-float type used for all three
//! roles would make `nearest_lane(sample.up)` legal, which it is not.
//!
//! Both are `#[repr(C)]` structs of three public `f32` fields, so handing one
//! to another math library is `Point::to_array`.
//!
//! ```no_run
//! let net = libopendrive::load_file("maps/town07.xodr")?;
//! let lane = net.driving_lanes().next().expect("a driving lane");
//! let pose = lane.center.pose_at(25.0);
//! let route = net.route(pose.position, lane.center.point_at(400.0));
//! # Ok::<(), libopendrive::ImportError>(())
//! ```
//!
//! # Which OpenDRIVE version
//!
//! The importer does not read `<header>`. It never inspects `revMajor` or
//! `revMinor`, and it never rejects a file for its version. Whether a file
//! loads depends only on whether it uses the elements below.
//!
//! Every one of those elements is in ASAM OpenDRIVE 1.9.0, the current
//! revision. `poly3`, and a signal's `<positionRoad>` and
//! `<positionInertial>`, are deprecated there, still specified, and still
//! read here. The test suite imports real files declaring 1.4, 1.6 and 1.7.
//!
//! # Which elements
//!
//! | Element | Attributes read |
//! | --- | --- |
//! | `<road>` | `id`, `length`, `junction` |
//! | `<road><link>` | `elementType`, `elementId`, `contactPoint` |
//! | `<planView><geometry>` | `s`, `x`, `y`, `hdg`, `length` |
//! | `<line>` | none |
//! | `<arc>` | `curvature` |
//! | `<spiral>` | `curvStart`, `curvEnd` |
//! | `<poly3>` | `a`, `b`, `c`, `d` |
//! | `<paramPoly3>` | `aU`, `bU`, `cU`, `dU`, `aV`, `bV`, `cV`, `dV`, `pRange` |
//! | `<elevationProfile><elevation>` | `s`, `a`, `b`, `c`, `d` |
//! | `<lateralProfile><superelevation>` | `s`, `a`, `b`, `c`, `d` |
//! | `<lanes><laneOffset>` | `s`, `a`, `b`, `c`, `d` |
//! | `<laneSection>` | `s` |
//! | `<left>`, `<center>`, `<right>` | none |
//! | `<lane>` | `id`, `type` |
//! | `<lane><width>` | `sOffset`, `a`, `b`, `c`, `d` |
//! | `<lane><height>` | `sOffset`, `inner`, `outer` |
//! | `<lane><roadMark>` | `sOffset`, `type`, `weight`, `color`, `width`, `height`, `laneChange` |
//! | `<roadMark><type>` | `width` |
//! | `<type><line>` | `length`, `space`, `tOffset`, `sOffset`, `rule`, `width`, `color` |
//! | `<roadMark><explicit>` | none |
//! | `<explicit><line>` | `length`, `tOffset`, `sOffset`, `rule`, `width` |
//! | `<roadMark><sway>` | `ds`, `a`, `b`, `c`, `d` |
//! | `<lane><link>` | `id` |
//! | `<junction>` | `id` |
//! | `<connection>` | `incomingRoad`, `connectingRoad`, `contactPoint` |
//! | `<laneLink>` | `from`, `to` |
//! | `<objects><object>` | `id`, `type`, `subtype`, `name`, `dynamic`, `orientation`, `validLength`, `s`, `t`, `zOffset`, `hdg`, `pitch`, `roll`, `length`, `width`, `height`, `radius` |
//! | `<objects><objectReference>` | `id`, `s`, `t`, `zOffset`, `orientation`, `validLength` |
//! | `<objects><tunnel>` | `id`, `name`, `type`, `s`, `length`, `lighting`, `daylight` |
//! | `<objects><bridge>` | `id`, `name`, `type`, `s`, `length` |
//! | `<signals><signal>` | `id`, `name`, `dynamic`, `orientation`, `s`, `t`, `zOffset`, `hOffset`, `pitch`, `roll`, `country`, `countryRevision`, `type`, `subtype`, `value`, `unit`, `text`, `length`, `width`, `height`, `invalidated`, `temporary` |
//! | `<signal><positionRoad>` | `roadId`, `s`, `t`, `zOffset`, `hOffset`, `pitch`, `roll` |
//! | `<signal><positionInertial>` | `x`, `y`, `z`, `hdg`, `pitch`, `roll` |
//! | `<signal><dependency>` | `id`, `type` |
//! | `<signal><reference>` | `elementType`, `elementId`, `type` |
//! | `<signals><signalReference>` | `id`, `s`, `t`, `orientation` |
//! | `<controller>`, at the top level | `id`, `name`, `sequence` |
//! | `<controller><control>` | `signalId`, `type` |
//! | `<junction><controller>` | `id`, `type`, `sequence` |
//! | `<validity>`, under `<object>`, `<objectReference>`, `<signal>`, `<signalReference>`, `<tunnel>` and `<bridge>` | `fromLane`, `toLane` |
//! | `<object><parkingSpace>` | `access`, `restrictions` |
//! | `<object><material>` | `surface`, `friction`, `roughness` |
//! | `<object><userData>` | `code`, `value` |
//! | `<object><repeat>` | `s`, `length`, `distance`, and the `Start`/`End` pair of `t`, `zOffset`, `length`, `width`, `height`, `radius` |
//! | `<outlines><outline>` | `id`, `closed`, `outer` |
//! | `<cornerRoad>` | `id`, `s`, `t`, `dz`, `height` |
//! | `<cornerLocal>` | `id`, `u`, `v`, `z`, `height` |
//! | `<markings><marking>` | `side`, `color`, `width`, `zOffset`, `lineLength`, `spaceLength`, `startOffset`, `stopOffset` |
//! | `<borders><border>` | `type`, `width`, `outlineId`, `useCompleteOutline` |
//! | `<cornerReference>` | `id` |
//! | `<road><surface><CRG>` | `file`, `mode`, `purpose`, `orientation`, `sStart`, `sEnd`, `sOffset`, `tOffset`, `hOffset`, `xOffset`, `yOffset`, `zOffset`, `zScale` |
//! | `<junction><surface><CRG>` | `file`, `mode`, `purpose`, `xOffset`, `yOffset`, `hOffset`, `zOffset`, `zScale` |
//!
//! Four attribute values steer the import:
//!
//! - `<lane type>` chooses the [`LaneType`] a lane bakes as. A name this
//!   crate does not recognise bakes as [`LaneType::Unknown`], so no lane is
//!   ever dropped for its type.
//! - `<link elementType>` is `junction`, or a road for any other value.
//! - `contactPoint` is `end`, or the start for any other value.
//! - `<paramPoly3 pRange>` is `arcLength`, matched without case, or
//!   normalized for any other value.
//!
//! `<link>`s and `<junction>`s resolve into a drive-direction lane graph.
//! "Successor" means "a lane you can drive into off this lane's exit end",
//! not a raw mirror of the file's `+s` links.
//!
//! Each `<object>` bakes to one or more [`Object`]s in world coordinates,
//! sitting on the road surface. As in libOpenDRIVE, an object's own frame
//! follows the road under it: `hdg`, `pitch` and `roll` turn it against the
//! road's grade and bank, and `zOffset` raises it square to the surface. So
//! an object on a banked road leans with the bank. `<repeat>` sweeps and
//! `<cornerRoad>` corners are given in road coordinates, not the object's
//! frame, and still rise straight up. `<object type>`
//! chooses their [`ObjectType`], and an unrecognised name bakes as
//! [`ObjectType::Unknown`]. The [`Shape`] follows libOpenDRIVE:
//!
//! - A plain object is a [`Shape::Solid`] at its `(s, t)`. A `radius` makes
//!   its [`Extent`] a cylinder. Any of `length`, `width` and `height` makes it
//!   a box, 0 in the dimensions not given.
//! - A `<repeat>` with a `distance` is one [`Shape::Solid`] every `distance`
//!   metres. One with a `distance` of 0 is a [`Shape::Sweep`], its
//!   cross-section swept continuously along the road, such as a guard rail.
//!   A `radius` makes the sweep round, a pipe resting on its `zOffset`, and
//!   its `width` and `height` play no part.
//! - Each `<outline>` is a [`Shape::Outline`], and an object with outlines
//!   has no solid of its own. `<outline>` is read under `<outlines>`, and
//!   straight under `<object>` as OpenDRIVE 1.4 writes it.
//! - Under a `<repeat>` with a `distance`, the outlines are baked at every
//!   step in place of the solid. libOpenDRIVE bakes them once. At each step
//!   `<cornerLocal>` corners follow the object's frame, and `<cornerRoad>`
//!   corners move by the step's offset from the object's `(s, t)`. The
//!   repeat's dimensions do not resize them.
//! - An outline with `outer="false"` is a hole. It goes into the holes of
//!   the first closed outer outline of the same object that encloses it, and
//!   the importer drops it if there is none.
//!
//! An `<objectReference>` bakes the `<object>` it names, on whichever road
//! that is, as if it stood at the reference's `s` and `t` with the
//! reference's `zOffset`. What the object gives in road coordinates, its
//! repeats and `<cornerRoad>`s, moves by the same distance along and across
//! the road. A reference to an id no `<object>` has is skipped.
//!
//! [`Object::lanes`] is the lanes alongside the stretch of road an object
//! spans, the lane sections it stands in or sweeps across, narrowed to the
//! `fromLane`-`toLane` ranges of its `<validity>`s if it has any. A
//! reference's own `<validity>`s apply, not its `<object>`'s, whose lane ids
//! are on another road.
//!
//! A `<marking>` becomes a [`Marking`] on the outline that has every corner
//! its `<cornerReference>`s name. The marking is a strip `width` across,
//! centred on the edges through those corners, and cut into dashes by
//! `lineLength` and `spaceLength`. A marking with no corner references
//! paints the edge of its `side` of a solid's box, on its base: `front`,
//! `rear`, `left` or `right`. A cylinder's box is the square round it.
//!
//! A `<border>` becomes a [`Border`] on the outline its `outlineId` names, or
//! on every outline it fits if it names none. The border is a band `width`
//! across, centred on every edge with `useCompleteOutline`, and on the edges
//! through its `<cornerReference>`s otherwise.
//!
//! Both lie in the surface under the edges: the road's for `<cornerRoad>`
//! corners, and the plane of the object's frame for `<cornerLocal>` corners
//! and a solid's box. So on a banked road a band tilts with the bank rather
//! than lying level, and paint is raised off the road along its normal.
//!
//! An object's `<parkingSpace>` becomes its [`ParkingSpace`], and each
//! `<material>` one of its [`Material`]s. Each `<userData>` is a
//! [`UserData`] of its `code` and `value`. Any XML nested inside one is not
//! kept.
//!
//! An [`Object`] keeps what describes the thing itself: its `subtype`, and
//! whether it is `dynamic`. What ties it to an OpenDRIVE road, its road id,
//! `<object id>`, `(s, t)`, `orientation` and `validLength`, is in its
//! [`ObjectProvenance`], from [`load_str_with_provenance`] or
//! [`load_file_with_provenance`], the same way [`LaneProvenance`] names a
//! lane's road. For a referenced object, those are the reference's, and
//! [`ObjectProvenance::referenced_from`] names the road the original is on.
//! [`RoadNetwork::object_mesh`] tessellates them all. Markings and borders
//! are not in it: they are already quads, in [`Marking::pieces`] and
//! [`Border::pieces`].
//!
//! Each `<tunnel>` and `<bridge>` bakes to a [`Structure`] with no geometry,
//! since OpenDRIVE describes neither a tube nor a deck. It covers the lanes
//! alongside its `length` metres of road from `s`, narrowed by its
//! `<validity>`s. A [`Coverage`] says where it starts and ends along each
//! lane. [`RoadNetwork::structures_over`] tells you whether a lane runs
//! through a tunnel or over a bridge. Its road id, id, `s` and `length` are
//! in its [`StructureProvenance`].
//!
//! Each `<signal>` bakes to a [`Signal`] in [`RoadNetwork::signals`]. It
//! keeps the catalogue codes the file gives, `country`, `type`, `subtype`,
//! `value`, `unit` and `text`, and looks nothing up in a catalogue. Its board
//! stands `zOffset` straight above the road surface at its `(s, t)`, the
//! point in [`Signal::applies_at`]. A `+` signal faces back along the road,
//! toward traffic running along `+s`. A `-` or `none` signal faces along it.
//! `hOffset` turns it counter-clockwise from there. `pitch` and `roll` are
//! against the horizontal, as the spec has them, so unlike an object, a board
//! on a banked road stays upright. Angles wrap into `(-π, π]`, since real
//! files give `hOffset`s of several turns.
//!
//! A `<positionRoad>` stands the board on the road it names instead, the same
//! way, with its own `s`, `t`, `zOffset`, `hOffset`, `pitch` and `roll`, and
//! still facing the traffic the signal's `orientation` names. A
//! `<positionInertial>` stands it at `(x, y, z)`, facing `hdg`. Either moves
//! only the board. The signal still takes effect at its own `(s, t)`, and
//! applies to the lanes there. One that names no road, or is missing a
//! coordinate, leaves the board at the signal's station.
//!
//! [`Signal::lanes`] is the lanes at `s` on the side of the road its
//! `orientation` names: negative ids for `+`, positive ids for `-`, and both
//! for `none`. Its `<validity>` ranges replace that side rather than narrow
//! it. The spec says they must lie within it, but esmini's maps give `+`
//! signals validities on both sides. The road id, `<signal id>`, `(s, t)` and
//! `orientation` are in its [`SignalProvenance`]. A signal missing `s` or
//! `t`, or off the ends of its road, is skipped.
//!
//! A `<signalReference>` applies the signal it names on its own road too,
//! and draws nothing new. It adds the lanes at its `s`, picked by its own
//! `orientation` and `<validity>`s the same way, to [`Signal::lanes`], and
//! its point to [`Signal::applies_at`].
//! [`SignalProvenance::references`] records its road, `(s, t)` and
//! `orientation`. Where two signals share an id, a reference applies the
//! first. A reference to an id no signal has is skipped.
//!
//! A signal's `<dependency>`s become its [`Signal::dependencies`], each the
//! signal it names and the `type`. OpenDRIVE 1.7 says a dependency names the
//! signal controlled, and 1.9 the one controlling, so the importer keeps the
//! link as the file gives it. Its `<reference>`s become its
//! [`Signal::references`]: a signal, or each [`Object`] the named `<object>`
//! baked to, with the `type`. One to an id nothing has, or to an
//! `elementType` other than `signal` or `object`, is skipped.
//!
//! Each top-level `<controller>` bakes to a [`Controller`] in
//! [`RoadNetwork::controllers`], with the signals its `<control>`s name, and
//! each of those signals lists it in [`Signal::controllers`]. A control that
//! names no signal is skipped, and its controller kept. A `<junction>`'s own
//! `<controller>` list names controllers that switch in step there. The
//! baked network has no junctions, so each entry is in the
//! [`ControllerProvenance`] of the controller it names.
//!
//! # Lane heights
//!
//! A lane's `<height>`s raise its surface off the road, as a sidewalk or a
//! kerb stands above the lanes beside it. Each gives the height at the
//! lane's inner and outer border, along the road's normal, from its
//! `sOffset` in the lane section. The baked [`Lane`] stands at that height.
//! Its centerline rises by the height halfway across it, and
//! [`Lane::bank`] adds the slope from its inner border to its outer one. So
//! the surface mesh and [`Lane::sample_at`] see the raised surface.
//! [`LaneProvenance::heights`] gives the heights at each centerline vertex.
//! Where a lane's heights change pace, its lane section has a station, so a
//! ramp 1 m long bakes 1 m long.
//!
//! The tessellator tilts a raised lane's cross axis by its slope, rather
//! than moving each edge to its own height. On a 0.1 m rise across a 2 m
//! lane, that leaves the edges under 0.1 mm off their heights and 1.3 mm
//! inside their borders. On a grade they also sit 0.05 m times the grade
//! along the road from where the road's normal puts them, 1.5 mm at 3 %.
//! Where a lane's kerb and outer edge change height at different paces, as
//! on a ramp, each quad of its mesh is twisted, and its two triangles cut
//! the corner by up to a quarter of the rise across that quad. Halfway up
//! a 0.1 m kerb ramp the mesh is 25 mm off. libOpenDRIVE also meshes a lane
//! as quads between its borders.
//!
//! The step between a raised lane and the lane beside it stays open. The
//! spec defines no kerb face, and libOpenDRIVE and esmini draw none.
//!
//! Objects and signals stand on the lane at their point, and a road mark on
//! its own lane. So a pole on a sidewalk stands on the sidewalk, a
//! sidewalk's outer mark rises with it, and the mark along the kerb, which
//! belongs to the lane below it, stays on the road. An object still leans
//! with the road, not with the slope of the lane under it, as in
//! libOpenDRIVE.
//!
//! The crate departs from the OpenDRIVE 1.9 spec here:
//!
//! - The spec's rule for lane geometry holds a height until the next one.
//!   The crate goes straight from each height to the next, as libOpenDRIVE
//!   and esmini do, and holds the last one after it. esmini's maps are
//!   written for this reading. Read as steps, a sidewalk giving 0.02 m at
//!   0 m and 2 m and 0.12 m at 3 m would jump at 3 m rather than ramp up
//!   from 2 m.
//! - The spec gives only the heights at a lane's two borders. The crate goes
//!   straight across from one to the other, as both readers do.
//! - The spec gives no height before a lane's first `<height>`. The first
//!   one holds there. libOpenDRIVE carries the first ramp on backwards.
//! - The spec requires `sOffset`, `inner` and `outer`. A missing one is 0,
//!   as libOpenDRIVE reads it. A negative `sOffset` is 0, as the crate reads
//!   a road mark's.
//! - The spec says a lane's heights come in ascending `sOffset`. Ones out of
//!   order are sorted rather than dropped.
//! - The spec forbids heights on the center lane. The crate never bakes the
//!   center lane, so it ignores them.
//! - The spec does not say whether objects, signals and road marks stand
//!   on a raised lane or on the road below it. The crate stands them on the
//!   lane.
//! - The spec does not say which lane a point on the border between two is
//!   on. The crate puts it on the inner one, as libOpenDRIVE does.
//! - The spec gives no height past the outermost lane. A point there takes
//!   that lane's outer height. libOpenDRIVE carries the lane's slope on.
//! - The spec measures a height from the road including its surface. An
//!   `attached` CRG adds its grid to the road without the lane's height, so
//!   a CRG laid over a raised lane answers at road level.
//!
//! # Road marks
//!
//! Each `<roadMark>` bakes to a [`RoadMark`] in [`RoadNetwork::road_marks`].
//! It runs along its lane's outer border, from its `sOffset` in the lane
//! section to the lane's next `<roadMark>` or the end of the section. The
//! center lane's marks run along the line between the two sides. A mark
//! names the lanes either side of it, [`RoadMark::left`] and
//! [`RoadMark::right`], looking along `+s`. Its `type` is a
//! [`RoadMarkType`], its `laneChange` a [`LaneChange`], and its `color` stays
//! the file's text. Its road, lane section, `<lane id>`, `s` and length are in
//! its [`RoadMarkProvenance`].
//!
//! Each of its [`RoadMark::lines`] is quads lying in the road surface, placed
//! at the lane's own stations, so the paint lies on the lane mesh. A mark's
//! `<type><line>`s and `<explicit><line>`s are its lines, whatever its
//! `type` other than `none`. Each is a [`RoadMarkLine`] with its own width,
//! colour, [`LinePattern`] and [`LineRule`], `tOffset` from the border. A
//! `<type><line>` repeats from its `sOffset` to the end of the mark, and an
//! `<explicit><line>` paints once. A mark with no lines gets stand-ins for
//! its type: one line for `solid` and `broken`, two for a double type, and
//! none for the rest. Each `<sway>` moves every line of the mark sideways by
//! its cubic, from its `ds` along the mark to the next. The marks of a lane
//! section too short to bake do not bake either.
//!
//! The crate departs from the OpenDRIVE 1.9 spec here:
//!
//! - The spec requires `color`, `type` and `sOffset`. A mark missing one is
//!   `standard`, `none` or 0, as libOpenDRIVE reads it. A `type` this crate
//!   does not know is [`RoadMarkType::Unknown`].
//! - The spec says a mark's and a line's `sOffset` are at least 0. A
//!   negative one is 0, as libOpenDRIVE and esmini read it.
//! - The spec gives no default `weight`. A mark missing one is standard.
//! - The spec says a mark's, a `<type>`'s and a `<line>`'s width are above
//!   0. A width of 0 counts as none, so the next in line applies: the
//!   line's, then the type's, then the mark's, then the weight's. esmini
//!   writes 0 on all three.
//! - The spec makes a line's `length` the part painted, so a line with a
//!   `length` and a `space` of 0 paints nothing. The crate reads it as one
//!   continuous line, as esmini means it. Without this, every mark in
//!   esmini's `multi_intersections.xodr` vanishes.
//! - The spec lets a `none` mark have lines. It paints none, since esmini
//!   writes a line of no width under each of its `none` marks and draws
//!   nothing there.
//! - The spec says only that `tOffset` is a lateral offset from the border.
//!   The crate adds it along +t on either side, as libOpenDRIVE and esmini
//!   both do. So a positive `tOffset` moves a right lane's line inward and a
//!   left lane's outward.
//! - The spec does not say which way a `<sway>` moves the lines either. The
//!   crate reads it along +t, as it does `tOffset`, and moves nothing before
//!   the first `ds`.
//! - The spec gives no default `rule`. A line without one has
//!   [`LineRule::None`].
//! - The spec does not say how wide a line of each weight is, or how a type
//!   looks. A line is 0.12 m wide, or 0.25 m bold, as libOpenDRIVE has it.
//!   `broken` is dashes 4 m long with 8 m between, and a double type's lines
//!   are one width either side of the border, as esmini draws them. `none`,
//!   `edge`, `grass`, `curb`, `botts dots` and `custom` paint nothing.
//! - The spec does not say what a dash's length is measured along. The
//!   crate measures it along the reference line, as libOpenDRIVE and esmini
//!   do, so a dash on the outside of a bend is longer.
//! - The spec says a lane's marks come in ascending `sOffset`. Marks out of
//!   order are sorted, as lane sections are, rather than dropped.
//! - The spec draws the marks of a lane with no `<width>` on its border,
//!   which is then its inner border. The crate does not bake the lane, so
//!   its marks go with it, and raises [`Warning::LaneDropped`].
//!
//! # Road surfaces
//!
//! Each `<CRG>` under a road's or a junction's `<surface>` becomes a
//! [`CrgSurface`] in [`RoadNetwork::crg_surfaces`]. The importer reads the
//! record, not the OpenCRG file it names. [`RoadSurface::new`] loads the
//! files through a closure you pass. Then [`RoadSurface::sample`] gives the
//! height, normal and friction under a point, in `f64`. A CRG answers where
//! it covers the point, and the surface mesh answers everywhere else. The CRG
//! does not change the mesh.
//!
//! The modes follow the road surface section of ASAM OpenDRIVE 1.9:
//!
//! - `attached` lays the CRG grid along the road's reference line and adds
//!   its height to the road's. It ignores the file's reference line, with its
//!   height, slope and bank. The spec's formula evaluates the file with
//!   `crgEvaluv2z`, which includes them, but its text says they are
//!   disregarded. The importer follows the text.
//! - `attached0` lays the file along the road the same way, and its
//!   elevation replaces the road's height.
//! - `genuine` starts the file's own reference line at the road's
//!   `(sOffset, tOffset)`, turned by `hOffset`.
//! - `global` leaves the file in its own coordinates, moved by `xOffset`,
//!   `yOffset` and `hOffset`.
//!
//! Along the road, `u = s - sOffset` and `v = t - tOffset`. The importer
//! samples the reference line under a road CRG every 0.25 m and joins the
//! samples with arcs, so `(u, v)` is exact on lines and arcs. On a spiral
//! that tightens from straight to a 50 m radius over 50 m, it is within a
//! micrometre.
//! `orientation="opposite"` negates both. The spec's matrix for it also
//! swaps them, which contradicts its own text that the file turns 180
//! degrees. The importer follows the text.
//!
//! A road's CRG applies between `sStart` and `sEnd`, on the lanes of the
//! sections there. A junction's CRG applies on every lane of the roads in the
//! junction, before any CRG of those roads, since the spec says it supersedes
//! their elevation. A junction's CRG must be `global`. The importer reads no
//! junction reference line, so it skips the other modes there.
//!
//! A friction CRG ignores `zOffset` and `zScale`, as the spec says, and gives
//! the grid values exactly as the file has them. OpenCRG, like its C-API,
//! shifts the heights of a file without a `$ROAD_CRG_MODS` block so its
//! reference line starts at 0. Friction skips that shift.
//!
//! # What the importer ignores
//!
//! Everything else in the file, silently. That includes `<geoReference>`,
//! `<junctionGroup>`, `<station>`, an object's
//! `<surface>`, and road `<type>` with its `<speed>`. Of signals, it ignores
//! a signal's `<userData>`, the boards `<staticBoard>` and `<vmsBoard>`, and
//! `<semantics>`. Of road marks, it ignores `material` and a `<type>`'s
//! `name`.
//!
//! Three omissions change the road you get back, rather than only dropping
//! detail around it:
//!
//! - `<shape>`, the other lateralProfile child, so a crowned or cambered
//!   cross-section imports flat across its width.
//! - A lane's `<border>`, as opposed to an object's. A lane whose extent
//!   comes from a border rather than a width element has nothing to sample,
//!   so the importer drops it and raises [`Warning::LaneDropped`].
//! - The center lane, lane 0, so it never becomes a [`Lane`]. Only its road
//!   marks are read.
//!
//! # Coordinate frame
//!
//! Baked geometry is OpenDRIVE's own frame: right-handed, **Z-up, metres**,
//! with the reference line in the X-Y plane, `hdg` the heading within it, and
//! elevation along +Z. A point imports unchanged, so a coordinate you read out
//! of the `.xodr` is the coordinate you get back.
//!
//! An OpenDRIVE left turn (increasing `hdg`) curves toward +Y. Positive lane
//! offset `t` is to the left of the heading, which for heading +X is +Y.
//!
//! A renderer or physics engine that wants Y-up has to rotate on the way in.
//! Doing that here instead would mean every coordinate in this API disagreed
//! with the file it came from, which is the harder bug to find.
//!
//! Travel direction follows right-hand traffic: negative-id (right) lanes run
//! with `+s`, positive-id (left) lanes against it. OpenDRIVE encodes no travel
//! direction of its own, so a left-hand-traffic map imports with its
//! directions inverted.
//!
//! # Robustness
//!
//! `.xodr` files come from outside your program. A road the importer cannot
//! interpret is skipped rather than fatal, because losing a whole city map to
//! one junk road is the worse failure; [`load_str`] still errors if the
//! document yielded no lanes at all. Non-finite attribute values are rejected
//! at parse. Rust's float parser accepts `NaN` and turns `1e400` into
//! infinity, and one such value poisons every point derived from it.
//!
//! [`Provenance::warnings`] says what the load did with a bad file. Each
//! [`Warning`] names where in the file it happened, and prints as a
//! sentence. The crate raises one where it drops something, or reads a file
//! that breaks a rule of the spec:
//!
//! - [`Warning::RoadSkipped`] for a road with no finite `length`, no
//!   `<planView>`, or no `<geometry>` it can bake. The [`RoadSkipReason`]
//!   says which.
//! - [`Warning::LaneDropped`] for a lane with no `<width>` it can read.
//!
//! Elements the crate does not read at all raise none. Real maps are full of
//! them, and they would bury the rest. Other things it drops, such as a lane
//! section under 1 mm long or a signal off the end of its road, raise none
//! yet. The crate prints nothing. To log the warnings, print the list.
//!
//! # Importer contract
//!
//! Code baking other map formats into a [`RoadNetwork`] must:
//!
//! - build lane geometry with [`Polyline::try_new`] and surface an error on
//!   degenerate input, rather than the panicking [`Polyline::new`];
//! - where it splits one curve into contiguous lanes, build them with
//!   [`Polyline::try_new_with_tangents`] and pass the curve's analytical
//!   tangent at each end. A polyline that has to guess its end tangent guesses
//!   from its last chord, and the two lanes meeting at a joint then guess
//!   differently and leave a visible seam;
//! - keep [`LaneId`]s opaque, and never assume one indexes the lane list.
//!
//! On curves tighter than the half-width, [`RoadNetwork::surface_mesh`] pinches
//! the inner rib so the surface strip stays fold-free; the outer edge keeps its
//! full width and radius.

mod coords;
mod crg;
mod geometry;
mod grid;
mod mesh;
mod network;
mod object;
mod object_mesh;
mod parse;
mod road_mark;
mod route;
mod signal;
mod structure;

#[cfg(test)]
mod fixtures;

pub use coords::{Point, Vector};
pub use crg::{
    CrgAlong, CrgMode, CrgPose, CrgPurpose, CrgSurface, RoadSurface, SurfaceHint, SurfaceSample,
};
pub use geometry::TooFewPoints;
pub use geometry::{Polyline, Pose, Projection, RoadSample};
pub use mesh::{LaneSpan, Mesh, MeshError, MeshSampler};
pub use network::{Direction, Lane, LaneId, LaneType, RoadNetwork};
pub use object::{
    Border, Corner, Extent, Marking, Material, Object, ObjectId, ObjectType, ParkingSpace, Section,
    Shape, UserData,
};
pub use object_mesh::ObjectSpan;
/// The OpenCRG reader, for loading the files a [`RoadSurface`] evaluates.
pub use opencrg;
pub use parse::{
    load_file, load_file_with_provenance, load_str, load_str_with_provenance, ControllerProvenance,
    ImportError, JunctionControllerProvenance, LaneHeight, LaneProvenance, ObjectProvenance,
    Orientation, Provenance, RoadMarkProvenance, RoadSkipReason, SignalProvenance,
    SignalReferenceProvenance, StructureProvenance, Warning,
};
pub use road_mark::{
    LaneChange, LinePattern, LineRule, RoadMark, RoadMarkId, RoadMarkLine, RoadMarkType,
    RoadMarkWeight,
};
pub use signal::{
    Control, Controller, ControllerId, Dependency, Reference, Referenced, Signal, SignalId, Unit,
};
pub use structure::{Coverage, Structure, StructureId, StructureKind};
