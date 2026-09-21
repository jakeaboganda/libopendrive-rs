//! Hand-authored road networks used as test fixtures.
//!
//! These are built in code, not imported, so tests of the baked model
//! (sampling, tessellation, routing) don't depend on the parser. They are not
//! part of the public API: an OpenDRIVE crate's job is to import maps, not to
//! ship them.

mod banked_oval;
mod demo_road;

pub(crate) use banked_oval::banked_oval;
pub(crate) use demo_road::demo_road;
