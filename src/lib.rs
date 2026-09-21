#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(rustdoc::broken_intra_doc_links)]
//! A pure-Rust OpenDRIVE (`.xodr`) importer.
//!
//! OpenDRIVE describes roads analytically: clothoids, arcs, cubic elevation
//! and width profiles, lane links. This crate evaluates all of it once, at
//! load, and hands back a [`RoadNetwork`] of plain polylines. Nothing
//! downstream touches OpenDRIVE again -- consumers sample points, walk the
//! lane graph, and tessellate a surface mesh.
//!
//! No C++ dependency, no bindings, no `unsafe`.
//!
//! ```no_run
//! let net = libopendrive::load_file("maps/town07.xodr")?;
//! let lane = net.driving_lanes().next().expect("a driving lane");
//! let pose = lane.center.pose_at(25.0);
//! let route = net.route(pose.position, lane.center.point_at(400.0));
//! # Ok::<(), libopendrive::ImportError>(())
//! ```
//!
//! # What gets imported
//!
//! - Reference geometry: `line`, `arc`, `spiral` (clothoid), `paramPoly3`,
//!   `poly3`.
//! - `<elevationProfile>`, and `<lateralProfile>` **superelevation** baked as
//!   a real cant: the cross-section rolls about the reference line, so an
//!   outer lane rides higher and its surface normal leans.
//! - Per-lane widths, `laneOffset`, and multiple lane sections.
//! - Road/lane `<link>`s and `<junction>`s, resolved into a drive-direction
//!   lane graph -- "successor" means "a lane you can drive into off this
//!   lane's exit end", not a raw mirror of the file's `+s` links.
//!
//! Not yet: `<lateralProfile>` `<shape>` (per-`t` crowning and camber), and
//! lane types other than `driving`.
//!
//! # Coordinate frame
//!
//! Baked geometry is right-handed, **Y-up, metres**, with the ground in the
//! X-Z plane -- the frame most renderers and physics engines want.
//!
//! OpenDRIVE itself is right-handed **Z-up**: the reference line lies in the
//! X-Y plane, `hdg` is the heading within it, and elevation runs along +Z.
//! The importer maps `(x, y, elev)` to `(x, elev, -y)`, so an OpenDRIVE left
//! turn (increasing heading) curves toward -Z.
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
//! at parse -- Rust's float parser accepts `NaN` and turns `1e400` into
//! infinity, and one such value poisons every point derived from it.
//!
//! # Importer contract
//!
//! Code baking other map formats into a [`RoadNetwork`] must:
//!
//! - build lane geometry with [`Polyline::try_new`] and surface an error on
//!   degenerate input, rather than the panicking [`Polyline::new`];
//! - keep [`LaneId`]s opaque -- never assume one indexes the lane list;
//! - avoid lane curvature tighter than the half-width, or the
//!   [`RoadNetwork::surface_mesh`] ribs can self-intersect (the tessellator
//!   does not yet guard against it).

mod geometry;
mod grid;
mod mesh;
mod network;
mod parse;
mod route;

#[cfg(test)]
mod fixtures;

/// The `glam` version this crate's `Vec3`s come from. It is a public
/// dependency: match it, or your `Vec3` is a different type than ours.
pub use glam;

pub use geometry::TooFewPoints;
pub use geometry::{Polyline, Pose, Projection, RoadSample};
pub use mesh::{LaneSpan, Mesh, MeshError, MeshSampler};
pub use network::{Direction, Lane, LaneId, LaneKind, RoadNetwork};
pub use parse::{load_file, load_str, ImportError};
