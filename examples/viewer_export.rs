//! Bake an OpenDRIVE map and export it as the JSON the three.js viewer reads.
//!
//! ```sh
//! cargo run --example viewer_export --features serde -- \
//!     tests/data/testtrack.xodr viewer/web/scene.json
//! ```
//!
//! The output is one object: a merged surface mesh (flat position/normal/index
//! buffers, ready for a three.js `BufferGeometry`), a per-lane table, and an
//! object table. Each
//! lane entry names the OpenDRIVE road, section, and lane id it came from, what
//! the lane is for, the mesh slice it owns (so a picked triangle resolves to a
//! lane), and its centerline with the heading at each point (so the viewer can
//! project the cursor to `(s, t)` and read out the same heading the crate
//! would). Each object entry is its type, name, and shape: a pose and extent
//! for a solid, or world-space corners for an outline or a sweep. The object
//! mesh from [`RoadNetwork::object_mesh`] comes too, in the same flat buffers
//! with each object's slice of it.
//!
//! Lane boundaries are not exported. They are already in the mesh: a lane's
//! vertex range alternates left and right rib, which is what the viewer draws
//! them from. See [`LaneSpan`].

use std::env;
use std::fs;
use std::process::ExitCode;

use libopendrive::{
    load_file_with_provenance, Corner, Direction, Extent, LaneProvenance, LaneSpan, Marking, Mesh,
    Object, ObjectProvenance, Orientation, Point, Provenance, RoadNetwork, Shape,
};
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

    let object_mesh = net.object_mesh();
    if !object_mesh.objects.is_empty() {
        if let Err(e) = object_mesh.validate() {
            eprintln!("warning: object mesh is not a valid trimesh: {e}");
        }
    }

    let scene = build_scene(&net, &mesh, &object_mesh, &provenance);
    let bytes = serde_json::to_vec(&scene).expect("scene serializes");
    if let Err(e) = fs::write(&output, &bytes) {
        eprintln!("writing {output}: {e}");
        return ExitCode::FAILURE;
    }

    eprintln!(
        "wrote {output}: {} lanes, {} objects, {} vertices, {} triangles ({} KiB)",
        mesh.lanes.len(),
        net.objects().len(),
        mesh.vertices.len(),
        mesh.indices.len() / 3,
        bytes.len() / 1024,
    );
    ExitCode::SUCCESS
}

/// Assemble the viewer scene: flat mesh buffers, the lane table, the object
/// table, and the object mesh.
fn build_scene(
    net: &RoadNetwork,
    mesh: &Mesh,
    object_mesh: &Mesh,
    provenance: &Provenance,
) -> Value {
    let lanes: Vec<Value> = mesh
        .lanes
        .iter()
        .map(|span| lane_entry(net, &provenance.lanes, span))
        .collect();
    let objects: Vec<Value> = net
        .objects()
        .iter()
        .map(|o| object_entry(o, provenance.objects.iter().find(|p| p.object == o.id)))
        .collect();

    let mut object_buffers = buffers(object_mesh);
    object_buffers["spans"] = object_mesh
        .objects
        .iter()
        .map(|s| {
            json!({
                "objectId": s.object.0,
                "vertices": [s.vertices.start, s.vertices.end],
                "indices": [s.indices.start, s.indices.end],
            })
        })
        .collect();

    json!({
        "meta": { "generator": "libopendrive viewer_export", "frame": "OpenDRIVE Z-up metres" },
        "mesh": buffers(mesh),
        "lanes": lanes,
        "objects": objects,
        "objectMesh": object_buffers,
    })
}

/// A mesh's positions, normals and indices, flattened into the
/// [x,y,z, x,y,z, ...] layout a three.js Float32BufferAttribute takes
/// directly.
fn buffers(mesh: &Mesh) -> Value {
    let positions: Vec<f32> = mesh.vertices.iter().flat_map(|v| v.to_array()).collect();
    let normals: Vec<f32> = mesh.normals.iter().flat_map(|n| n.to_array()).collect();
    json!({ "positions": positions, "normals": normals, "indices": mesh.indices })
}

/// One object's viewer record: its identity, OpenDRIVE provenance, and
/// shape. `lanes` is the `LaneId`s it applies to, `markings` its paint, and
/// `borders` the bands along its edges. `parkingSpace` is null unless the
/// object is one. `materials` lists what its surface is made of. `referencedFrom` is the road of the `<object>` an
/// `<objectReference>` placed again, and null otherwise. A `solid`
/// carries a pose, angles in radians applied yaw, then pitch, then roll, and
/// an `extent` that is null for an object the map gives no size. An
/// `outline` and a `sweep` carry corners already in world coordinates, each
/// a `[base, top]` pair of points.
fn object_entry(object: &Object, prov: Option<&ObjectProvenance>) -> Value {
    let corner = |c: &Corner| json!([c.base.to_array(), c.top.to_array()]);
    let shape = match &object.shape {
        Shape::Solid {
            position,
            heading,
            pitch,
            roll,
            extent,
        } => {
            let extent = match extent {
                Some(Extent::Box {
                    length,
                    width,
                    height,
                }) => json!({ "shape": "box", "length": length, "width": width, "height": height }),
                Some(Extent::Cylinder { radius, height }) => {
                    json!({ "shape": "cylinder", "radius": radius, "height": height })
                }
                None => Value::Null,
            };
            json!({
                "kind": "solid",
                "position": position.to_array(),
                "heading": heading,
                "pitch": pitch,
                "roll": roll,
                "extent": extent,
            })
        }
        Shape::Outline { corners, closed } => json!({
            "kind": "outline",
            "corners": corners.iter().map(corner).collect::<Vec<_>>(),
            "closed": closed,
        }),
        Shape::Sweep { sections } => json!({
            "kind": "sweep",
            "sections": sections
                .iter()
                .map(|s| json!({ "left": corner(&s.left), "right": corner(&s.right) }))
                .collect::<Vec<_>>(),
        }),
    };
    let orientation = |o: Orientation| match o {
        Orientation::Positive => "+",
        Orientation::Negative => "-",
        Orientation::Both => "none",
    };
    json!({
        "objectId": object.id.0,
        "objectType": object.kind.as_str(),
        "subtype": object.subtype,
        "name": object.name,
        "dynamic": object.dynamic,
        "lanes": object.lanes.iter().map(|l| l.0).collect::<Vec<_>>(),
        "markings": object.markings.iter().map(marking_entry).collect::<Vec<_>>(),
        "parkingSpace": object
            .parking_space
            .as_ref()
            .map(|p| json!({ "access": p.access, "restrictions": p.restrictions })),
        "materials": object
            .materials
            .iter()
            .map(|m| json!({ "surface": m.surface, "friction": m.friction, "roughness": m.roughness }))
            .collect::<Vec<_>>(),
        "borders": object
            .borders
            .iter()
            .map(|b| json!({ "type": b.kind, "width": b.width, "pieces": pieces(&b.pieces) }))
            .collect::<Vec<_>>(),
        "roadId": prov.map(|p| p.road_id.as_str()),
        "odId": prov.map(|p| p.od_id.as_str()),
        "s": prov.map(|p| p.s),
        "t": prov.map(|p| p.t),
        "orientation": prov.map(|p| orientation(p.orientation)),
        "validLength": prov.and_then(|p| p.valid_length),
        "referencedFrom": prov.and_then(|p| p.referenced_from.as_deref()),
        "shape": shape,
    })
}

/// One marking's viewer record: its attributes, and its pieces, each four
/// world-space points.
fn marking_entry(m: &Marking) -> Value {
    json!({
        "side": m.side,
        "color": m.color,
        "width": m.width,
        "lineLength": m.line_length,
        "spaceLength": m.space_length,
        "pieces": pieces(&m.pieces),
    })
}

/// Quads of world-space points, as nested arrays.
fn pieces(quads: &[[Point; 4]]) -> Vec<[[f32; 3]; 4]> {
    quads.iter().map(|q| q.map(|p| p.to_array())).collect()
}

/// One lane's viewer record: identity, OpenDRIVE provenance, its mesh slice,
/// and its centerline for cursor projection.
fn lane_entry(net: &RoadNetwork, provenance: &[LaneProvenance], span: &LaneSpan) -> Value {
    let prov = provenance.iter().find(|p| p.lane == span.lane);
    let lane = net.lane(span.lane);

    let centerline: Vec<[f32; 3]> = lane
        .map(|l| l.center.points().iter().map(|p| p.to_array()).collect())
        .unwrap_or_default();
    // Parallel to `centerline`. The viewer interpolates these the way
    // `Polyline::pose_at` does, rather than re-deriving a heading from the
    // chords and disagreeing with the crate at every vertex.
    let headings: Vec<[f32; 3]> = lane
        .map(|l| l.center.tangents().iter().map(|t| t.to_array()).collect())
        .unwrap_or_default();

    let mut entry = Map::new();
    entry.insert("laneId".into(), json!(span.lane.0));
    entry.insert("roadId".into(), json!(prov.map(|p| p.road_id.as_str())));
    entry.insert("section".into(), json!(prov.map(|p| p.section)));
    entry.insert("odLaneId".into(), json!(prov.map(|p| p.od_id)));
    entry.insert("laneType".into(), json!(lane.map(|l| l.kind.as_str())));
    entry.insert("drivable".into(), json!(lane.map(|l| l.kind.is_drivable())));
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
    entry.insert("headings".into(), json!(headings));
    Value::Object(entry)
}
