#!/usr/bin/env bash
# The spike's performance runs (question 4): each sample unpinned, on a P-core (8) and on an E-core (16) of the
# i7-12850HX, at 260x240 and 520x480. Usage: ./bench.sh [files...]
set -euo pipefail
cd "$(dirname "$0")"
bin=target/release/rive-spike
files=("$@")
[ ${#files[@]} -eq 0 ] && files=(samples/death_knight.riv samples/bad_skin.riv samples/rating-animation.riv)
uptime
for f in "${files[@]}"; do
    for size in "260 240" "520 480"; do
        read -r w h <<<"$size"
        echo "=== $f ${w}x${h} unpinned"
        "$bin" bench "$f" "$w" "$h"
        for cpu in 8 16; do
            echo "=== $f ${w}x${h} on cpu $cpu"
            taskset -c "$cpu" "$bin" bench "$f" "$w" "$h" | grep -E 'advance|paced|idle'
        done
    done
done
