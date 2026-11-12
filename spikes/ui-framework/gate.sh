#!/usr/bin/env bash
# FNT-75's gate (R1): the CPU stack (spike-gate) against the release iced pet, paired by fn-1 task 28's method
# (scripts/perf/pet-bench.sh): both run at once, each in its own sandbox, settle 60 s, then 5 rounds of a 30 s sample
# of each, alternating. One run per case: Sprout and Dewdrop, idle and working. CPU is utime+stime over a sample as
# a percentage of one core; memory is VmRSS and Pss after the settle.
#
# The iced pet shows the gate's board (AIPET_GATE_CASE, spike branch only): working, two chats and a review; idle,
# three reviews over an idle chat. The gate draws the same cards, all spread. Both must draw at the same output
# scale: run it once per scale, with the output the two windows open on set to that scale first (on Hyprland,
# `hyprctl eval 'hl.monitor({ output = "<name>", mode = ..., position = ..., scale = 2 })'` on the Lua config, where
# `hyprctl keyword` answers "unknown request"; then 1.5), and pass the scale as the first argument; the gate
# logs the scale it got, and the script refuses a run where it isn't that.
#
# A case passes when the gate's CPU median is below the iced pet's and its RSS is at most 50 MB. It waits up to WAIT
# seconds (600) for the 1-minute load average to be at most MAX_LOAD (1.0, the method's limit), and refuses to run
# otherwise; a run at a raised MAX_LOAD is printed as provisional.
#
#   spikes/ui-framework/gate.sh <scale> [iced AiPet] [gate binary]
# SETTLE, SAMPLES, WINDOW (seconds) and CASES ("sprout:idle sprout:working dewdrop:idle dewdrop:working") override.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
scale=${1:?usage: $0 <scale> [iced AiPet] [gate binary]}
iced=${2:-$root/rust/target/release/AiPet}
gate=${3:-$here/target/release/spike-gate}
settle=${SETTLE:-60} samples=${SAMPLES:-5} window=${WINDOW:-30} max_load=${MAX_LOAD:-1.0} wait=${WAIT:-600}
cases=${CASES:-sprout:idle sprout:working dewdrop:idle dewdrop:working}
ticks=$(getconf CLK_TCK)

busy() { awk -v l="$(cut -d' ' -f1 /proc/loadavg)" -v m="$max_load" 'BEGIN { exit !(l > m) }'; }
quiet() {
    for ((waited = 0; waited < wait && $(busy && echo 1 || echo 0); waited += 15)); do sleep 15; done
    if busy; then
        echo "the 1-minute load average is $(cut -d' ' -f1 /proc/loadavg) (over $max_load); not measuring" >&2
        exit 1
    fi
}
cpu() { awk '{ sub(/^.*\) /, ""); print $12 + $13 }' "/proc/$1/stat"; }
median() { printf '%s\n' "$@" | sort -g | awk '{ v[NR] = $1 } END { print (NR % 2) ? v[(NR + 1) / 2] : (v[NR / 2] + v[NR / 2 + 1]) / 2 }'; }

pids=() dirs=()
cleanup() {
    for p in "${pids[@]}"; do kill "$p" 2>/dev/null || true; wait "$p" 2>/dev/null || true; done
    for d in "${dirs[@]}"; do rm -rf "$d"; done
    pids=() dirs=()
}
trap cleanup EXIT

provisional=
awk -v m="$max_load" 'BEGIN { exit !(m > 1.0) }' && provisional=" (PROVISIONAL: load limit $max_load, over the method's 1.0)"
failed=0
printf 'scale\tcase\tpet\tcpu_median\tcpu_samples\trss_kb\tpss_kb\tgl_maps\n'
for case in $cases; do
    pet=${case%:*} state=${case#*:}
    quiet
    sandbox=$(mktemp -d "${TMPDIR:-/tmp}/gate.XXXXXX")
    dirs+=("$sandbox")
    mkdir -p "$sandbox/data/AiPet/avatars"
    avatar=Sprout
    if [[ $pet == dewdrop ]]; then
        cp -r "$root/avatars/samples/skill-example" "$sandbox/data/AiPet/avatars/"
        avatar="Dewdrop (example)"
    fi
    printf '{"Pills":true,"OnTop":true,"Avatar":"%s","Music":false,"WindowHeight":600}\n' "$avatar" \
        >"$sandbox/data/AiPet/config.json"
    AIPET_GATE_CASE=$state AIPET_PIPE="$sandbox/aipet.sock" XDG_DATA_HOME="$sandbox/data" \
        setsid "$iced" >"$sandbox/iced.log" 2>&1 &
    ipid=$!
    GATE_PET=$pet GATE_STATE=$state GATE_DEBUG=1 setsid "$gate" >"$sandbox/gate.log" 2>&1 &
    gpid=$!
    pids+=("$ipid" "$gpid")
    sleep "$settle"
    for p in "$ipid" "$gpid"; do
        kill -0 "$p" 2>/dev/null || { echo "$case: a pet exited; logs in $sandbox" >&2; exit 1; }
    done
    got=$(grep -o 'scale [0-9.]*' "$sandbox/gate.log" | tail -1 | cut -d' ' -f2)
    if ! awk -v g="${got:-1}" -v s="$scale" 'BEGIN { exit !(g + 0 == s + 0) }'; then
        echo "$case: the gate drew at scale ${got:-1}, not $scale; set the output's scale first" >&2
        exit 1
    fi
    declare -A rss pss cpus maps
    for who in iced gate; do
        p=$([[ $who == iced ]] && echo "$ipid" || echo "$gpid")
        rss[$who]=$(awk '/^VmRSS:/ { print $2 }' "/proc/$p/status")
        pss[$who]=$(awk '/^Pss:/ { print $2 }' "/proc/$p/smaps_rollup")
        maps[$who]=$(grep -cE 'libGL|libEGL|libvulkan|nvidia|/dri/' "/proc/$p/maps" || true)
        cpus[$who]=
    done
    for ((i = 0; i < samples; i++)); do
        for who in iced gate; do
            p=$([[ $who == iced ]] && echo "$ipid" || echo "$gpid")
            before=$(cpu "$p") s=$(date +%s.%N)
            sleep "$window"
            after=$(cpu "$p") e=$(date +%s.%N)
            cpus[$who]+="$(awk -v b="$before" -v a="$after" -v s="$s" -v e="$e" -v t="$ticks" \
                'BEGIN { printf "%.2f", (a - b) / t / (e - s) * 100 }') "
        done
    done
    declare -A med
    for who in iced gate; do
        # shellcheck disable=SC2086
        med[$who]=$(median ${cpus[$who]})
        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$scale" "$case" "$who" "${med[$who]}" \
            "$(echo ${cpus[$who]} | tr ' ' ,)" "${rss[$who]}" "${pss[$who]}" "${maps[$who]}"
    done
    if ! awk -v g="${med[gate]}" -v i="${med[iced]}" 'BEGIN { exit !(g < i) }'; then
        echo "FAIL $scale $case: gate CPU ${med[gate]} % is not below iced ${med[iced]} %" >&2
        failed=1
    fi
    if (( rss[gate] > 50 * 1024 )); then
        echo "FAIL $scale $case: gate RSS ${rss[gate]} kB over 50 MB" >&2
        failed=1
    fi
    tail -1 "$sandbox/gate.log" >&2
    cleanup
done
if (( failed )); then
    echo "verdict at scale $scale: STOP$provisional"
    exit 1
fi
echo "verdict at scale $scale: GO$provisional"
