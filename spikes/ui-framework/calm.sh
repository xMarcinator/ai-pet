#!/usr/bin/env bash
# The calm runs that ui-framework.md still lacks: fn-1's method (scripts/perf/pet-calm.sh) for the release iced pet
# and each prototype, one after another, on a quiet machine (pet-calm.sh refuses above a 1-minute load of 1.0).
# Build the prototypes first (cargo build --release in each folder, CARGO_TARGET_DIR=../target).
#
#   spikes/ui-framework/calm.sh <path to the release AiPet>
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
pet=${1:?usage: $0 <release AiPet binary>}
t=$here/target/release
mesa=/usr/share/glvnd/egl_vendor.d/50_mesa.json
calm=$root/scripts/perf/pet-calm.sh
"$calm" iced-release Sprout -- "$pet"
"$calm" minimal Sprout -- "$t/spike-minimal"
"$calm" slint Sprout -- "$t/spike-slint"
# gpui on Mesa GL (Intel), as the iced pet runs; then its own choice (here NVIDIA's Vulkan)
"$calm" gpui-gl Sprout -- env VK_LOADER_DRIVERS_DISABLE='*' __EGL_VENDOR_LIBRARY_FILENAMES=$mesa "$t/spike-gpui"
"$calm" gpui-default Sprout -- "$t/spike-gpui"
