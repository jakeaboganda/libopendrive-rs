//! Drive a wheel down a lane of a map with OpenCRG surfaces and write what it
//! rolls over, as CSV, for a vehicle model to replay.
//!
//! ```sh
//! cargo run --release --example crg_profile -- target/crg/country_road.xodr > profile.csv
//! ```
//!
//! The wheel follows the first lane a CRG covers, or `--lane ID`, from its
//! start to its end in steps of `--step` metres, 0.01 by default.
//! `--offset T` moves it `T` metres left of the lane's centerline. It loads
//! the CRG files from beside the `.xodr` and carries one [`SurfaceHint`] down
//! the lane, as a wheel in a simulation would.
//!
//! Each row is `s, x, y, z, crg_height, friction, nx, ny, nz`. `z` and the
//! normal are the road surface. `crg_height` is the CRG grid's own height
//! there, and `friction` is the friction CRG's value, each empty where no
//! CRG gives one. A summary goes to stderr.

use std::env;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use libopendrive::opencrg::CrgGrid;
use libopendrive::{load_file, LaneId, RoadSurface, SurfaceHint};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let option = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse::<f64>().ok())
    };
    let Some(input) = args
        .iter()
        .enumerate()
        .find(|(i, a)| !a.starts_with("--") && (*i == 0 || !args[i - 1].starts_with("--")))
        .map(|(_, a)| a)
    else {
        eprintln!("usage: crg_profile FILE.xodr [--lane ID] [--offset T] [--step DS]");
        return ExitCode::FAILURE;
    };
    let step = option("--step").unwrap_or(0.01);
    let offset = option("--offset").unwrap_or(0.0);

    let net = match load_file(input) {
        Ok(net) => net,
        Err(e) => {
            eprintln!("error: {input}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mesh = net.surface_mesh();
    let dir = Path::new(input).parent().unwrap_or(Path::new("."));
    let surface = RoadSurface::new(&net, &mesh, |file| {
        CrgGrid::from_path(dir.join(file))
            .map_err(|e| eprintln!("warning: CRG {file}: {e}"))
            .ok()
    });

    let id = option("--lane").map(|id| LaneId(id as usize)).or_else(|| {
        net.crg_surfaces()
            .iter()
            .find_map(|c| c.lanes.first().copied())
    });
    let Some(lane) = id.and_then(|id| net.lane(id)) else {
        eprintln!("error: {input}: no such lane, or no CRG to pick one by");
        return ExitCode::FAILURE;
    };

    let mut out = BufWriter::new(io::stdout().lock());
    let _ = writeln!(out, "s,x,y,z,crg_height,friction,nx,ny,nz");
    let length = f64::from(lane.center.length());
    let steps = (length / step).floor() as usize;
    let mut wheel = SurfaceHint::default();
    let (mut rows, mut heights, mut frictions) = (0, Vec::new(), Vec::new());
    let started = Instant::now();
    for i in 0..=steps {
        let s = i as f64 * step;
        let pose = lane.center.pose_at(s as f32);
        let x = f64::from(pose.position.x) - offset * f64::from(pose.heading.y);
        let y = f64::from(pose.position.y) + offset * f64::from(pose.heading.x);
        let Some(ground) = surface.sample(x, y, &mut wheel) else {
            continue;
        };
        let cell = |v: Option<f64>| v.map_or(String::new(), |v| format!("{v:.6}"));
        let [nx, ny, nz] = ground.normal;
        let _ = writeln!(
            out,
            "{s:.3},{x:.4},{y:.4},{:.6},{},{},{nx:.6},{ny:.6},{nz:.6}",
            ground.z,
            cell(ground.crg_height),
            cell(ground.friction),
        );
        rows += 1;
        heights.extend(ground.crg_height);
        frictions.extend(ground.friction);
    }
    let elapsed = started.elapsed();
    let _ = out.flush();

    eprintln!(
        "lane {}: {length:.1} m, {rows} samples at {step} m, {:.2} us each",
        lane.id.0,
        elapsed.as_secs_f64() * 1e6 / rows.max(1) as f64
    );
    if !heights.is_empty() {
        let rms = (heights.iter().map(|h| h * h).sum::<f64>() / heights.len() as f64).sqrt();
        let (lo, hi) = heights
            .iter()
            .fold((f64::MAX, f64::MIN), |(lo, hi), &h| (lo.min(h), hi.max(h)));
        eprintln!(
            "  CRG height on {} samples: {:.1} to {:.1} mm, RMS {:.2} mm",
            heights.len(),
            lo * 1e3,
            hi * 1e3,
            rms * 1e3
        );
    }
    if !frictions.is_empty() {
        let (lo, hi) = frictions
            .iter()
            .fold((f64::MAX, f64::MIN), |(lo, hi), &f| (lo.min(f), hi.max(f)));
        eprintln!(
            "  friction on {} samples: {lo:.3} to {hi:.3}",
            frictions.len()
        );
    }
    ExitCode::SUCCESS
}
