#!/bin/sh
# The MCP token's header for Codex, which runs its header helper with no arguments: the launcher next to this script
# with --print-token-header. Prints {} (or nothing) when the hook can't run, and always exits 0.
case $0 in */*) dir=${0%/*} ;; *) dir=. ;; esac
sh "$dir/aipet-hook.sh" --print-token-header
exit 0
