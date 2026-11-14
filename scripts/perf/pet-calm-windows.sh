#!/usr/bin/env bash
# Calm CPU and memory of one pet on Windows, run from Git Bash (never PowerShell): pet-calm.sh's method with the
# sampler (scripts/perf/sampler) for what Linux reads from /proc. The pet starts in a sandbox data folder
# (AIPET_DATA_DIR, its own pipe and MCP port, so it runs beside your pet) wearing the avatar named, settles for 60 s
# with no events, then 5 samples of 30 s. CPU is user + kernel time over each sample as a percentage of one core;
# memory the working set and private bytes after the settle. Prints one tab-separated line: label, median CPU %, the
# samples, working set kB, private kB.
#
#   scripts/perf/pet-calm-windows.sh <label> <avatar> [theme] -- <pet exe…>
#
# CONNECT=1 keeps an MCP client connected (fnt-71 R13: mcp-client.sh hold); LISTENER=off holds the pet's MCP port
# first, so its listener can't start. SETTLE, SAMPLES and WINDOW (seconds) override the method's numbers; MAX_CPU,
# QUIET and WAIT the machine-CPU gate (pet-bench-windows.sh's). SAMPLER as there. It also runs on Linux.
set -euo pipefail

label=$1 avatar=$2
shift 2
theme=
if [[ $1 != -- ]]; then
    theme=$1
    shift
fi
[[ $1 == -- ]] || { echo "usage: $0 <label> <avatar> [theme] -- <pet exe…>" >&2; exit 2; }
shift

here=$(cd "$(dirname "$0")" && pwd)
exe=; [[ ${OS:-} == Windows_NT ]] && exe=.exe
settle=${SETTLE:-60} samples=${SAMPLES:-5} window=${WINDOW:-30}
max_cpu=${MAX_CPU:-10} quiet_for=${QUIET:-30} wait=${WAIT:-600}
port=${AIPET_PORT:-47741}

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

# the machine-CPU gate (Windows has no load average)
waited=0
while true; do
    read -r b1 _ < <(sampler system)
    t1=$(now_ms)
    sleep "$quiet_for"
    read -r b2 _ < <(sampler system)
    t2=$(now_ms)
    machine=$(awk -v b="$((b2 - b1))" -v t="$((t2 - t1))" 'BEGIN { printf "%.1f", b / t * 100 }')
    awk -v c="$machine" -v m="$max_cpu" 'BEGIN { exit !(c <= m) }' && break
    waited=$((waited + quiet_for))
    if ((waited >= wait)); then
        echo "$label: the machine's CPU is $machine % of a core (over $max_cpu %); not measuring" >&2
        exit 1
    fi
done

sandbox=$(mktemp -d "${TMPDIR:-/tmp}/pet-calm.XXXXXX")
mkdir -p "$sandbox/AiPet"
theme_field=
[[ -n $theme ]] && theme_field=",\"AvatarTheme\":\"$theme\""
printf '{"Pills":true,"OnTop":true,"Avatar":"%s"%s,"Music":false,"WindowHeight":600}\n' "$avatar" "$theme_field" \
    >"$sandbox/AiPet/config.json"
if [[ $exe == .exe ]]; then pipe="aipet-calm-$$"; else pipe="$sandbox/aipet.sock"; fi

helpers=()
if [[ ${LISTENER:-on} == off ]]; then
    # shellcheck disable=SC2086
    $SAMPLER hold-port "$port" >/dev/null &
    helpers+=($!)
    sleep 1
fi
AIPET_DATA_DIR="$sandbox/AiPet" AIPET_PIPE="$pipe" AIPET_PORT="$port" "$@" >"$sandbox/pet.log" 2>&1 </dev/null &
pid=$!
cleanup() {
    kill "$pid" "${helpers[@]}" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
    rm -rf "$sandbox"
}
trap cleanup EXIT
sleep 2
if [[ -r /proc/$pid/winpid ]]; then os=$(cat "/proc/$pid/winpid"); else os=$pid; fi
if [[ ${CONNECT:-} == 1 ]]; then
    sleep 3
    AIPET_DATA_DIR="$sandbox/AiPet" AIPET_PORT=$port "$here/mcp-client.sh" hold
fi

sleep "$settle"
read -r _ cpu ws priv < <(sampler proc "$os")
[[ $cpu == gone ]] && { echo "$label: the pet exited (see its log)" >&2; cat "$sandbox/pet.log" >&2; exit 1; }

cpus=()
for ((i = 0; i < samples; i++)); do
    before=$(sampler proc "$os" | awk '{ print $2 }') s=$(now_ms)
    sleep "$window"
    after=$(sampler proc "$os" | awk '{ print $2 }') e=$(now_ms)
    cpus+=("$(awk -v b="$before" -v a="$after" -v s="$s" -v e="$e" 'BEGIN { printf "%.2f", (a - b) / (e - s) * 100 }')")
done
median=$(printf '%s\n' "${cpus[@]}" | sort -g | awk '{ v[NR] = $1 } END { print (NR % 2) ? v[(NR + 1) / 2] : (v[NR / 2] + v[NR / 2 + 1]) / 2 }')
printf '%s\t%s\t%s\t%s\t%s\n' "$label" "$median" "$(IFS=,; echo "${cpus[*]}")" "$ws" "$priv"
