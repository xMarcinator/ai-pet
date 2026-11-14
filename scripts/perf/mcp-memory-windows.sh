#!/usr/bin/env bash
# R15 (fnt-71) on Windows, run from Git Bash: mcp-memory.sh's paired method with the sampler for what Linux reads from
# /proc. Two pets of the same build side by side, each in its own sandbox (AIPET_DATA_DIR, its own pipe): one with the
# listener on and an MCP client connected (mcp-client.sh hold), one with its port held (sampler hold-port) so its
# listener can't start. After SETTLE seconds both are read at the same moment; then both get the same event, a hook
# body just under the 4 MiB cap (over HTTP to the "on" pet, through the old exe and its pipe to the "off" one), and
# both are read every 10 s for 60 s.
#
#   scripts/perf/mcp-memory-windows.sh [pairs] [pet exe] [aipet-hook exe] > results.tsv
# Prints: pair, phase, per pet working set kB and private kB, then on minus off for both. Alternates which pet starts
# first. SAMPLER as in pet-bench-windows.sh. It also runs on Linux.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
exe=; [[ ${OS:-} == Windows_NT ]] && exe=.exe
pairs=${1:-3}
pet=${2:-$root/rust/target/release/AiPet$exe}
hook=${3:-$(dirname "$pet")/aipet-hook$exe}
settle=${SETTLE:-60}
port_on=${PORT_ON:-47751} port_off=${PORT_OFF:-47752}

if [[ -z ${SAMPLER:-} ]]; then
    if [[ ! -f $here/sampler/out/sampler.dll ]]; then
        command -v dotnet >/dev/null || { echo "the sampler needs the .NET 8 SDK (dotnet), or SAMPLER set" >&2; exit 2; }
        dotnet build -c Release -o "$here/sampler/out" "$here/sampler/sampler.csproj" >/dev/null
    fi
    SAMPLER="dotnet $here/sampler/out/sampler.dll"
fi
# shellcheck disable=SC2086
sampler() { $SAMPLER "$@" | tr -d '\r'; }
os_pid() { if [[ -r /proc/$1/winpid ]]; then cat "/proc/$1/winpid"; else echo "$1"; fi; }

printf 'pair\tphase\ton_ws_kb\ton_private_kb\toff_ws_kb\toff_private_kb\tdiff_ws_kb\tdiff_private_kb\n'
for ((pair = 1; pair <= pairs; pair++)); do
    box=$(mktemp -d "${TMPDIR:-/tmp}/mcp-mem.XXXXXX")
    pids=()
    cleanup() {
        for p in "${pids[@]}"; do kill "$p" 2>/dev/null || true; done
        wait 2>/dev/null || true
        rm -rf "$box"
    }
    trap cleanup EXIT
    # shellcheck disable=SC2086
    $SAMPLER hold-port "$port_off" >/dev/null &
    pids+=($!)
    sleep 1
    pipe() { if [[ $exe == .exe ]]; then echo "aipet-mcp-mem-$1-$$"; else echo "$box/$1.sock"; fi; }
    start() { # name port
        mkdir -p "$box/$1/AiPet"
        printf '{"Pills":true,"OnTop":true,"Avatar":"Sprout","Music":false}\n' >"$box/$1/AiPet/config.json"
        AIPET_DATA_DIR="$box/$1/AiPet" AIPET_PIPE="$(pipe "$1")" AIPET_PORT=$2 "$pet" >"$box/$1.log" 2>&1 </dev/null &
        echo $!
    }
    if ((pair % 2)); then
        on=$(start on "$port_on"); off=$(start off "$port_off")
    else
        off=$(start off "$port_off"); on=$(start on "$port_on")
    fi
    pids+=("$on" "$off")
    sleep 5
    grep -q "mcp: listening on 127.0.0.1:$port_on" "$box/on/AiPet/aipet.log" || { echo "the on pet isn't listening" >&2; exit 1; }
    grep -q "mcp: can't listen" "$box/off/AiPet/aipet.log" || { echo "the off pet is listening" >&2; exit 1; }
    on_os=$(os_pid "$on") off_os=$(os_pid "$off")
    AIPET_DATA_DIR="$box/on/AiPet" AIPET_PORT=$port_on "$here/mcp-client.sh" hold
    sleep "$settle"
    row() {
        read -r _ _ a b < <(sampler proc "$on_os")
        read -r _ _ c d < <(sampler proc "$off_os")
        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$pair" "$1" "$a" "$b" "$c" "$d" $((a - c)) $((b - d))
    }
    row idle
    AIPET_PORT=$port_on "$here/mcp-client.sh" big-body
    "$here/mcp-client.sh" big-body-stdout | AIPET_PIPE="$(pipe off)" AIPET_DATA_DIR="$box/off/AiPet" "$hook" >/dev/null 2>&1 || true
    for t in 0 10 20 30 40 50 60; do
        row "big+${t}s"
        ((t < 60)) && sleep 10
    done
    cleanup
    trap - EXIT
done
