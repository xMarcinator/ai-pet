"""The agent side of mcp-memory.sh, against the pet on 127.0.0.1:$AIPET_PORT.

    mcp-client.py hold      initialize an MCP session ($AIPET_DATA_DIR's token), call status, then stay connected
    mcp-client.py big-body  POST one Claude hook body just under the 4 MiB cap to /hook/claude
    mcp-client.py big-body-stdout  the same body on stdout, for the exe
"""
import json, os, sys, time, urllib.request

url = "http://127.0.0.1:" + os.environ["AIPET_PORT"]


def post(path, body, headers):
    req = urllib.request.Request(url + path, body, {"Content-Type": "application/json", **headers})
    with urllib.request.urlopen(req, timeout=30) as r:
        return r.status, r.headers, r.read()


if sys.argv[1] == "hold":
    token = open(os.path.join(os.environ["AIPET_DATA_DIR"], "mcp-token")).read().strip()
    h = {"Authorization": "Bearer " + token, "Accept": "application/json, text/event-stream"}
    init = {"jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                       "clientInfo": {"name": "mcp-memory", "version": "0"}}}
    _, headers, _ = post("/mcp", json.dumps(init).encode(), h)
    h["Mcp-Session-Id"] = headers["Mcp-Session-Id"]
    post("/mcp", json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}).encode(), h)
    status, _, _ = post("/mcp", json.dumps({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
                                            "params": {"name": "status", "arguments": {}}}).encode(), h)
    assert status == 200, status
    time.sleep(10**6)
else:
    cap = 4 * 1024 * 1024
    event = {"session_id": "mcp-memory", "hook_event_name": "PreToolUse", "tool_name": "Bash",
             "tool_input": {"command": ""}}
    pad = cap - len(json.dumps(event)) - 64
    event["tool_input"]["command"] = "a" * pad
    body = json.dumps(event).encode()
    if sys.argv[1] == "big-body-stdout":
        sys.stdout.buffer.write(body)
        sys.exit()
    status, _, _ = post("/hook/claude", body, {"Host": "127.0.0.1:" + os.environ["AIPET_PORT"]})
    print(f"big body: {len(body)} bytes, {status}", file=sys.stderr)
