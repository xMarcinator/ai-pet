#!/usr/bin/env bash
# Question 6: truncated and corrupted copies of each sample, each loaded in a child process with a 5 s limit, first
# fully (parse, instantiate, 30 frames), then parse only (File::new). Crashing and hanging copies stay in out/.
set -euo pipefail
cd "$(dirname "$0")"
rm -rf out/fuzz out/fuzz-parse
for f in samples/death_knight.riv samples/rating-animation.riv samples/bad_skin.riv; do
    echo "== $f (full load)"
    target/release/rive-spike fuzz "$f" out/fuzz | head -1
    echo "== $f (File::new only)"
    SPIKE_PARSE_ONLY=1 target/release/rive-spike fuzz "$f" out/fuzz-parse | head -1
done
