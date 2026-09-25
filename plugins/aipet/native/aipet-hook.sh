#!/bin/sh
# Runs the aipet-hook for this OS (native/<rid>/, next to this script) with the same arguments.
# Anything it can't run (an unsupported OS, a missing binary) is a quiet no-op: the hook must never fail a chat.
dir=$(cd "$(dirname "$0")" && pwd)
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) bin="$dir/win-x64/aipet-hook.exe" ;;
  Linux) case "$(uname -m)" in aarch64|arm64) bin="$dir/linux-arm64/aipet-hook" ;; *) bin="$dir/linux-x64/aipet-hook" ;; esac ;;
  *) exit 0 ;;
esac
[ -f "$bin" ] || exit 0
[ -x "$bin" ] || chmod +x "$bin" 2>/dev/null
exec "$bin" "$@"
