#!/usr/bin/env bash
# The performance gate (fn-1 task 28, R18) on Linux: the Rust pet and the C# pet side by side, each in a sandbox data
# folder with its own socket, wearing Sprout, measured under two workloads:
#   calm    no events: both settle 60 s (they fall asleep), then 5 paired 30 s samples, Rust then C#, alternating;
#   lively  workload.jsonl replayed through aipet-hook into both pets on a 10 s loop (two Claude chats working, one
#           needing attention), with the same settle and samples.
# CPU is the process's utime+stime over a sample as a percentage of one core; memory VmRSS and Pss after the settle.
# Budgets: calm CPU, Rust median <= C# median + 0.5 points; RSS, Rust <= 1.5 x C# (both workloads); lively CPU is
# recorded only. Prints the medians and a verdict; exits 1 when a budget fails.
#
#   scripts/perf/pet-bench.sh [rust binary] [C# binary] [hook binary]
# SETTLE, SAMPLES, WINDOW (seconds) override the method's numbers for a quick try; it waits up to WAIT seconds (600)
# for the 1-minute load average to be at most MAX_LOAD (1.0) before each workload, and refuses to run if it isn't.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
rust=${1:-rust/target/release/AiPet}
csharp=${2:-$HOME/.local/share/AiPet/app/AiPet}
hook=${3:-rust/target/release/aipet-hook}
settle=${SETTLE:-60} samples=${SAMPLES:-5} window=${WINDOW:-30} max_load=${MAX_LOAD:-1.0} wait=${WAIT:-600}
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

pids=() dirs=() replay=
cleanup() {
    [[ -n $replay ]] && kill "$replay" 2>/dev/null || true
    for p in "${pids[@]}"; do kill "$p" 2>/dev/null || true; wait "$p" 2>/dev/null || true; done
    for d in "${dirs[@]}"; do rm -rf "$d"; done
    pids=() dirs=() replay=
}
trap cleanup EXIT

# start <command…>: a pet in a new sandbox, wearing Sprout; prints its pid and sandbox
start() {
    local sandbox
    sandbox=$(mktemp -d "${TMPDIR:-/tmp}/pet-bench.XXXXXX")
    mkdir -p "$sandbox/data/AiPet"
    printf '{"Left":3900,"Top":231,"Pills":true,"OnTop":true,"Avatar":"Sprout","Music":false,"WindowHeight":600}\n' \
        >"$sandbox/data/AiPet/config.json"
    AIPET_PIPE="$sandbox/aipet.sock" XDG_DATA_HOME="$sandbox/data" setsid "$@" >"$sandbox/pet.log" 2>&1 &
    echo "$! $sandbox"
}

# replays workload.jsonl into the sockets given, every 10 s, until killed
replay_into() {
    while true; do
        local begun=$SECONDS
        while IFS= read -r line; do
            local t agent event
            t=$(jq -r .t <<<"$line") agent=$(jq -r .agent <<<"$line") event=$(jq -c .event <<<"$line")
            while (( $(awk -v n="$(( SECONDS - begun ))" -v t="$t" 'BEGIN { print (n < t) }') )); do sleep 0.1; done
            for sock in "$@"; do
                printf '%s' "$event" | AIPET_PIPE="$sock" "$hook" --agent "$agent" || true
            done
        done <"$here/workload.jsonl"
        while (( SECONDS - begun < 10 )); do sleep 0.2; done
    done
}

failed=0
printf 'workload\tpet\tcpu_median\tcpu_samples\trss_kb\tpss_kb\n'
for workload in calm lively; do
    quiet
    read -r rpid rdir < <(start "$rust")
    read -r cpid cdir < <(start "$csharp")
    pids=("$rpid" "$cpid") dirs=("$rdir" "$cdir")
    if [[ $workload == lively ]]; then
        sleep 2
        replay_into "$rdir/aipet.sock" "$cdir/aipet.sock" &
        replay=$!
    fi
    sleep "$settle"
    for p in "$rpid" "$cpid"; do
        kill -0 "$p" 2>/dev/null || { echo "a pet exited; logs in $rdir and $cdir" >&2; exit 1; }
    done
    declare -A rss pss cpus
    for who in rust csharp; do
        p=$([[ $who == rust ]] && echo "$rpid" || echo "$cpid")
        rss[$who]=$(awk '/^VmRSS:/ { print $2 }' "/proc/$p/status")
        pss[$who]=$(awk '/^Pss:/ { print $2 }' "/proc/$p/smaps_rollup")
        cpus[$who]=
    done
    for ((i = 0; i < samples; i++)); do
        for who in rust csharp; do
            p=$([[ $who == rust ]] && echo "$rpid" || echo "$cpid")
            before=$(cpu "$p") s=$(date +%s.%N)
            sleep "$window"
            after=$(cpu "$p") e=$(date +%s.%N)
            cpus[$who]+="$(awk -v b="$before" -v a="$after" -v s="$s" -v e="$e" -v t="$ticks" \
                'BEGIN { printf "%.2f", (a - b) / t / (e - s) * 100 }') "
        done
    done
    declare -A med
    for who in rust csharp; do
        # shellcheck disable=SC2086
        med[$who]=$(median ${cpus[$who]})
        printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$workload" "$who" "${med[$who]}" "$(echo ${cpus[$who]} | tr ' ' ,)" \
            "${rss[$who]}" "${pss[$who]}"
    done
    if [[ $workload == calm ]] && ! awk -v r="${med[rust]}" -v c="${med[csharp]}" 'BEGIN { exit !(r <= c + 0.5) }'; then
        echo "FAIL calm CPU: Rust ${med[rust]} % > C# ${med[csharp]} % + 0.5" >&2
        failed=1
    fi
    if ! awk -v r="${rss[rust]}" -v c="${rss[csharp]}" 'BEGIN { exit !(r <= 1.5 * c) }'; then
        echo "FAIL $workload RSS: Rust ${rss[rust]} kB > 1.5 x C# ${rss[csharp]} kB" >&2
        failed=1
    fi
    cleanup
done
if (( failed )); then
    echo "verdict: FAIL"
    exit 1
fi
echo "verdict: PASS"
