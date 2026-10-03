# libopendrive is now xodr

This crate is renamed to [`xodr`](https://crates.io/crates/xodr). The old
name was easily confused with libOpenDRIVE, a C++ library.

This last release, 0.4.2, re-exports everything from `xodr` 0.5, so
existing code keeps compiling. To move over, replace `libopendrive` with
`xodr` in `Cargo.toml` and in your `use` paths:

```toml
xodr = "0.5"
```

New releases come out as `xodr` only.
