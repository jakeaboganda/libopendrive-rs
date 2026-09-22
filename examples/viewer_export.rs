//! Bake an OpenDRIVE map and export it as the JSON the three.js viewer reads.
//!
//! ```sh
//! cargo run --example viewer_export --features serde -- \
//!     tests/data/testtrack.xodr viewer/web/scene.json
//! ```
//!
//! The output is one object: a merged surface mesh (flat position/normal/index
//! buffers, ready for a three.js `BufferGeometry`) and a per-lane table. Each
//! lane entry names the OpenDRIVE road, section, and lane id it came from, the
//! mesh slice it owns (so a picked triangle resolves to a lane), and its
//! centerline points (so the viewer can project the cursor to `(s, t)`).

use std::env;
use std::fs;
use std::process::ExitCode;

use libopendrive::{load_file_with_provenance, Direction, LaneProvenance, LaneSpan, Mesh, RoadNetwork};
use serde_json::{json, Map, Value};

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let (Some(input), output) = (args.next(), args.next()) else {
        eprintln!("usage: viewer_export <input.xodr> [output.json]");
        return ExitCode::FAILURE;
    };
    let output = output.unwrap_or_else(|| "viewer/web/scene.json".to_string());

    let (net, provenance) = match load_file_with_provenance(&input) {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("import failed: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mesh = net.surface_mesh();
    if let Err(e) = mesh.validate() {
        // A degenerate map still renders; warn but keep going.
        eprintln!("warning: mesh is not a valid trimesh: {e}");
    }

    let scene = build_scene(&net, &mesh, &provenance);
    let bytes = serde_json::to_vec(&scene).expect("scene serializes");
    if let Err(e) = fs::write(&output, &bytes) {
        eprintln!("writing {output}: {e}");
        return ExitCode::FAILURE;
    }

    eprintln!(
        "wrote {output}: {} lanes, {} vertices, {} triangles ({} KiB)",
        mesh.lanes.len(),
        mesh.vertices.len(),
        mesh.indices.len() / 3,
        bytes.len() / 1024,
    );
    ExitCode::SUCCESS
}

/// Assemble the viewer scene: bounds, flat mesh buffers, and the lane table.
fn build_scene(net: &RoadNetwork, mesh: &Mesh, provenance: &[LaneProvenance]) -> Value {
    // Flatten positions and normals into the [x,y,z, x,y,z, ...] layout a
    // three.js Float32BufferAttribute takes directly.
    let mut positions = Vec::with_capacity(mesh.vertices.len() * 3);
    for v in &mesh.vertices {
        positions.extend_from_slice(&v.to_array());
    }
    let mut normals = Vec::with_capacity(mesh.normals.len() * 3);
    for n in &mesh.normals {
        normals.extend_from_slice(&n.to_array());
    }

    let lanes: Vec<Value> = mesh
        .lanes
        .iter()
        .map(|span| lane_entry(net, provenance, span))
        .collect();

    json!({
        "meta": { "generator": "libopendrive viewer_export", "frame": "OpenDRIVE Z-up metres" },
        "mesh": { "positions": positions, "normals": normals, "indices": mesh.indices },
        "lanes": lanes,
    })
}

/// One lane's viewer record: identity, OpenDRIVE provenance, its mesh slice,
/// and its centerline for cursor projection.
fn lane_entry(net: &RoadNetwork, provenance: &[LaneProvenance], span: &LaneSpan) -> Value {
    let prov = provenance.iter().find(|p| p.lane == span.lane);
    let lane = net.lane(span.lane);

    let centerline: Vec<[f32; 3]> = lane
        .map(|l| l.center.points().iter().map(|p| p.to_array()).collect())
        .unwrap_or_default();

    let mut entry = Map::new();
    entry.insert("laneId".into(), json!(span.lane.0));
    entry.insert("roadId".into(), json!(prov.map(|p| p.road_id.as_str())));
    entry.insert("section".into(), json!(prov.map(|p| p.section)));
    entry.insert("odLaneId".into(), json!(prov.map(|p| p.od_id)));
    entry.insert(
        "direction".into(),
        json!(lane.map(|l| match l.direction {
            Direction::Forward => "forward",
            Direction::Backward => "backward",
        })),
    );
    entry.insert("width".into(), json!(lane.map(|l| l.width)));
    entry.insert("length".into(), json!(lane.map(|l| l.center.length())));
    entry.insert(
        "vertexRange".into(),
        json!([span.vertices.start, span.vertices.end]),
    );
    entry.insert(
        "indexRange".into(),
        json!([span.indices.start, span.indices.end]),
    );
    entry.insert("centerline".into(), json!(centerline));
    Value::Object(entry)
}
