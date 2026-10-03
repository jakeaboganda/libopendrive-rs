//! `libopendrive` is now [`xodr`](https://docs.rs/xodr).
//!
//! The crate was renamed because `libopendrive` is easily confused with
//! libOpenDRIVE, a C++ library. This last release re-exports everything
//! from `xodr` 0.5, so existing code keeps compiling. To move over, replace
//! `libopendrive` with `xodr` in `Cargo.toml` and in your `use` paths.
//! Nothing else changes. New releases come out as `xodr` only.

pub use xodr::*;
