#!/bin/sh
# Download measured and test-course OpenCRG files, write a map for each with
# the crg_to_xodr example, and export each map for the viewer.
#
#     sh examples/crg_data.sh [DIR]
#
# Run it from any folder. The files go to DIR, target/crg in the repository
# by default, and the viewer scenes to viewer/web/NAME.json. country_road.crg
# is 85 MB, and the rest total 2 MB. A download that stops part way is
# fetched again on the next run.
#
# ASAM OpenCRG, Apache License 2.0, https://github.com/asam-ev/OpenCRG
#   country_road      569 m of a scanned country road, 1 cm grid
#   belgian_block     10 m of scanned cobbles, 1 cm grid
# Project Chrono, BSD 3-Clause, https://github.com/projectchrono/chrono
#   halfround_6in     100 m test course with a 6 inch half-round obstacle
#   detrended_rms_course_2in
#                     505 m random-roughness course, 2 inch RMS
#   Horstwalde        250 m obstacle course, heights up to 1.8 m
set -eu

case ${1:-} in
"") dir= ;;
/*) dir=$1 ;;
*) dir=$PWD/$1 ;;
esac
cd "$(dirname "$0")/.."
dir=${dir:-$PWD/target/crg}
asam=https://raw.githubusercontent.com/asam-ev/OpenCRG/4b747acf9a25f02f329eb1e640836cbd9a35952f/crg-bin
chrono=https://raw.githubusercontent.com/projectchrono/chrono/030e6aa85c3b5d8bfc959455250f02aae659b4b8/data/vehicle/terrain/crg_roads

mkdir -p "$dir" viewer/web
cargo build -q --release --example crg_to_xodr
cargo build -q --release -p xodr-viewer

for file in "$asam/country_road.crg" "$asam/belgian_block.crg" \
    "$chrono/halfround_6in.crg" "$chrono/detrended_rms_course_2in.crg" \
    "$chrono/Horstwalde.crg"; do
    name=$(basename "$file" .crg)
    if [ ! -f "$dir/$name.crg" ]; then
        curl -fsSL -o "$dir/$name.crg.part" "$file"
        mv "$dir/$name.crg.part" "$dir/$name.crg"
    fi
    target/release/examples/crg_to_xodr "$dir/$name.crg"
    target/release/viewer_export "$dir/$name.xodr" "viewer/web/$name.json"
done
