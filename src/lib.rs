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
//!
//! Four attribute values steer the import:
//!
//! - `<lane type>` must be `driving`. Every other lane type is skipped.
//! - `<link elementType>` is `junction`, or a road for any other value.
//! - `contactPoint` is `end`, or the start for any other value.
//! - `<paramPoly3 pRange>` is `arcLength`, matched without case, or
//!   normalized for any other value.
//!
//! `<link>`s and `<junction>`s resolve into a drive-direction lane graph.
//! "Successor" means "a lane you can drive into off this lane's exit end",
//! not a raw mirror of the file's `+s` links.
//!
//! # What the importer ignores
//!
//! Everything else in the file, silently. That includes `<geoReference>`,
//! `<objects>`, `<signals>`, `<roadMark>`, `<controller>`,
//! `<junctionGroup>`, `<station>`, and road `<type>` with its `<speed>`.
//!
//! Four omissions change the road you get back, rather than only dropping
//! detail around it:
//!
//! - `<shape>`, the other lateralProfile child, so a crowned or cambered
//!   cross-section imports flat across its width.
//! - `<border>`. A lane whose extent comes from a border rather than a width
//!   element has nothing to sample, so the importer drops it.
//! - `<center>`, so lane 0 never becomes a [`Lane`].
//! - Lane types other than `driving`, so sidewalks, shoulders, and parking
//!   lanes are dropped.
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
mod parse;
mod route;

#[cfg(test)]
mod fixtures;

pub use coords::{Point, Vector};
pub use geometry::TooFewPoints;
pub use geometry::{Polyline, Pose, Projection, RoadSample};
pub use mesh::{LaneSpan, Mesh, MeshError, MeshSampler};
pub use network::{Direction, Lane, LaneId, LaneType, RoadNetwork};
pub use parse::{
    load_file, load_file_with_provenance, load_str, load_str_with_provenance, ImportError,
    LaneProvenance,
};
