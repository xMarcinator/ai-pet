#!/usr/bin/env bash
# The agent side of the MCP measurements, with curl only (Git Bash has no Python): the Windows twin of mcp-client.py,
# against the pet on 127.0.0.1:$AIPET_PORT (47615 by default).
#
#   mcp-client.sh hold      an MCP session as an agent opens one: initialize with $AIPET_DATA_DIR's token, initialized,
#                           one status call; the pet keeps the session after this returns, as it does between an
#                           agent's calls
#   mcp-client.sh big-body  one Claude hook body just under the 4 MiB cap, POSTed to /hook/claude
#   mcp-client.sh big-body-stdout   the same body on stdout, for the old exe and its pipe
set -euo pipefail

url=http://127.0.0.1:${AIPET_PORT:-47615}
body() {
    # {"session_id":..,"tool_input":{"command":"aaa…"}} padded to 4 MiB less 64 bytes
    local head='{"session_id": "mcp-memory", "hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_input": {"command": "'
    local tail='"}}'
    local pad=$((4 * 1024 * 1024 - ${#head} - ${#tail} - 64))
    printf '%s' "$head"
    head -c "$pad" /dev/zero | tr '\0' a
    printf '%s' "$tail"
}

case ${1:-} in
hold)
    token=$(tr -d '\r\n' <"$AIPET_DATA_DIR/mcp-token")
    auth="Authorization: Bearer $token"
    accept='Accept: application/json, text/event-stream'
    headers=$(mktemp)
    trap 'rm -f "$headers"' EXIT
    curl -sS -o /dev/null -D "$headers" -H "$auth" -H "$accept" -H 'Content-Type: application/json' \
        --data '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"mcp-memory","version":"0"}}}' \
        "$url/mcp"
    session=$(tr -d '\r' <"$headers" | awk -F': ' 'tolower($1) == "mcp-session-id" { print $2 }')
    [[ -n $session ]] || { echo "no MCP session from $url" >&2; exit 1; }
    curl -sS -o /dev/null -H "$auth" -H "$accept" -H 'Content-Type: application/json' -H "Mcp-Session-Id: $session" \
        --data '{"jsonrpc":"2.0","method":"notifications/initialized"}' "$url/mcp"
    code=$(curl -sS -o /dev/null -w '%{http_code}' -H "$auth" -H "$accept" -H 'Content-Type: application/json' \
        -H "Mcp-Session-Id: $session" \
        --data '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"status","arguments":{}}}' "$url/mcp")
    [[ $code == 200 ]] || { echo "status answered $code" >&2; exit 1; }
    ;;
big-body)
    file=$(mktemp)
    trap 'rm -f "$file"' EXIT
    body >"$file"
    code=$(curl -sS -o /dev/null -w '%{http_code}' -H 'Content-Type: application/json' --data-binary "@$file" \
        "$url/hook/claude")
    echo "big body: $(wc -c <"$file") bytes, $code" >&2
    ;;
big-body-stdout) body ;;
*)
    echo "usage: $0 hold | big-body | big-body-stdout" >&2
    exit 2
    ;;
esac
