#!/usr/bin/env bash
# Checks the AiPet plugin folder that serves both Claude Code and Codex: its manifests and hook files parse, and the
# manifests name the plugin "aipet" and agree on the version. With --hook, the hook files must be exactly what the
# hook code makes of its event tables. With --marketplaces it also checks the two marketplace files at a repository
# root.
#
#   scripts/check-plugin.sh [--marketplaces <repo-root>] [--release] [<plugin-dir>] [--hook <command>...]
#
# <plugin-dir> is plugins/aipet by default.
# --release also requires the hook binaries in native/<rid>/: the release workflow checks the plugin it's about to push.
# --hook takes the rest of the arguments, so it comes last: a command that runs aipet-hook (e.g. --hook dotnet
# <dir>/aipet-hook.dll).
# hooks/hooks.json and hooks/codex.json must be what it prints with --print-plugin-hooks claude and codex, byte for
# byte, which it makes from ClaudeConfig.Events (src/AiPet.Hook/Install.cs) and CodexConfig.PluginEvents
# (src/AiPet.Hook/CodexConfig.cs). Without --hook they're only checked to be JSON.
# Needs jq.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
plugin=""
markets=""
release=0
hook=()
while [ $# -gt 0 ]; do
  case "$1" in
    --marketplaces) markets="${2:?--marketplaces needs a folder}"; shift 2 ;;
    --release) release=1; shift ;;
    --hook)
      hook=("${@:2}")
      if [ ${#hook[@]} -eq 0 ]; then echo "--hook needs a command" >&2; exit 2; fi
      # the hook ignores arguments it doesn't use, so an option of ours after --hook would be lost without a word
      for a in "${hook[@]}"; do
        case "$a" in --marketplaces|--release|--hook|-h|--help) echo "--hook takes the rest of the arguments: put $a before it" >&2; exit 2 ;; esac
      done
      break ;;
    -h|--help) sed -n '2,16p' "$0"; exit 0 ;;
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
# With --hook, each is exactly what the hook prints for it. A Windows checkout may give a file CRLF line endings
# (* text=auto in .gitattributes), but git keeps it with LF, so its CRs don't count; the hook's output must be LF.
if [ ${#hook[@]} -gt 0 ]; then
  printed="$(mktemp -d)"
  trap 'rm -rf "$printed"' EXIT
fi
# Usage: hook_file <agent> <file>
hook_file() {
  local agent=$1 file=$2
  json "$file" || return 0
  if [ ${#hook[@]} -eq 0 ]; then return 0; fi
  if ! "${hook[@]}" --print-plugin-hooks "$agent" > "$printed/$agent.json" 2> "$printed/$agent.err"; then
    fail "${hook[*]} --print-plugin-hooks $agent failed: $(head -n 1 "$printed/$agent.err")"
    return 0
  fi
  tr -d '\r' < "$file" > "$printed/$agent.file"
  if ! diff -u --label "$file" --label "aipet-hook --print-plugin-hooks $agent" "$printed/$agent.file" "$printed/$agent.json" > "$printed/$agent.diff"; then
    fail "$file isn't what aipet-hook --print-plugin-hooks $agent prints (the diff follows)"
    cat "$printed/$agent.diff" >&2
  fi
}
hook_file claude "$plugin/hooks/hooks.json"
hook_file codex "$plugin/hooks/codex.json"

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
# without --hook, OK mustn't read as if the hook files were compared with anything
unchecked=""
if [ ${#hook[@]} -eq 0 ]; then unchecked="; hook files checked as JSON only: pass --hook to compare them with aipet-hook"; fi
echo "Plugin files OK: $plugin${markets:+ (and the marketplaces in $markets)}$unchecked"
