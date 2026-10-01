//! Write an OpenDRIVE map for an OpenCRG file: one road along the file's
//! reference line, with a lane on each side as wide as the grid, and a
//! `<CRG>` that lays the file on it.
//!
//! ```sh
//! cargo run --release --example crg_to_xodr -- target/crg/country_road.crg
//! ```
//!
//! The map is written beside the CRG, with the same name and `.xodr`. Pass
//! `--genuine` to lay the file in `genuine` mode instead of `attached`.
//!
//! The road follows the reference line in 0.25 m arcs. Each starts at the
//! file's position and heading there and turns to its heading at the next.
//! The elevation and superelevation are cubics every 0.25 m through the
//! file's reference line height and bank. So the road carries the file's
//! large-scale shape, and an `attached` CRG adds only its grid. On ASAM's
//! `country_road.crg` and `belgian_block.crg`, `RoadSurface` then agrees with
//! OpenCRG's own evaluation to 0.02 mm, and exactly in `genuine` mode.
//!
//! A file's `u` need not start at 0, so `sOffset` is minus its start.
//!
//! Files that close on themselves, such as a race track, fail to load. The
//! `opencrg` crate does not support `refline_continuation` yet.

use std::env;
use std::f64::consts::PI;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use libopendrive::opencrg::{CrgGrid, Uv};

/// How long each arc of the plan view is, and how far apart the elevation
/// and superelevation records are, in metres.
const STEP: f64 = 0.25;

/// The step of the central differences that give headings, in metres. Half
/// a cell of a 1 cm grid.
const DIFF: f64 = 0.005;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let genuine = args.iter().any(|a| a == "--genuine");
    let Some(input) = args.iter().find(|a| !a.starts_with("--")) else {
        eprintln!("usage: crg_to_xodr [--genuine] FILE.crg");
        return ExitCode::FAILURE;
    };
    let grid = match CrgGrid::from_path(input) {
        Ok(grid) => grid,
        Err(e) => {
            eprintln!("error: {input}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let file = Path::new(input);
    let output = file.with_extension("xodr");
    let name = file.file_name().and_then(|n| n.to_str()).unwrap_or(input);

    let xodr = map(&grid, name, genuine);
    if let Err(e) = fs::write(&output, xodr) {
        eprintln!("error: {}: {e}", output.display());
        return ExitCode::FAILURE;
    }
    let ((u0, u1), (v0, v1)) = (grid.u_range(), grid.v_range());
    eprintln!(
        "wrote {}: {:.1} m of road, {:.2} m wide",
        output.display(),
        u1 - u0,
        v1 - v0
    );
    ExitCode::SUCCESS
}

/// The `.xodr` text for `grid`, named `file` in its `<CRG>`.
fn map(grid: &CrgGrid, file: &str, genuine: bool) -> String {
    let file = escape(file);
    let ((u0, u1), (v0, v1)) = (grid.u_range(), grid.v_range());
    let length = u1 - u0;
    let knots: Vec<f64> = (0..=(length / STEP).ceil().max(1.0) as usize)
        .map(|i| (u0 + i as f64 * STEP).min(u1))
        .collect();
    let rate = |u: f64, f: &dyn Fn(f64) -> f64| {
        let (behind, ahead) = ((u - STEP).max(u0), (u + STEP).min(u1));
        (f(ahead) - f(behind)) / (ahead - behind)
    };

    let at = |u: f64, v: f64| grid.xy_from_uv(Uv { u, v });
    let heading = |u: f64| {
        let (behind, ahead) = (at((u - DIFF).max(u0), 0.0), at((u + DIFF).min(u1), 0.0));
        (ahead.y - behind.y).atan2(ahead.x - behind.x)
    };
    let mut plan = String::new();
    for w in knots.windows(2) {
        let (u, l) = (w[0], w[1] - w[0]);
        let (p, hdg) = (at(u, 0.0), heading(u));
        let curvature = wrap(heading(w[1]) - hdg) / l;
        let shape = if curvature.abs() < 1e-12 {
            "<line/>".to_string()
        } else {
            format!(r#"<arc curvature="{curvature:.12}"/>"#)
        };
        let _ = writeln!(
            plan,
            r#"      <geometry s="{:.6}" x="{:.6}" y="{:.6}" hdg="{hdg:.12}" length="{l:.6}">{shape}</geometry>"#,
            u - u0,
            p.x,
            p.y
        );
    }

    let base = |u: f64, v: f64| {
        let uv = Uv { u, v };
        let grid_z = grid.grid_at_uv(uv).map_or(0.0, |g| g.z);
        grid.elevation_at_uv(uv).unwrap_or(0.0) - grid_z
    };
    let half = 0.5 * (v1 - v0).min(1.0);
    let height = |u: f64| base(u, 0.0);
    let bank = |u: f64| ((base(u, half) - base(u, -half)) / (2.0 * half)).atan();
    let mut elevation = String::new();
    let mut superelevation = String::new();
    for w in knots.windows(2) {
        let (u, u1) = (w[0], w[1]);
        let cubic =
            |f: &dyn Fn(f64) -> f64| cubic(u - u0, u1 - u, f(u), f(u1), rate(u, f), rate(u1, f));
        let _ = writeln!(elevation, "      <elevation {}/>", cubic(&height));
        let _ = writeln!(superelevation, "      <superelevation {}/>", cubic(&bank));
    }

    let crg = if genuine {
        format!(r#"<CRG file="{file}" sStart="0" sEnd="{length:.6}" mode="genuine"/>"#)
    } else {
        format!(
            r#"<CRG file="{file}" sStart="0" sEnd="{length:.6}" mode="attached" sOffset="{:.6}"/>"#,
            -u0
        )
    };
    let lane = |id: i32, width: f64| {
        format!(
            r#"<lane id="{id}" type="driving"><width sOffset="0" a="{width:.6}" b="0" c="0" d="0"/></lane>"#
        )
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!-- Written by the crg_to_xodr example. -->
<OpenDRIVE>
  <header revMajor="1" revMinor="9"/>
  <road id="1" length="{length:.6}" junction="-1">
    <planView>
{plan}    </planView>
    <elevationProfile>
{elevation}    </elevationProfile>
    <lateralProfile>
{superelevation}    </lateralProfile>
    <lanes><laneSection s="0">
      <left>{}</left>
      <center><lane id="0" type="none"/></center>
      <right>{}</right>
    </laneSection></lanes>
    <surface>
      {crg}
    </surface>
  </road>
</OpenDRIVE>
"#,
        lane(1, v1),
        lane(-1, -v0),
    )
}

/// The attributes of the cubic record at `s` that runs `l` metres from `a`
/// to `a1`, starting with slope `m0` and ending with `m1`.
fn cubic(s: f64, l: f64, a: f64, a1: f64, m0: f64, m1: f64) -> String {
    let rise = (a1 - a) / l;
    let c = (3.0 * rise - 2.0 * m0 - m1) / l;
    let d = (m0 + m1 - 2.0 * rise) / (l * l);
    format!(r#"s="{s:.6}" a="{a:.9}" b="{m0:.9}" c="{c:.9}" d="{d:.9}""#)
}

/// `text` with the characters XML gives a meaning in an attribute escaped.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// An angle wrapped into `[-pi, pi)`.
fn wrap(a: f64) -> f64 {
    (a + PI).rem_euclid(2.0 * PI) - PI
}
