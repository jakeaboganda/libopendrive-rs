//! Bake OpenDRIVE maps and export each as the JSON the three.js viewer reads.
//!
//! ```sh
//! cargo run -p libopendrive-viewer -- tests/data/*.xodr
//! cargo run -p libopendrive-viewer -- tests/data/testtrack.xodr /tmp/testtrack.json
//! ```
//!
//! Each map goes to `viewer/web/<map name>.json`, unless there is one map and
//! an output path after it. A map that fails to load is reported and the rest
//! still export. It then lists every scene in `viewer/web/` in
//! `scenes.json`, which the viewer's map picker reads. OpenCRG files are
//! read from beside the `.xodr`.

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use libopendrive::opencrg::CrgGrid;
use libopendrive_viewer::bake;

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
    let xodr = fs::read_to_string(input).map_err(|e| format!("reading {input}: {e}"))?;
    let dir = Path::new(input).parent().unwrap_or(Path::new("."));
    let scene = bake(&xodr, |file| {
        CrgGrid::from_path(dir.join(file)).map_err(|e| e.to_string())
    })?;
    for note in &scene.notes {
        eprintln!("warning: {input}: {note}");
    }

    let bytes = serde_json::to_vec(&scene.json).expect("scene serializes");
    fs::write(output, &bytes).map_err(|e| format!("writing {output}: {e}"))?;

    let count = |key: &str| scene.json[key].as_array().map_or(0, Vec::len);
    let mesh = &scene.json["mesh"];
    let length = |key: &str| mesh[key].as_array().map_or(0, Vec::len);
    let crg_files = scene.json["crg"]["surfaces"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| s["loaded"] == true)
        .map(|s| &s["file"])
        .collect::<HashSet<_>>()
        .len();
    eprintln!(
        "wrote {output}: {} lanes, {} objects, {} signals, {} road marks, {} vertices, {} triangles, {} CRG files ({} KiB)",
        count("lanes"),
        count("objects"),
        count("signals"),
        count("roadMarks"),
        length("positions") / 3,
        length("indices") / 3,
        crg_files,
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
