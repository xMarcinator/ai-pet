#!/usr/bin/env bash
# Checks that the agent skills Claude Code reads (.claude/skills/) and the copies Codex reads (.agents/skills/) are
# identical, file for file. Prints the files that differ or are only in one tree, and exits 1 if any do.
#
#   scripts/check-skill-copies.sh [<skill>...]
#
# With no arguments it checks every skill under .claude/skills/. To bring a copy in step after editing the Claude
# one: rm -rf .agents/skills/<skill> && cp -r .claude/skills/<skill> .agents/skills/
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

if [ "$#" -gt 0 ]; then
  skills=("$@")
else
  skills=()
  for dir in .claude/skills/*/; do
    [ -d "$dir" ] && skills+=("$(basename "$dir")")
  done
fi

if [ "${#skills[@]}" -eq 0 ]; then
  echo "no skills under .claude/skills/" >&2
  exit 1
fi

bad=0
for skill in "${skills[@]}"; do
  claude=".claude/skills/$skill"
  codex=".agents/skills/$skill"
  if [ ! -d "$claude" ] || [ ! -d "$codex" ]; then
    echo "$skill: missing $([ -d "$claude" ] && echo "$codex" || echo "$claude")"
    bad=1
    continue
  fi
  if ! out=$(diff -rq "$claude" "$codex"); then
    echo "$skill: the two copies differ:"
    printf '%s\n' "$out" | sed 's/^/  /'
    bad=1
  fi
done

if [ "$bad" -eq 0 ]; then
  echo "OK: ${skills[*]} identical in .claude/skills/ and .agents/skills/"
fi
exit "$bad"
