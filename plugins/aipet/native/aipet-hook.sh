#!/bin/sh
# Runs the aipet-hook for this OS and CPU (native/<rid>/, next to this script) with the same arguments and stdin.
# Anything it can't run (an unknown platform, a missing binary, one that can't load) is a quiet no-op that exits 0:
# the hook must never fail a chat. It forks as little as it can: one uname at most (none on Windows), and the one that
# runs the hook (not exec, see below). The time it takes delays the hook's start, which orders Claude's events, and
# each fork is slow on Git Bash.
case $0 in */*) dir=${0%/*} ;; *) dir=. ;; esac
if [ "${OS:-}" = Windows_NT ]; then
  bin=$dir/win-x64/aipet-hook.exe
else
  # only the glibc builds are shipped: without their loader (musl, a 32-bit userland on a 64-bit kernel) they can't run
  case $(uname -sm 2>/dev/null) in
    MINGW*|MSYS*|CYGWIN*) bin=$dir/win-x64/aipet-hook.exe ;;
    "Linux x86_64"|"Linux amd64") [ -e /lib64/ld-linux-x86-64.so.2 ] || exit 0; bin=$dir/linux-x64/aipet-hook ;;
    "Linux aarch64"|"Linux arm64") [ -e /lib/ld-linux-aarch64.so.1 ] || exit 0; bin=$dir/linux-arm64/aipet-hook ;;
    *) exit 0 ;;
  esac
fi
[ -f "$bin" ] || exit 0
[ -x "$bin" ] || chmod +x "$bin" 2>/dev/null
[ -x "$bin" ] || exit 0
# not exec: a binary that still can't load (a glibc older than the one it was built against, or an .exe antivirus
# blocks) ends in a quiet 0 too
"$bin" "$@" 2>/dev/null
exit 0
