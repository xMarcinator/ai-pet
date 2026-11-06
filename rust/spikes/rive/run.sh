#!/usr/bin/env bash
# Runs the spike's steps on the sample files (after `cargo build --release`). Usage: ./run.sh <step> [files...]
set -euo pipefail
cd "$(dirname "$0")"
bin=target/release/rive-spike
v=vendor/rive-rs
t=$v/submodules/rive-cpp/tests/unit_tests/assets
samples=(
    "$v/assets/rating-animation.riv"
    "$t/state_machine_triggers.riv"
    "$t/light_switch.riv"
    "$t/juice.riv"
)
step=$1
shift
# samples/: symlinks into the clone, which bench.sh, repeat.sh and fuzz.sh use
mkdir -p samples
for f in "$v/assets/rating-animation.riv" "$t/death_knight.riv" "$t/bad_skin.riv" "$t/hunter_x_demo.riv" \
    "$t/hosted_image_file.riv" "$t/light_switch.riv" "$v/submodules/rive-cpp/renderer/webgpu_player/rivs/Timer.riv"; do
    ln -sf "../$f" "samples/$(basename "$f")"
done
[ "$step" = link ] && exit 0
files=("$@")
[ ${#files[@]} -eq 0 ] && files=("${samples[@]}")
for f in "${files[@]}"; do
    case $step in
    info) "$bin" info "$f" ;;
    render) "$bin" render "$f" out/render 8 250 ;;
    inputs) "$bin" inputs "$f" out/inputs ;;
    bench) "$bin" bench "$f" ;;
    bench2x) "$bin" bench "$f" 520 480 ;;
    fuzz) "$bin" fuzz "$f" out/fuzz ;;
    *) echo "unknown step $step" >&2; exit 64 ;;
    esac
    echo "---"
done
