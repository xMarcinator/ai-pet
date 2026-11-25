#!/usr/bin/env bash
# The performance gate (fn-1 task 28, R18) on Windows, run from Git Bash (never PowerShell): pet-bench.sh's method
# with the sampler (scripts/perf/sampler) for what Linux reads from /proc. The Rust pet and the C# pet, each in a
# sandbox data folder (AIPET_DATA_DIR) with its own pipe (AIPET_PIPE), wearing Sprout, under two workloads:
#   calm    no events: the pet settles 60 s (it falls asleep), then 5 samples of 30 s;
#   lively  workload.jsonl replayed through aipet-hook into the pet on a 10 s loop (two Claude chats working, one
#           needing attention), with the same settle and samples.
# Where pet-bench.sh runs both pets side by side and alternates their samples, Windows can't: both pets take the
# single-instance mutex Local\AiPetApp whatever AIPET_PIPE says (one pet per user), so the second would quit. Each pet
# runs alone, the Rust pet first, then the C# pet, under each workload; the gate asks for two runs of the script,
# which shows the drift alternation would have hidden.
# CPU is the process's user + kernel time over a sample as a percentage of one core; memory its working set and
# private bytes after the settle. Budgets: calm CPU, Rust median <= C# median + 0.5 points; working set, Rust <= 1.5 x
# C# (both workloads); lively CPU is recorded only. Prints the medians and a verdict; exits 1 when a budget fails.
#
#   scripts/perf/pet-bench-windows.sh [rust exe] [C# exe] [hook exe]
# Defaults: the release build, the C# 0.1.0 pet in ~/AiPet-0.1.0 (a copy of the installed one's files, or the v0.1.0
# portable zip: not the Velopack folder itself, whose copy checks for updates and installs one on its next start)
# and the release hook. Quit your own pet first (it holds the mutex), and leave the mouse alone (the eyes follow it).
# SAMPLER overrides how the sampler is run (default: built once with `dotnet build` into scripts/perf/sampler/out
# and run with `dotnet`; needs the .NET 8 SDK).
#
# Windows has no load average: before each pet it waits up to WAIT seconds (600) until the whole machine's CPU over
# QUIET seconds (30) is at most MAX_CPU percent of one core (10), and refuses to run if it isn't; the figure is
# printed. SETTLE, SAMPLES and WINDOW (seconds) override the method's numbers for a quick try.
#
# It also runs on Linux (the pids are the pids, the "pipes" sockets), so its arithmetic can be tried there.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
exe=; [[ ${OS:-} == Windows_NT ]] && exe=.exe
rust=${1:-$root/rust/target/release/AiPet$exe}
csharp=${2:-$HOME/AiPet-0.1.0/AiPet$exe}
hook=${3:-$root/rust/target/release/aipet-hook$exe}
settle=${SETTLE:-60} samples=${SAMPLES:-5} window=${WINDOW:-30}
max_cpu=${MAX_CPU:-10} quiet_for=${QUIET:-30} wait=${WAIT:-600}

for f in "$rust" "$csharp" "$hook"; do
    [[ -f $f ]] || { echo "not found: $f" >&2; exit 2; }
done

if [[ -z ${SAMPLER:-} ]]; then
    if [[ ! -f $here/sampler/out/sampler.dll ]]; then
        command -v dotnet >/dev/null || { echo "the sampler needs the .NET 8 SDK (dotnet), or SAMPLER set" >&2; exit 2; }
        dotnet build -c Release -p:UseAppHost=false -o "$here/sampler/out" "$here/sampler/sampler.csproj" >/dev/null
    fi
    SAMPLER="dotnet $here/sampler/out/sampler.dll"
fi
# shellcheck disable=SC2086
sampler() { $SAMPLER "$@" | tr -d '\r'; }

now_ms() { echo $(($(date +%s%N) / 1000000)); }
median() { printf '%s\n' "$@" | sort -g | awk '{ v[NR] = $1 } END { print (NR % 2) ? v[(NR + 1) / 2] : (v[NR / 2] + v[NR / 2 + 1]) / 2 }'; }
# a path as the pet reads it: Windows programs get C:\…, not Git Bash's /tmp/…
native() { if [[ $exe == .exe ]]; then cygpath -w "$1"; else echo "$1"; fi; }

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

pid= sandbox= pipe= replay=
cleanup() {
    [[ -n $replay ]] && { kill "$replay" 2>/dev/null || true; wait "$replay" 2>/dev/null || true; }
    [[ -n $pid ]] && { kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; }
    # the pet's files come free a moment after it has gone; a sandbox that won't go is reported, not fatal
    [[ -n $sandbox ]] && { rm -rf "$sandbox" 2>/dev/null || { sleep 2; rm -rf "$sandbox" 2>/dev/null; } ||
        echo "left behind: $sandbox" >&2; }
    pid= sandbox= pipe= replay=
}
trap cleanup EXIT

# start <name> <port> <exe>: a pet in a new sandbox wearing Sprout, with its own pipe and its own MCP port (the Rust
# pet's listener; the C# pet ignores it), and the agents' folders in the sandbox too, so neither pet reads the
# owner's; sets pid, sandbox and pipe. Not run in a subshell: the pet stays this shell's child, for kill and wait.
start() {
    sandbox=$(mktemp -d "${TMPDIR:-/tmp}/pet-bench.XXXXXX")
    mkdir -p "$sandbox/AiPet" "$sandbox/claude" "$sandbox/codex"
    printf '{"Pills":true,"OnTop":true,"Avatar":"Sprout","Music":false,"WindowHeight":600}\n' >"$sandbox/AiPet/config.json"
    if [[ $exe == .exe ]]; then pipe="aipet-bench-$1-$$"; else pipe="$sandbox/aipet.sock"; fi
    AIPET_DATA_DIR="$(native "$sandbox/AiPet")" AIPET_PIPE="$pipe" AIPET_PORT="$2" \
        CLAUDE_CONFIG_DIR="$(native "$sandbox/claude")" CODEX_HOME="$(native "$sandbox/codex")" \
        "$3" >"$sandbox/pet.log" 2>&1 </dev/null &
    pid=$!
}

# replays workload.jsonl into the pipe given, every 10 s, until killed (its lines are {"t": .., "agent": .., "event": {..}})
replay_into() {
    local shape='^\{"t": ([0-9.]+), "agent": "([a-z]+)", "event": (\{.*\})\}$'
    while true; do
        local begun
        begun=$(now_ms)
        while IFS= read -r line; do
            line=${line%$'\r'} # a Windows checkout (core.autocrlf) gives the file CRLF line ends
            [[ $line =~ $shape ]] || { echo "can't read workload line: $line" >&2; continue; }
            local t=${BASH_REMATCH[1]} agent=${BASH_REMATCH[2]} event=${BASH_REMATCH[3]}
            while awk -v n="$(($(now_ms) - begun))" -v t="$t" 'BEGIN { exit !(n < t * 1000) }'; do sleep 0.1; done
            printf '%s' "$event" | AIPET_PIPE="$1" "$hook" --agent "$agent" || true
        done <"$here/workload.jsonl"
        while (($(now_ms) - begun < 10000)); do sleep 0.2; done
    done
}

failed=0
printf 'workload\tpet\tcpu_median\tcpu_samples\tworking_set_kb\tprivate_kb\n'
for workload in calm lively; do
    declare -A ws priv med
    for who in rust csharp; do
        quiet "$workload/$who"
        if [[ $who == rust ]]; then start rust 47731 "$rust"; else start csharp 47732 "$csharp"; fi
        sleep 2
        ospid=$(os_pid "$pid")
        if [[ $workload == lively ]]; then
            replay_into "$pipe" &
            replay=$!
        fi
        sleep "$settle"
        read -r _ cpu w p < <(sampler proc "$ospid")
        if [[ $cpu == gone ]]; then
            echo "the $who pet exited; its log: $sandbox/pet.log" >&2
            cat "$sandbox/pet.log" >&2
            sandbox=
            exit 1
        fi
        ws[$who]=$w priv[$who]=$p
        cpus=()
        for ((i = 0; i < samples; i++)); do
            before=$(cpu_ms "$ospid") s=$(now_ms)
            sleep "$window"
            after=$(cpu_ms "$ospid") e=$(now_ms)
            if [[ $after == gone || -z $after ]]; then
                echo "the $who pet exited during sample $i; its log: $sandbox/pet.log" >&2
                sandbox=
                exit 1
            fi
            cpus+=("$(awk -v b="$before" -v a="$after" -v s="$s" -v e="$e" \
                'BEGIN { printf "%.2f", (a - b) / (e - s) * 100 }')")
        done
        med[$who]=$(median "${cpus[@]}")
        printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$workload" "$who" "${med[$who]}" "$(IFS=,; echo "${cpus[*]}")" \
            "${ws[$who]}" "${priv[$who]}"
        cleanup
    done
    if [[ $workload == calm ]] && ! awk -v r="${med[rust]}" -v c="${med[csharp]}" 'BEGIN { exit !(r <= c + 0.5) }'; then
        echo "FAIL calm CPU: Rust ${med[rust]} % > C# ${med[csharp]} % + 0.5" >&2
        failed=1
    fi
    if ! awk -v r="${ws[rust]}" -v c="${ws[csharp]}" 'BEGIN { exit !(r <= 1.5 * c) }'; then
        echo "FAIL $workload working set: Rust ${ws[rust]} kB > 1.5 x C# ${ws[csharp]} kB" >&2
        failed=1
    fi
    unset ws priv med
done
if ((failed)); then
    echo "verdict: FAIL"
    exit 1
fi
echo "verdict: PASS"
