//! A pure-Rust OpenDRIVE (`.xodr`) importer.
//!
//! This commit carries the destination: the baked road-network model every
//! importer writes into and every consumer reads. The parser follows.
//!
//! # The baked model
//!
//! OpenDRIVE describes roads analytically -- clothoids, arcs, cubic elevation
//! and width profiles. All of that is evaluated once, at load, into plain
//! polylines. Consumers only ever sample points, walk the lane graph, or
//! tessellate the surface.
//!
//! - [`RoadNetwork`] is the compiled map: a lane list plus a drive-direction
//!   lane graph, with `nearest_lane`, `sample_near`, `route`, and
//!   `surface_mesh` over it.
//! - [`Lane`] is one drivable strip: a centerline [`Polyline`], a width, a
//!   superelevation profile, and its graph edges.
//! - [`Mesh`] is the tessellated road surface -- positions, normals, indices.
//!   Renderer-agnostic on purpose.
//!
//! # Coordinate frame
//!
//! Baked geometry is right-handed, **Y-up, metres**, with the ground in the
//! X-Z plane -- the frame most renderers and physics engines want. OpenDRIVE
//! itself is right-handed **Z-up**; the mapping happens at import.
//!
//! # Importer contract
//!
//! Code baking external (possibly malformed) map data into a [`RoadNetwork`]
//! must:
//!
//! - build lane geometry with [`Polyline::try_new`] and surface an error on
//!   degenerate input, rather than the panicking [`Polyline::new`];
//! - keep [`LaneId`]s opaque -- never assume one indexes the lane list;
//! - avoid lane curvature tighter than the half-width, or the
//!   `surface_mesh` ribs can self-intersect (the tessellator does not yet
//!   guard against it).

mod geometry;
mod mesh;
mod network;
mod route;

#[cfg(test)]
mod fixtures;

/// The `glam` version this crate's `Vec3`s come from. It is a public
/// dependency: match it, or your `Vec3` is a different type than ours.
pub use glam;

pub use geometry::{Polyline, Pose, Projection, RoadSample};
pub use mesh::{Mesh, MeshError};
pub use network::{Direction, Lane, LaneId, LaneKind, RoadNetwork};
