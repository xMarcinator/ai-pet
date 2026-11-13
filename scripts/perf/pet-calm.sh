#!/usr/bin/env bash
# Calm CPU and memory of one pet on Linux, with fn-1 task 28's method: the pet starts in a sandbox data folder (its
# own socket, so it runs beside your pet) wearing the avatar named, settles for 60 s with no events (it falls
# asleep), then 5 samples of 30 s each. CPU is the process's utime+stime over each sample as a percentage of one
# core; memory is VmRSS and Pss after the settle. Prints one tab-separated line: label, median CPU %, the samples,
# RSS kB, PSS kB.
#
#   scripts/perf/pet-calm.sh <label> <avatar> [theme] -- <pet command…>
#   scripts/perf/pet-calm.sh rust-sprout Sprout -- rust/target/release/AiPet
#   scripts/perf/pet-calm.sh csharp-sprout Sprout -- ~/.local/share/AiPet/app/AiPet
#
# CONNECT=1 keeps an MCP client connected and idle to the pet's listener (fnt-71 R13; mcp-client.py, the pet's
# port: AIPET_PORT or 47615). LISTENER=off holds that port first, so the pet's listener can't start.
# SETTLE, SAMPLES and WINDOW (seconds) override the method's numbers for a quick try. It waits up to WAIT seconds
# (600) for the 1-minute load average to be at most MAX_LOAD (1.0), and refuses to run if it isn't.
set -euo pipefail

label=$1 avatar=$2
shift 2
theme=
if [[ $1 != -- ]]; then
    theme=$1
    shift
fi
[[ $1 == -- ]] || { echo "usage: $0 <label> <avatar> [theme] -- <pet command…>" >&2; exit 2; }
shift

settle=${SETTLE:-60} samples=${SAMPLES:-5} window=${WINDOW:-30} max_load=${MAX_LOAD:-1.0} wait=${WAIT:-600}
busy() { awk -v l="$(cut -d' ' -f1 /proc/loadavg)" -v m="$max_load" 'BEGIN { exit !(l > m) }'; }
for ((waited = 0; waited < wait && $(busy && echo 1 || echo 0); waited += 15)); do
    sleep 15
done
load=$(cut -d' ' -f1 /proc/loadavg)
if busy; then
    echo "$label: the 1-minute load average is $load (over $max_load); not measuring" >&2
    exit 1
fi

sandbox=$(mktemp -d "${TMPDIR:-/tmp}/pet-calm.XXXXXX")
mkdir -p "$sandbox/data/AiPet"
theme_field=
[[ -n $theme ]] && theme_field=",\"AvatarTheme\":\"$theme\""
printf '{"Left":3900,"Top":231,"Pills":true,"OnTop":true,"Avatar":"%s"%s,"Music":false,"WindowHeight":600}\n' \
    "$avatar" "$theme_field" >"$sandbox/data/AiPet/config.json"

here=$(cd "$(dirname "$0")" && pwd)
helpers=()
if [[ ${LISTENER:-on} == off ]]; then
    python3 -c "import socket,time;s=socket.socket();s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1);s.bind(('127.0.0.1',${AIPET_PORT:-47615}));s.listen();time.sleep(10**6)" &
    helpers+=($!)
    sleep 1
fi
AIPET_PIPE="$sandbox/aipet.sock" XDG_DATA_HOME="$sandbox/data" setsid "$@" >"$sandbox/pet.log" 2>&1 &
pid=$!
if [[ ${CONNECT:-} == 1 ]]; then
    sleep 5
    AIPET_DATA_DIR="$sandbox/data/AiPet" AIPET_PORT=${AIPET_PORT:-47615} python3 -I "$here/mcp-client.py" hold &
    helpers+=($!)
fi
cleanup() {
    kill "$pid" "${helpers[@]}" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
    rm -rf "$sandbox"
}
trap cleanup EXIT

ticks=$(getconf CLK_TCK)
# utime + stime, in clock ticks (fields 14 and 15, after the command name in parentheses)
cpu() { awk '{ sub(/^.*\) /, ""); print $12 + $13 }' "/proc/$pid/stat"; }

sleep "$settle"
kill -0 "$pid" 2>/dev/null || { echo "$label: the pet exited (see its log)" >&2; cat "$sandbox/pet.log" >&2; exit 1; }
rss=$(awk '/^VmRSS:/ { print $2 }' "/proc/$pid/status")
pss=$(awk '/^Pss:/ { print $2 }' "/proc/$pid/smaps_rollup")

cpus=()
for ((i = 0; i < samples; i++)); do
    before=$(cpu) start=$(date +%s.%N)
    sleep "$window"
    after=$(cpu) end=$(date +%s.%N)
    cpus+=("$(awk -v b="$before" -v a="$after" -v s="$start" -v e="$end" -v t="$ticks" \
        'BEGIN { printf "%.2f", (a - b) / t / (e - s) * 100 }')")
done
median=$(printf '%s\n' "${cpus[@]}" | sort -g | awk '{ v[NR] = $1 } END { print (NR % 2) ? v[(NR + 1) / 2] : (v[NR / 2] + v[NR / 2 + 1]) / 2 }')
printf '%s\t%s\t%s\t%s\t%s\n' "$label" "$median" "$(IFS=,; echo "${cpus[*]}")" "$rss" "$pss"
