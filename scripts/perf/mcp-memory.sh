#!/usr/bin/env bash
# R15 (fnt-71): the MCP listener's memory, paired. Two pets of the same build run side by side, each in its own
# sandbox: one with the listener on (an MCP client connected and idle), one with it off (its port held by another
# socket, so the bind fails and the listener never starts). After SETTLE seconds both are read at the same moment;
# then the "on" pet gets one hook body just under the 4 MiB cap and both are read every 10 s for 60 s.
#
#   scripts/perf/mcp-memory.sh [pairs] [pet binary] [aipet-hook binary] > results.tsv
# Prints: pair, phase (idle, or seconds after the big body), and per pet RSS kB and anonymous kB (Anonymous in
# smaps_rollup), then on-minus-off for both. Alternates which pet starts first.
set -euo pipefail

pairs=${1:-3}
pet=${2:-rust/target/release/AiPet}
hook=${3:-$(dirname "$pet")/aipet-hook}
settle=${SETTLE:-60}
here=$(cd "$(dirname "$0")" && pwd)
port_on=${PORT_ON:-47701} port_off=${PORT_OFF:-47702}

read_mem() { # pid -> "rss anon"
    printf '%s %s' "$(awk '/^VmRSS:/ { print $2 }' "/proc/$1/status")" \
        "$(awk '/^Anonymous:/ { print $2 }' "/proc/$1/smaps_rollup")"
}

printf 'pair\tphase\ton_rss_kb\ton_anon_kb\toff_rss_kb\toff_anon_kb\tdiff_rss_kb\tdiff_anon_kb\n'
for ((pair = 1; pair <= pairs; pair++)); do
    box=$(mktemp -d "${TMPDIR:-/tmp}/mcp-mem.XXXXXX")
    pids=()
    cleanup() {
        for p in "${pids[@]}"; do kill "$p" 2>/dev/null || true; done
        wait 2>/dev/null || true
        rm -rf "$box"
    }
    trap cleanup EXIT
    # the "off" pet's port, held so its bind fails
    python3 -c "import socket,time;s=socket.socket();s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1);s.bind(('127.0.0.1',$port_off));s.listen();time.sleep(10**6)" &
    pids+=($!)
    start() { # name port
        mkdir -p "$box/$1/AiPet"
        printf '{"Pills":true,"OnTop":true,"Avatar":"Sprout","Music":false}\n' >"$box/$1/AiPet/config.json"
        AIPET_PIPE="$box/$1.sock" XDG_DATA_HOME="$box/$1" AIPET_PORT=$2 "$pet" >"$box/$1.log" 2>&1 </dev/null &
        echo $!
    }
    sleep 1
    if ((pair % 2)); then
        on=$(start on "$port_on"); off=$(start off "$port_off")
    else
        off=$(start off "$port_off"); on=$(start on "$port_on")
    fi
    pids+=("$on" "$off")
    sleep 5
    grep -q "mcp: listening on 127.0.0.1:$port_on" "$box/on/AiPet/aipet.log" || { echo "the on pet isn't listening" >&2; exit 1; }
    grep -q "mcp: can't listen" "$box/off/AiPet/aipet.log" || { echo "the off pet is listening" >&2; exit 1; }
    # an agent connected and idle: one MCP session, initialized, one status call, then kept open
    AIPET_DATA_DIR="$box/on/AiPet" AIPET_PORT=$port_on python3 -I "$here/mcp-client.py" hold &
    pids+=($!)
    sleep "$settle"
    row() {
        read -r a b <<<"$(read_mem "$on")"
        read -r c d <<<"$(read_mem "$off")"
        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$pair" "$1" "$a" "$b" "$c" "$d" $((a - c)) $((b - d))
    }
    row idle
    # the same event to both, so both pets show the same state: over HTTP to the "on" pet, through the old exe and
    # the pipe to the "off" one (both trim it the same way before the pet sees it)
    AIPET_PORT=$port_on python3 -I "$here/mcp-client.py" big-body
    AIPET_PORT=$port_on python3 -I "$here/mcp-client.py" big-body-stdout |
        AIPET_PIPE="$box/off.sock" XDG_DATA_HOME="$box/off" "$hook" >/dev/null 2>&1 || true
    for t in 0 10 20 30 40 50 60; do
        row "big+${t}s"
        ((t < 60)) && sleep 10
    done
    cleanup
    trap - EXIT
done
