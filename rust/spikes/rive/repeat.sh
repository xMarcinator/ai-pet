#!/usr/bin/env bash
# Five repeated benches of one file at 260x240, unpinned, for the paced CPU medians (question 4).
set -euo pipefail
cd "$(dirname "$0")"
f=${1:-samples/death_knight.riv}
uptime
for i in 1 2 3 4 5; do
    target/release/rive-spike bench "$f" | grep -E '^advance\+draw|^paced'
done
