#!/usr/bin/env bash
# Checks the AiPet plugin folder that serves both Claude Code and Codex: its manifests and hook files parse, name the
# plugin "aipet", agree on the version, and register exactly the events and handlers the hook code expects. With
# --marketplaces it also checks the two marketplace files at a repository root.
#
#   scripts/check-plugin.sh [--marketplaces <repo-root>] [--release] [<plugin-dir>]     (default: plugins/aipet)
#
# --release also requires the hook binaries in native/<rid>/: the release workflow checks the plugin it's about to push.
# Needs jq.
#
# The expected events are copies of ClaudeConfig.Events (src/AiPet.Hook/Install.cs) and of the Windows set of
# CodexConfig.Events (src/AiPet.Hook/CodexConfig.cs): keep them in step with the code.
# TODO: once aipet-hook can print its own plugin hook files (aipet-hook --print-plugin-hooks claude|codex), compare
# hooks/hooks.json and hooks/codex.json with its output instead, so these copies can go.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
plugin=""
markets=""
release=0
while [ $# -gt 0 ]; do
  case "$1" in
    --marketplaces) markets="${2:?--marketplaces needs a folder}"; shift 2 ;;
    --release) release=1; shift ;;
    -h|--help) sed -n '2,14p' "$0"; exit 0 ;;
    -*) echo "unknown option: $1" >&2; exit 2 ;;
    *) plugin="$1"; shift ;;
  esac
done
plugin="${plugin:-$root/plugins/aipet}"
command -v jq >/dev/null || { echo "check-plugin.sh needs jq" >&2; exit 2; }

errors=0
fail() {
  if [ -n "${GITHUB_ACTIONS:-}" ]; then printf '::error::%s\n' "$*"; else printf 'error: %s\n' "$*" >&2; fi
  errors=$((errors + 1))
}

# (jq reads every file from stdin, since a native jq on Windows can't open paths longer than 260 characters, and its
# output loses CRs, which it adds to every line there.)

# The file exists and is valid JSON.
json() {
  if [ ! -f "$1" ]; then fail "$1 is missing"; return 1; fi
  if ! jq empty < "$1" >/dev/null 2>&1; then fail "$1 is not valid JSON: $(jq empty < "$1" 2>&1 | head -n 1)"; return 1; fi
}

# Runs a jq program over a file; every line it prints is a problem. Usage: check <file> <program> [jq options...]
check() {
  local file=$1 prog=$2 out line
  shift 2
  if ! out=$(jq -r "$@" "$prog" < "$file" 2>&1 | tr -d '\r'); then fail "$file: the check itself failed: $out"; return 0; fi
  while IFS= read -r line; do
    if [ -n "$line" ]; then fail "$file: $line"; fi
  done <<< "$out"
  return 0
}

# A bash list as a JSON array of strings.
json_array() { printf '%s\n' "$@" | jq -R . | jq -cs .; }

# ---------------------------------------------------------------- what the hook code expects
# Claude: ClaudeConfig.Events. Matcher "*" on the tool events; everything async except Stop, which runs in line so
# the turn's final "done" lands last.
claude_events=(SessionStart UserPromptSubmit PreToolUse PostToolUse PostToolUseFailure PermissionRequest PermissionDenied
  Notification Elicitation ElicitationResult PreCompact PostCompact SubagentStart SubagentStop Stop StopFailure SessionEnd)
claude_matcher_events=(PreToolUse PostToolUse PostToolUseFailure PermissionRequest)
claude_inline_events=(Stop)
# Shell form through bash (never PowerShell), with the launcher that picks the binary for the OS. Claude expands
# ${CLAUDE_PLUGIN_ROOT}, not this script.
# shellcheck disable=SC2016
claude_command='sh "${CLAUDE_PLUGIN_ROOT}/native/aipet-hook.sh" --agent claude'

# Codex: the Windows set of CodexConfig.Events (one hook file serves every OS), all async with a 30 s timeout. The
# shell Codex starts expands $PLUGIN_ROOT, not this script.
codex_events=(SessionStart UserPromptSubmit PreToolUse PermissionRequest Stop PreCompact PostCompact SubagentStart SubagentStop)
# shellcheck disable=SC2016
codex_command='sh "$PLUGIN_ROOT/native/aipet-hook.sh" --agent codex'
# Windows runs this with PowerShell: the exe directly, no Git Bash.
codex_command_windows="& (Join-Path \$env:PLUGIN_ROOT 'native\\win-x64\\aipet-hook.exe') --agent codex"

marketplace_url="https://github.com/xMarcinator/ai-pet-plugin.git"

# ---------------------------------------------------------------- manifests
claude_manifest="$plugin/.claude-plugin/plugin.json"
codex_manifest="$plugin/.codex-plugin/plugin.json"
versions=()
for f in "$claude_manifest" "$codex_manifest"; do
  if json "$f"; then
    check "$f" '
      (if .name != "aipet" then "name is \(.name | tojson), expected \"aipet\" (never rename it: Codex trust keys contain aipet@aipet)" else empty end),
      (if (.version | type) != "string" or ((.version | type) == "string" and (.version | test("^[0-9]+\\.[0-9]+\\.[0-9]+(-[0-9A-Za-z.-]+)?$") | not))
       then "version \(.version | tojson) is not a version like 0.2.0" else empty end)'
    versions+=("$(jq -r '.version // ""' < "$f" | tr -d '\r')")
  fi
done
if [ ${#versions[@]} -eq 2 ] && [ "${versions[0]}" != "${versions[1]}" ]; then
  fail "the two plugin.json files disagree on the version (${versions[0]} and ${versions[1]})"
fi
if [ -f "$codex_manifest" ] && jq empty < "$codex_manifest" >/dev/null 2>&1; then
  check "$codex_manifest" 'if .hooks != "./hooks/codex.json" then "hooks is \(.hooks | tojson), expected \"./hooks/codex.json\"" else empty end'
fi

# ---------------------------------------------------------------- hook files
# One group per event with one handler; the group has a matcher only where expected; the handler is exactly $want.
hooks_program='
  (.hooks // {}) as $h
  | ($events[] | select($h[.] == null) | "missing event \(.)"),
    ($h | keys[] | select(. as $e | $events | any(. == $e) | not) | "unexpected event \(.)"),
    ($events[] as $e | select($h[$e] != null) | $h[$e] as $groups
      | ($handler + (if ($inline | any(. == $e)) then {} else {async: true} end)) as $want
      | (if ($matcher | any(. == $e)) then {matcher: "*"} else {} end) as $group
      | if ($groups | type) != "array" or ($groups | length) != 1 then "\($e): expected one group"
        elif ($groups[0].hooks | type) != "array" or ($groups[0].hooks | length) != 1 then "\($e): expected one handler"
        elif ($groups[0] | del(.hooks)) != $group then "\($e): the group is \($groups[0] | del(.hooks) | tojson), expected \($group | tojson)"
        elif $groups[0].hooks[0] != $want then "\($e): the handler is \($groups[0].hooks[0] | tojson), expected \($want | tojson)"
        else empty end)'

claude_hooks="$plugin/hooks/hooks.json"
if json "$claude_hooks"; then
  check "$claude_hooks" "$hooks_program" \
    --argjson events "$(json_array "${claude_events[@]}")" \
    --argjson matcher "$(json_array "${claude_matcher_events[@]}")" \
    --argjson inline "$(json_array "${claude_inline_events[@]}")" \
    --argjson handler "$(jq -n --arg c "$claude_command" '{type: "command", command: $c, shell: "bash", timeout: 5}')"
fi

codex_hooks="$plugin/hooks/codex.json"
if json "$codex_hooks"; then
  check "$codex_hooks" "$hooks_program" \
    --argjson events "$(json_array "${codex_events[@]}")" \
    --argjson matcher '[]' \
    --argjson inline '[]' \
    --argjson handler "$(jq -n --arg c "$codex_command" --arg w "$codex_command_windows" \
      '{type: "command", command: $c, commandWindows: $w, timeout: 30}')"
fi

# ---------------------------------------------------------------- the launcher and the binaries
launcher="$plugin/native/aipet-hook.sh"
if [ ! -f "$launcher" ]; then
  fail "$launcher is missing"
else
  if [ "$(head -c 2 "$launcher")" != "#!" ]; then fail "$launcher doesn't start with #!"; fi
  if [ "$(tr -d '\r' < "$launcher" | wc -c)" != "$(wc -c < "$launcher")" ]; then
    fail "$launcher has CRLF line endings, which sh can't run"
  fi
fi
if [ "$release" = 1 ]; then
  for bin in win-x64/aipet-hook.exe linux-x64/aipet-hook linux-arm64/aipet-hook; do
    f="$plugin/native/$bin"
    if [ ! -s "$f" ]; then fail "$f is missing or empty"
    elif [ ! -x "$f" ]; then fail "$f isn't executable"
    fi
  done
fi

# ---------------------------------------------------------------- marketplaces
if [ -n "$markets" ]; then
  for f in "$markets/.claude-plugin/marketplace.json" "$markets/.agents/plugins/marketplace.json"; do
    if json "$f"; then
      check "$f" '
        (if .name != "aipet" then "the marketplace name is \(.name | tojson), expected \"aipet\" (never rename it)" else empty end),
        ([.plugins[]? | select(.name == "aipet")] as $p
         | if ($p | length) != 1 then "expected one plugin entry named aipet, found \($p | length)"
           else $p[0] as $e | $e.source as $s
             | (if ($s | type) != "object" or $s.source != "git-subdir" then "the aipet entry needs a git-subdir source"
                else
                  (if $s.url != $url then "the source url is \($s.url | tojson), expected \($url | tojson)" else empty end),
                  (if (($s.path // "") | ltrimstr("./")) != "plugins/aipet" then "the source path is \($s.path | tojson), expected \"plugins/aipet\"" else empty end),
                  (if (($s.ref // "") | test("^v[0-9]+\\.[0-9]+\\.[0-9]+")) | not then "the source ref is \($s.ref | tojson), expected a release tag like \"v0.2.0\"" else empty end),
                  (if (($s.sha // "") | test("^[0-9a-f]{40}$")) | not then "the source sha is \($s.sha | tojson), expected a full 40-character lowercase commit sha" else empty end)
                end),
               (if $e | has("version") then "the entry sets a version: leave it to plugin.json" else empty end)
           end)' \
        --arg url "$marketplace_url"
    fi
  done
fi

if [ "$errors" -gt 0 ]; then
  echo "$errors problem(s) in the plugin files" >&2
  exit 1
fi
echo "Plugin files OK: $plugin${markets:+ (and the marketplaces in $markets)}"
