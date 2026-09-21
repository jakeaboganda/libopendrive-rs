//! Import cost: parsing and baking a whole `.xodr` into a `RoadNetwork`.
//!
//! Loading is a one-off, but it is a *startup* one-off -- it sits between the
//! user asking for a map and the scene existing.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use libopendrive::load_str;

const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/");

fn bench_import(c: &mut Criterion) {
    let mut group = c.benchmark_group("import");
    // Read once: the benchmark measures parse + bake, not the filesystem.
    for name in ["testtrack", "e6mini", "town07"] {
        let xml = std::fs::read_to_string(format!("{DATA}{name}.xodr"))
            .unwrap_or_else(|e| panic!("reading {name}.xodr: {e}"));
        group.bench_function(name, |b| b.iter(|| load_str(black_box(&xml)).unwrap()));
    }
    group.finish();
}

criterion_group!(benches, bench_import);
criterion_main!(benches);
