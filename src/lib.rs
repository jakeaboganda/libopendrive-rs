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
//! revision. `poly3` is deprecated there, still specified, and still read
//! here. The test suite imports real files declaring 1.4, 1.6 and 1.7.
//!
//! # Which elements
//!
//! | Element | Attributes read |
//! | --- | --- |
//! | `<road>` | `id`, `length` |
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
//! | `<left>`, `<right>` | none |
//! | `<lane>` | `id`, `type` |
//! | `<lane><width>` | `sOffset`, `a`, `b`, `c`, `d` |
//! | `<lane><link>` | `id` |
//! | `<junction>` | `id` |
//! | `<connection>` | `incomingRoad`, `connectingRoad`, `contactPoint` |
//! | `<laneLink>` | `from`, `to` |
//! | `<objects><object>` | `id`, `type`, `subtype`, `name`, `dynamic`, `orientation`, `validLength`, `s`, `t`, `zOffset`, `hdg`, `pitch`, `roll`, `length`, `width`, `height`, `radius` |
//! | `<objects><objectReference>` | `id`, `s`, `t`, `zOffset`, `orientation`, `validLength` |
//! | `<objects><tunnel>` | `id`, `name`, `type`, `s`, `length`, `lighting`, `daylight` |
//! | `<objects><bridge>` | `id`, `name`, `type`, `s`, `length` |
//! | `<validity>`, under `<object>`, `<objectReference>`, `<tunnel>` and `<bridge>` | `fromLane`, `toLane` |
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
//! # What the importer ignores
//!
//! Everything else in the file, silently. That includes `<geoReference>`,
//! `<signals>`, `<roadMark>`, `<controller>`, `<junctionGroup>`,
//! `<station>`, and road `<type>` with its `<speed>`.
//!
//! Three omissions change the road you get back, rather than only dropping
//! detail around it:
//!
//! - `<shape>`, the other lateralProfile child, so a crowned or cambered
//!   cross-section imports flat across its width.
//! - A lane's `<border>`, as opposed to an object's. A lane whose extent
//!   comes from a border rather than a width element has nothing to sample,
//!   so the importer drops it.
//! - `<center>`, so lane 0 never becomes a [`Lane`].
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
mod geometry;
mod grid;
mod mesh;
mod network;
mod object;
mod object_mesh;
mod parse;
mod route;
mod structure;

#[cfg(test)]
mod fixtures;

pub use coords::{Point, Vector};
pub use geometry::TooFewPoints;
pub use geometry::{Polyline, Pose, Projection, RoadSample};
pub use mesh::{LaneSpan, Mesh, MeshError, MeshSampler};
pub use network::{Direction, Lane, LaneId, LaneType, RoadNetwork};
pub use object::{
    Border, Corner, Extent, Marking, Material, Object, ObjectId, ObjectType, ParkingSpace, Section,
    Shape, UserData,
};
pub use object_mesh::ObjectSpan;
pub use parse::{
    load_file, load_file_with_provenance, load_str, load_str_with_provenance, ImportError,
    LaneProvenance, ObjectProvenance, Orientation, Provenance, StructureProvenance,
};
pub use structure::{Coverage, Structure, StructureId, StructureKind};
