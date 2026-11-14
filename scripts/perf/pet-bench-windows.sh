#!/usr/bin/env bash
# The performance gate (fn-1 task 28, R18) on Windows, run from Git Bash (never PowerShell): pet-bench.sh's method
# step for step, with the sampler (scripts/perf/sampler) for what Linux reads from /proc. The Rust pet and the C# pet
# side by side, each in a sandbox data folder (AIPET_DATA_DIR) with its own pipe (AIPET_PIPE), wearing Sprout, under
# two workloads:
#   calm    no events: both settle 60 s (they fall asleep), then 5 paired 30 s samples, Rust then C#, alternating;
#   lively  workload.jsonl replayed through aipet-hook into both pets on a 10 s loop (two Claude chats working, one
#           needing attention), with the same settle and samples.
# CPU is the process's user + kernel time over a sample as a percentage of one core; memory its working set and
# private bytes after the settle. Budgets: calm CPU, Rust median <= C# median + 0.5 points; working set, Rust <= 1.5 x
# C# (both workloads); lively CPU is recorded only. Prints the medians and a verdict; exits 1 when a budget fails.
#
#   scripts/perf/pet-bench-windows.sh [rust exe] [C# exe] [hook exe]
# Defaults: the release build, the installed C# pet (%LOCALAPPDATA%\AiPet\current\AiPet.exe, Velopack's layout) and
# the release hook. SAMPLER overrides how the sampler is run (default: built once with `dotnet build` into
# scripts/perf/sampler/out and run with `dotnet`; needs the .NET 8 SDK).
#
# Windows has no load average: before each workload it waits up to WAIT seconds (600) until the whole machine's CPU
# over QUIET seconds (30) is at most MAX_CPU percent of one core (10), and refuses to run if it isn't; the figure is
# printed. SETTLE, SAMPLES and WINDOW (seconds) override the method's numbers for a quick try.
#
# It also runs on Linux (the pids are the pids, the "pipes" sockets), so its arithmetic can be tried there.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
exe=; [[ ${OS:-} == Windows_NT ]] && exe=.exe
rust=${1:-$root/rust/target/release/AiPet$exe}
csharp=${2:-${LOCALAPPDATA:-}/AiPet/current/AiPet.exe}
hook=${3:-$root/rust/target/release/aipet-hook$exe}
settle=${SETTLE:-60} samples=${SAMPLES:-5} window=${WINDOW:-30}
max_cpu=${MAX_CPU:-10} quiet_for=${QUIET:-30} wait=${WAIT:-600}

for f in "$rust" "$csharp" "$hook"; do
    [[ -f $f ]] || { echo "not found: $f" >&2; exit 2; }
done

if [[ -z ${SAMPLER:-} ]]; then
    if [[ ! -f $here/sampler/out/sampler.dll ]]; then
        command -v dotnet >/dev/null || { echo "the sampler needs the .NET 8 SDK (dotnet), or SAMPLER set" >&2; exit 2; }
        dotnet build -c Release -o "$here/sampler/out" "$here/sampler/sampler.csproj" >/dev/null
    fi
    SAMPLER="dotnet $here/sampler/out/sampler.dll"
fi
# shellcheck disable=SC2086
sampler() { $SAMPLER "$@" | tr -d '\r'; }

now_ms() { echo $(($(date +%s%N) / 1000000)); }
median() { printf '%s\n' "$@" | sort -g | awk '{ v[NR] = $1 } END { print (NR % 2) ? v[(NR + 1) / 2] : (v[NR / 2] + v[NR / 2 + 1]) / 2 }'; }

# the whole machine's CPU over QUIET seconds, as a percentage of one core
machine_cpu() {
    local b1 b2 t1 t2
    read -r b1 _ < <(sampler system)
    t1=$(now_ms)
    sleep "$quiet_for"
    read -r b2 _ < <(sampler system)
    t2=$(now_ms)
    awk -v b="$((b2 - b1))" -v t="$((t2 - t1))" 'BEGIN { printf "%.1f", b / t * 100 }'
}
quiet() {
    local cpu waited=0
    while true; do
        cpu=$(machine_cpu)
        awk -v c="$cpu" -v m="$max_cpu" 'BEGIN { exit !(c <= m) }' && break
        waited=$((waited + quiet_for))
        if ((waited >= wait)); then
            echo "the machine's CPU is $cpu % of a core (over $max_cpu %); not measuring" >&2
            exit 1
        fi
    done
    echo "machine CPU before $1: $cpu % of a core" >&2
}

# the pet's process: its Windows pid where Git Bash knows it (/proc/<pid>/winpid), else the pid itself
os_pid() { if [[ -r /proc/$1/winpid ]]; then cat "/proc/$1/winpid"; else echo "$1"; fi; }
cpu_ms() { sampler proc "$1" | awk '{ print $2 }'; }

pids=() dirs=() replay=
cleanup() {
    [[ -n $replay ]] && kill "$replay" 2>/dev/null || true
    for p in "${pids[@]}"; do
        kill "$p" 2>/dev/null || true
        wait "$p" 2>/dev/null || true
    done
    for d in "${dirs[@]}"; do rm -rf "$d"; done
    pids=() dirs=() replay=
}
trap cleanup EXIT

# start <name> <port> <exe>: a pet in a new sandbox wearing Sprout, with its own pipe, and its own MCP port (the Rust
# pet's listener; the C# pet ignores it); prints its pid, its sandbox and its pipe
start() {
    local sandbox pipe
    sandbox=$(mktemp -d "${TMPDIR:-/tmp}/pet-bench.XXXXXX")
    mkdir -p "$sandbox/AiPet"
    printf '{"Pills":true,"OnTop":true,"Avatar":"Sprout","Music":false,"WindowHeight":600}\n' >"$sandbox/AiPet/config.json"
    if [[ $exe == .exe ]]; then pipe="aipet-bench-$1-$$"; else pipe="$sandbox/aipet.sock"; fi
    AIPET_DATA_DIR="$sandbox/AiPet" AIPET_PIPE="$pipe" AIPET_PORT="$2" "$3" >"$sandbox/pet.log" 2>&1 </dev/null &
    echo "$! $sandbox $pipe"
}

# replays workload.jsonl into the pipes given, every 10 s, until killed (its lines are {"t": .., "agent": .., "event": {..}})
replay_into() {
    local shape='^\{"t": ([0-9.]+), "agent": "([a-z]+)", "event": (\{.*\})\}$'
    while true; do
        local begun
        begun=$(now_ms)
        while IFS= read -r line; do
            [[ $line =~ $shape ]] || { echo "can't read workload line: $line" >&2; continue; }
            local t=${BASH_REMATCH[1]} agent=${BASH_REMATCH[2]} event=${BASH_REMATCH[3]}
            while awk -v n="$(($(now_ms) - begun))" -v t="$t" 'BEGIN { exit !(n < t * 1000) }'; do sleep 0.1; done
            for pipe in "$@"; do
                printf '%s' "$event" | AIPET_PIPE="$pipe" "$hook" --agent "$agent" || true
            done
        done <"$here/workload.jsonl"
        while (($(now_ms) - begun < 10000)); do sleep 0.2; done
    done
}

failed=0
printf 'workload\tpet\tcpu_median\tcpu_samples\tworking_set_kb\tprivate_kb\n'
for workload in calm lively; do
    quiet "$workload"
    read -r rpid rdir rpipe < <(start rust 47731 "$rust")
    read -r cpid cdir cpipe < <(start csharp 47732 "$csharp")
    pids=("$rpid" "$cpid") dirs=("$rdir" "$cdir")
    sleep 2
    declare -A os
    os[rust]=$(os_pid "$rpid") os[csharp]=$(os_pid "$cpid")
    if [[ $workload == lively ]]; then
        replay_into "$rpipe" "$cpipe" &
        replay=$!
    fi
    sleep "$settle"
    declare -A ws priv cpus
    for who in rust csharp; do
        read -r _ cpu w p < <(sampler proc "${os[$who]}")
        [[ $cpu == gone ]] && { echo "the $who pet exited; logs in $rdir and $cdir" >&2; exit 1; }
        ws[$who]=$w priv[$who]=$p cpus[$who]=
    done
    for ((i = 0; i < samples; i++)); do
        for who in rust csharp; do
            before=$(cpu_ms "${os[$who]}") s=$(now_ms)
            sleep "$window"
            after=$(cpu_ms "${os[$who]}") e=$(now_ms)
            cpus[$who]+="$(awk -v b="$before" -v a="$after" -v s="$s" -v e="$e" \
                'BEGIN { printf "%.2f", (a - b) / (e - s) * 100 }') "
        done
    done
    declare -A med
    for who in rust csharp; do
        # shellcheck disable=SC2086
        med[$who]=$(median ${cpus[$who]})
        # shellcheck disable=SC2086
        printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$workload" "$who" "${med[$who]}" "$(echo ${cpus[$who]} | tr ' ' ,)" \
            "${ws[$who]}" "${priv[$who]}"
    done
    if [[ $workload == calm ]] && ! awk -v r="${med[rust]}" -v c="${med[csharp]}" 'BEGIN { exit !(r <= c + 0.5) }'; then
        echo "FAIL calm CPU: Rust ${med[rust]} % > C# ${med[csharp]} % + 0.5" >&2
        failed=1
    fi
    if ! awk -v r="${ws[rust]}" -v c="${ws[csharp]}" 'BEGIN { exit !(r <= 1.5 * c) }'; then
        echo "FAIL $workload working set: Rust ${ws[rust]} kB > 1.5 x C# ${ws[csharp]} kB" >&2
        failed=1
    fi
    cleanup
done
if ((failed)); then
    echo "verdict: FAIL"
    exit 1
fi
echo "verdict: PASS"
