//! Bake OpenDRIVE maps and export each as the JSON the three.js viewer reads.
//!
//! ```sh
//! cargo run --example viewer_export --features serde -- tests/data/*.xodr
//! cargo run --example viewer_export --features serde -- \
//!     tests/data/testtrack.xodr /tmp/testtrack.json
//! ```
//!
//! Each map goes to `viewer/web/<map name>.json`, unless there is one map and
//! an output path after it. A map that fails to load is reported and the rest
//! still export. It then lists every scene in `viewer/web/` in
//! `scenes.json`, which the viewer's map picker reads.
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
//! with each object's slice of it. A structure table lists the tunnels and
//! bridges, each with the stretch of every lane it covers. A signal table
//! gives each signal's meaning, the lanes it applies to, and its board's
//! pose and size. A controller table lists the signals each controller
//! switches together.
//!
//! Where the map lays OpenCRG files on its roads, the exporter loads them from
//! beside the `.xodr` and samples [`RoadSurface`] over every lane they cover:
//! a grid draped on the road mesh, with the CRG height, the surface height and
//! the friction at each vertex.
//!
//! Lane boundaries are not exported. They are already in the mesh: a lane's
//! vertex range alternates left and right rib, which is what the viewer draws
//! them from. See [`LaneSpan`].

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use libopendrive::opencrg::CrgGrid;
use libopendrive::{
    load_file_with_provenance, Controller, ControllerProvenance, Corner, CrgMode, CrgPurpose,
    CrgSurface, Direction, Extent, LaneId, LaneProvenance, LaneSpan, Marking, Mesh, Object,
    ObjectProvenance, Orientation, Point, Provenance, Referenced, RoadNetwork, RoadSurface, Shape,
    Signal, SignalProvenance, Structure, StructureKind, StructureProvenance, SurfaceHint,
};
use serde_json::{json, Map, Value};

/// Where scenes go without an output path, and the folder the viewer serves.
const SCENES: &str = "viewer/web";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let jobs: Vec<(String, String)> = match args.as_slice() {
        [input, output] if output.ends_with(".json") => vec![(input.clone(), output.clone())],
        inputs if !inputs.is_empty() && !inputs.iter().any(|a| a.ends_with(".json")) => inputs
            .iter()
            .map(|input| {
                let stem = Path::new(input).file_stem().unwrap_or_default();
                let output = format!("{SCENES}/{}.json", stem.to_string_lossy());
                (input.clone(), output)
            })
            .collect(),
        _ => {
            eprintln!("usage: viewer_export <input.xodr> [output.json]");
            eprintln!("       viewer_export <input.xodr>...");
            return ExitCode::FAILURE;
        }
    };

    let mut failed = 0;
    let mut written: HashMap<&str, &str> = HashMap::new();
    for (input, output) in &jobs {
        if let Some(first) = written.get(output.as_str()) {
            eprintln!("skipping {input}: {first} already wrote {output}");
            failed += 1;
            continue;
        }
        match export(input, output) {
            Ok(()) => {
                written.insert(output, input);
            }
            Err(e) => {
                eprintln!("{input}: {e}");
                failed += 1;
            }
        }
    }

    let scenes = fs::canonicalize(SCENES).ok();
    let in_scenes = |output: &&str| {
        let folder = Path::new(output)
            .parent()
            .filter(|p| !p.as_os_str().is_empty());
        fs::canonicalize(folder.unwrap_or(Path::new("."))).ok() == scenes
    };
    if scenes.is_some() && written.keys().any(in_scenes) {
        if let Err(e) = write_scene_list(Path::new(SCENES)) {
            eprintln!("warning: listing scenes in {SCENES}: {e}");
        }
    }
    if failed > 0 {
        eprintln!("{failed} of {} maps not exported", jobs.len());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Bake one map and write its viewer scene to `output`.
fn export(input: &str, output: &str) -> Result<(), String> {
    let (net, provenance) =
        load_file_with_provenance(input).map_err(|e| format!("import failed: {e}"))?;

    let mesh = net.surface_mesh();
    if let Err(e) = mesh.validate() {
        // A degenerate map still renders; warn but keep going.
        eprintln!("warning: {input}: mesh is not a valid trimesh: {e}");
    }

    let object_mesh = net.object_mesh();
    if !object_mesh.objects.is_empty() {
        if let Err(e) = object_mesh.validate() {
            eprintln!("warning: {input}: object mesh is not a valid trimesh: {e}");
        }
    }

    let dir = Path::new(input).parent().unwrap_or(Path::new("."));
    let mut loaded: HashMap<String, f64> = HashMap::new();
    let surface = RoadSurface::new(&net, &mesh, |file| {
        match CrgGrid::from_path(dir.join(file)) {
            Ok(grid) => {
                loaded.insert(file.to_string(), spacing(&grid));
                Some(grid)
            }
            Err(e) => {
                eprintln!("warning: CRG {file}: {e}");
                None
            }
        }
    });

    let mut scene = build_scene(&net, &mesh, &object_mesh, &provenance);
    scene["crg"] = crg_overlay(&net, &mesh, &surface, &loaded);
    let bytes = serde_json::to_vec(&scene).expect("scene serializes");
    fs::write(output, &bytes).map_err(|e| format!("writing {output}: {e}"))?;

    eprintln!(
        "wrote {output}: {} lanes, {} objects, {} signals, {} vertices, {} triangles, {} CRG files ({} KiB)",
        mesh.lanes.len(),
        net.objects().len(),
        net.signals().len(),
        mesh.vertices.len(),
        mesh.indices.len() / 3,
        loaded.len(),
        bytes.len() / 1024,
    );
    Ok(())
}

/// Write `scenes.json` in `folder`: the sorted names of the other `.json`
/// files there.
fn write_scene_list(folder: &Path) -> std::io::Result<()> {
    let mut names = Vec::new();
    for entry in fs::read_dir(folder)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if name.ends_with(".json") && name != "scenes.json" {
            names.push(name);
        }
    }
    names.sort_by_key(|n| n.to_lowercase());
    fs::write(
        folder.join("scenes.json"),
        serde_json::to_vec(&names).expect("names serialize"),
    )
}

/// Assemble the viewer scene: flat mesh buffers, the lane table, the object
/// table, the object mesh, and the structure, signal and controller tables.
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

    let structures: Vec<Value> = net
        .structures()
        .iter()
        .map(|s| {
            structure_entry(
                s,
                provenance.structures.iter().find(|p| p.structure == s.id),
            )
        })
        .collect();

    let signals: Vec<Value> = net
        .signals()
        .iter()
        .map(|s| signal_entry(s, provenance.signals.iter().find(|p| p.signal == s.id)))
        .collect();

    let controllers: Vec<Value> = net
        .controllers()
        .iter()
        .map(|c| {
            controller_entry(
                c,
                provenance.controllers.iter().find(|p| p.controller == c.id),
            )
        })
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
        "structures": structures,
        "signals": signals,
        "controllers": controllers,
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
/// object is one. `materials` lists what its surface is made of, and
/// `userData` its vendor data. `referencedFrom` is the road of the `<object>` an
/// `<objectReference>` placed again, and null otherwise. A `solid`
/// carries a pose, angles in radians applied yaw, then pitch, then roll, and
/// an `extent` that is null for an object the map gives no size. An
/// `outline` and a `sweep` carry corners already in world coordinates, each
/// a `[base, top]` pair of points. An `outline`'s `holes` are rings of them.
/// A `round` sweep is the ellipse inscribed in each section.
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
        Shape::Outline {
            corners,
            closed,
            holes,
        } => json!({
            "kind": "outline",
            "corners": corners.iter().map(corner).collect::<Vec<_>>(),
            "closed": closed,
            "holes": holes
                .iter()
                .map(|h| h.iter().map(corner).collect::<Vec<_>>())
                .collect::<Vec<_>>(),
        }),
        Shape::Sweep { sections, round } => json!({
            "kind": "sweep",
            "round": round,
            "sections": sections
                .iter()
                .map(|s| json!({ "left": corner(&s.left), "right": corner(&s.right) }))
                .collect::<Vec<_>>(),
        }),
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
        "userData": object
            .user_data
            .iter()
            .map(|u| json!({ "code": u.code, "value": u.value }))
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

/// One signal's viewer record: what it means, its OpenDRIVE provenance, the
/// lanes it applies to, and its board. `appliesAt` is the points on the road
/// where it takes effect, its own and then one per entry in
/// `signalReferences`, the `<signalReference>`s that apply it on other
/// roads. `dependencies` and `references` are its links to other signals and
/// objects, by `signalId` and `objectId`. The board's `position` is the middle of its bottom
/// edge, and its angles are in radians, applied yaw, then pitch, then roll.
/// `length`, `width` and `height` are null where the map gives none.
fn signal_entry(s: &Signal, prov: Option<&SignalProvenance>) -> Value {
    json!({
        "signalId": s.id.0,
        "name": s.name,
        "dynamic": s.dynamic,
        "country": s.country,
        "countryRevision": s.country_revision,
        "type": s.kind,
        "subtype": s.subtype,
        "value": s.value,
        "unit": s.unit.map(|u| u.as_str()),
        "text": s.text,
        "invalidated": s.invalidated,
        "temporary": s.temporary,
        "controllers": s.controllers.iter().map(|c| c.0).collect::<Vec<_>>(),
        "dependencies": s
            .dependencies
            .iter()
            .map(|d| json!({ "signalId": d.signal.0, "type": d.kind }))
            .collect::<Vec<_>>(),
        "references": s
            .references
            .iter()
            .map(|r| match r.to {
                Referenced::Signal(id) => json!({ "signalId": id.0, "type": r.kind }),
                Referenced::Object(id) => json!({ "objectId": id.0, "type": r.kind }),
            })
            .collect::<Vec<_>>(),
        "lanes": s.lanes.iter().map(|l| l.0).collect::<Vec<_>>(),
        "appliesAt": s.applies_at.iter().map(|p| p.to_array()).collect::<Vec<_>>(),
        "position": s.position.to_array(),
        "heading": s.heading,
        "pitch": s.pitch,
        "roll": s.roll,
        "length": s.length,
        "width": s.width,
        "height": s.height,
        "roadId": prov.map(|p| p.road_id.as_str()),
        "odId": prov.map(|p| p.od_id.as_str()),
        "s": prov.map(|p| p.s),
        "t": prov.map(|p| p.t),
        "orientation": prov.map(|p| orientation(p.orientation)),
        "signalReferences": prov
            .map(|p| p.references.as_slice())
            .unwrap_or_default()
            .iter()
            .map(|r| json!({
                "roadId": r.road_id,
                "s": r.s,
                "t": r.t,
                "orientation": orientation(r.orientation),
            }))
            .collect::<Vec<_>>(),
    })
}

/// One controller's viewer record: its name, the signals it controls, and
/// the junctions that sync it.
fn controller_entry(c: &Controller, prov: Option<&ControllerProvenance>) -> Value {
    json!({
        "controllerId": c.id.0,
        "name": c.name,
        "sequence": c.sequence,
        "signals": c
            .signals
            .iter()
            .map(|s| json!({ "signalId": s.signal.0, "type": s.kind }))
            .collect::<Vec<_>>(),
        "odId": prov.map(|p| p.od_id.as_str()),
        "junctions": prov
            .map(|p| p.junctions.as_slice())
            .unwrap_or_default()
            .iter()
            .map(|j| json!({ "junctionId": j.junction_id, "type": j.kind, "sequence": j.sequence }))
            .collect::<Vec<_>>(),
    })
}

/// How the file spells an orientation.
fn orientation(o: Orientation) -> &'static str {
    match o {
        Orientation::Positive => "+",
        Orientation::Negative => "-",
        Orientation::Both => "none",
    }
}

/// One tunnel's or bridge's viewer record: what it is, its OpenDRIVE
/// provenance, and the part of each lane it covers, in metres along the
/// lane's centerline.
fn structure_entry(s: &Structure, prov: Option<&StructureProvenance>) -> Value {
    let (kind, type_, lighting, daylight) = match &s.kind {
        StructureKind::Tunnel {
            kind,
            lighting,
            daylight,
        } => ("tunnel", kind, *lighting, *daylight),
        StructureKind::Bridge { kind } => ("bridge", kind, None, None),
    };
    json!({
        "structureId": s.id.0,
        "kind": kind,
        "name": s.name,
        "type": type_,
        "lighting": lighting,
        "daylight": daylight,
        "roadId": prov.map(|p| p.road_id.as_str()),
        "odId": prov.map(|p| p.od_id.as_str()),
        "s": prov.map(|p| p.s),
        "length": prov.map(|p| p.length),
        "lanes": s
            .lanes
            .iter()
            .map(|c| json!({ "laneId": c.lane.0, "from": c.from, "to": c.to }))
            .collect::<Vec<_>>(),
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

/// How many grid cells each loaded CRG file may add to the overlay.
const CELLS_PER_FILE: f64 = 250_000.0;

/// How far above the road mesh the overlay hangs, in metres, so the road
/// never hides it.
const LIFT: f32 = 0.005;

/// The finest spacing of a CRG file's grid, in metres.
fn spacing(grid: &CrgGrid) -> f64 {
    let (nu, nv) = grid.dims();
    let ((u0, u1), (v0, v1)) = (grid.u_range(), grid.v_range());
    let du = (u1 - u0) / (nu.max(2) - 1) as f64;
    let dv = (v1 - v0) / (nv.max(2) - 1) as f64;
    du.min(dv)
}

/// The CRG overlay: [`RoadSurface`] sampled over every lane a CRG covers, on
/// a grid draped `LIFT` above the road mesh. Each vertex carries the CRG
/// grid height (`heights`), the surface height (`z`) and the friction, null
/// where none applies. A cell is drawn only where all four corners have one
/// of them. The grid is as fine as the finest loaded file, or coarser to stay
/// within [`CELLS_PER_FILE`] per file. The colour scale runs `range` either
/// side of `center`: the median CRG height, and the 99.9th percentile of the
/// distance from it. A file of absolute heights and a flat road with a few
/// obstacles both get a scale that shows them.
fn crg_overlay(
    net: &RoadNetwork,
    mesh: &Mesh,
    surface: &RoadSurface,
    loaded: &HashMap<String, f64>,
) -> Value {
    let surfaces: Vec<Value> = net
        .crg_surfaces()
        .iter()
        .map(|c| crg_entry(c, loaded.contains_key(&c.file)))
        .collect();
    let covered: HashSet<LaneId> = net
        .crg_surfaces()
        .iter()
        .filter(|c| loaded.contains_key(&c.file))
        .flat_map(|c| c.lanes.iter().copied())
        .collect();
    let spans: Vec<&LaneSpan> = mesh
        .lanes
        .iter()
        .filter(|s| covered.contains(&s.lane))
        .collect();

    let ribs = |span: &LaneSpan| -> Vec<(Point, Point)> {
        let v = &mesh.vertices[span.vertices.start as usize..span.vertices.end as usize];
        v.chunks_exact(2).map(|p| (p[0], p[1])).collect()
    };
    let area: f64 = spans
        .iter()
        .map(|span| {
            ribs(span)
                .windows(2)
                .map(|w| {
                    let along = ((w[1].0 - w[0].0).length() + (w[1].1 - w[0].1).length()) / 2.0;
                    let across = ((w[0].0 - w[0].1).length() + (w[1].0 - w[1].1).length()) / 2.0;
                    f64::from(along * across)
                })
                .sum::<f64>()
        })
        .sum();
    let finest = loaded.values().copied().fold(f64::INFINITY, f64::min);
    let cell = finest.max((area / (CELLS_PER_FILE * loaded.len() as f64)).sqrt()) as f32;

    let (mut positions, mut heights, mut z, mut friction) = (vec![], vec![], vec![], vec![]);
    let mut indices: Vec<u32> = vec![];
    for span in &spans {
        let ribs = ribs(span);
        let widest = ribs
            .iter()
            .map(|(l, r)| (*l - *r).length())
            .fold(0.0, f32::max);
        let columns = (widest / cell).ceil().max(1.0) as usize;
        let mut rows: Vec<(Point, Point)> = vec![];
        for w in ribs.windows(2) {
            let along = ((w[1].0 - w[0].0).length()).max((w[1].1 - w[0].1).length());
            let steps = (along / cell).ceil().max(1.0) as usize;
            for i in 0..steps {
                let f = i as f32 / steps as f32;
                rows.push((lerp(w[0].0, w[1].0, f), lerp(w[0].1, w[1].1, f)));
            }
        }
        rows.extend(ribs.last().copied());

        let mut hints = vec![SurfaceHint::default(); columns + 1];
        let mut previous: Vec<Option<u32>> = vec![];
        for (left, right) in rows {
            let row: Vec<Option<u32>> = (0..=columns)
                .map(|j| {
                    let p = lerp(left, right, j as f32 / columns as f32);
                    let sample = surface.sample(f64::from(p.x), f64::from(p.y), &mut hints[j])?;
                    if sample.crg_height.is_none() && sample.friction.is_none() {
                        return None;
                    }
                    positions.extend([p.x, p.y, p.z + LIFT]);
                    heights.push(sample.crg_height.map(|h| h as f32));
                    z.push(sample.z as f32);
                    friction.push(sample.friction.map(|f| f as f32));
                    Some((heights.len() - 1) as u32)
                })
                .collect();
            if !previous.is_empty() {
                for j in 0..columns {
                    let quad = [previous[j], previous[j + 1], row[j + 1], row[j]];
                    if let [Some(a), Some(b), Some(c), Some(d)] = quad {
                        indices.extend([a, b, c, a, c, d]);
                    }
                }
            }
            previous = row;
        }
    }

    let percentile = |mut values: Vec<f32>, p: f64| {
        values.sort_by(f32::total_cmp);
        let at = ((values.len() as f64 * p) as usize).min(values.len().saturating_sub(1));
        (values.get(at).copied(), values.last().copied())
    };
    let center = percentile(heights.iter().flatten().copied().collect(), 0.5)
        .0
        .unwrap_or(0.0);
    let (tail, most) = percentile(
        heights
            .iter()
            .flatten()
            .map(|h| (h - center).abs())
            .collect(),
        0.999,
    );
    let range = [tail, most]
        .into_iter()
        .flatten()
        .find(|r| *r > 0.0)
        .unwrap_or(0.001);

    json!({
        "surfaces": surfaces,
        "cell": cell,
        "center": center,
        "range": range,
        "positions": positions,
        "heights": heights,
        "z": z,
        "friction": friction,
        "indices": indices,
    })
}

/// One `<CRG>` record: its file, mode, purpose and the lanes it covers, and
/// whether its file loaded.
fn crg_entry(c: &CrgSurface, loaded: bool) -> Value {
    let mode = match c.mode {
        CrgMode::Attached(_) => "attached",
        CrgMode::Attached0(_) => "attached0",
        CrgMode::Genuine { .. } => "genuine",
        CrgMode::Global { .. } => "global",
    };
    json!({
        "file": c.file,
        "mode": mode,
        "purpose": match c.purpose {
            CrgPurpose::Elevation => "elevation",
            CrgPurpose::Friction => "friction",
        },
        "zOffset": c.z_offset,
        "zScale": c.z_scale,
        "lanes": c.lanes.iter().map(|l| l.0).collect::<Vec<_>>(),
        "loaded": loaded,
    })
}

fn lerp(a: Point, b: Point, f: f32) -> Point {
    a + (b - a) * f
}
