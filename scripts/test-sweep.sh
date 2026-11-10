#!/bin/sh
# Tests of scripts/sweep.sh: sh scripts/test-sweep.sh (Linux: the refusal reads /proc)
# The script under test runs under TEST_SH (default: the sh on PATH), so run it again with TEST_SH=dash or another
# shell /bin/sh may be. CI runs it under dash and bash.
set -u  # not -e: the cases below check exit codes themselves

here=$(cd "$(dirname "$0")" && pwd)
script="$here/sweep.sh"
tmp=$(mktemp -d)
tmp=$(cd "$tmp" && pwd -P)
busy_pid=""
trap '[ -n "$busy_pid" ] && kill "$busy_pid" 2>/dev/null; rm -rf "$tmp"' EXIT

# The script gets a PATH of the tools it uses, plus a fake cargo-sweep or none, so a real one can't leak in.
mkdir -p "$tmp/bin" "$tmp/with" "$tmp/procs" "$tmp/co/rust/target/debug" "$tmp/co-other/rust" "$tmp/bare"
for tool in uname cat readlink ps grep du cut awk sed dirname rm; do
  ln -s "$(command -v "$tool")" "$tmp/bin/$tool"
done
ln -s "$(command -v "${TEST_SH:-sh}")" "$tmp/bin/sh"
ln -s "$(command -v env)" "$tmp/bin/env"
# The fake logs each call and, for --time, removes the stale file as cargo-sweep would.
printf '#!/bin/sh\necho "$PWD: $*" >> "%s/calls"\n[ "$2" = --time ] && rm -f target/debug/stale\nexit 0\n' "$tmp" \
  > "$tmp/with/cargo-sweep"
# A stand-in for a build: a process named cargo that stays in its folder until killed.
printf '#!/bin/sh\nwhile :; do sleep 1; done\n' > "$tmp/procs/cargo"
chmod +x "$tmp/with/cargo-sweep" "$tmp/procs/cargo"
sh_bin="$tmp/bin/sh"
failures=0

check() { # name, expected, actual
  if [ "$2" = "$3" ]; then echo "ok   $1"; return; fi
  echo "FAIL $1"; echo "  want: $2"; echo "  got:  $3"; failures=$((failures + 1))
}

run() { # PATH, then env assignments, then the script's args
  path=$1; shift
  "$tmp/bin/env" -i HOME=/home/tester PATH="$path" "$@"
}

fill() { # a target with a stale file of 2 MiB
  rm -f "$tmp/calls"
  head -c 2097152 /dev/zero > "$tmp/co/rust/target/debug/stale"
}

busy_in() { # folder: start a fake cargo there and wait until /proc names it
  (cd "$1" && exec "$tmp/procs/cargo") &
  busy_pid=$!
  n=0
  while [ "$(cat "/proc/$busy_pid/comm" 2>/dev/null)" != cargo ] && [ $n -lt 50 ]; do sleep 0.1; n=$((n + 1)); done
}

stop_busy() { kill "$busy_pid" 2>/dev/null; wait "$busy_pid" 2>/dev/null; busy_pid=""; }

fill
got=$(run "$tmp/with:$tmp/bin" "$sh_bin" "$script" "$tmp/co" 2>"$tmp/err"; echo "exit $?")
check "sweeps and exits 0" "exit 0" "$(printf '%s' "$got" | tail -n 1)"
check "both sweeps in rust/, --time 7 by default" "$tmp/co/rust: sweep --time 7
$tmp/co/rust: sweep --installed" "$(cat "$tmp/calls")"
check "prints the size before and after" "swept $tmp/co/rust/target: 2 MiB -> 0 MiB, freed 2 MiB" \
  "$(printf '%s' "$got" | sed -n 1p)"
check "no stderr when sweeping" "" "$(cat "$tmp/err")"

fill
run "$tmp/with:$tmp/bin" DAYS=3 "$sh_bin" "$script" "$tmp/co" >/dev/null 2>&1
check "DAYS sets --time" "$tmp/co/rust: sweep --time 3" "$(sed -n 1p "$tmp/calls")"

fill
busy_in "$tmp/co/rust"
got=$(run "$tmp/with:$tmp/bin" "$sh_bin" "$script" "$tmp/co" 2>"$tmp/err"; echo "exit $?")
stop_busy
check "refuses while cargo runs inside the checkout" "exit 1" "$got"
check "the refusal names the checkout" "not sweeping: cargo is running in $tmp/co" "$(cat "$tmp/err")"
check "nothing swept while busy" "no calls" "$(cat "$tmp/calls" 2>/dev/null || echo "no calls")"

fill
busy_in "$tmp/co-other/rust"
got=$(run "$tmp/with:$tmp/bin" "$sh_bin" "$script" "$tmp/co" 2>/dev/null; echo "exit $?")
stop_busy
check "a build in a folder that only shares the prefix doesn't count" "exit 0" "$(printf '%s' "$got" | tail -n 1)"

fill
got=$(cd "$tmp/co" && run "$tmp/with:$tmp/bin" "$sh_bin" "$script" "." 2>/dev/null | sed -n 1p)
check "a relative checkout is resolved" "swept $tmp/co/rust/target: 2 MiB -> 0 MiB, freed 2 MiB" "$got"

got=$(run "$tmp/with:$tmp/bin" DAYS=x "$sh_bin" "$script" "$tmp/co" 2>"$tmp/err"; echo "exit $?")
check "DAYS must be a number" "exit 2" "$got"
check "the DAYS message" "sweep.sh: DAYS must be a whole number of days, not 'x'" "$(cat "$tmp/err")"

got=$(run "$tmp/with:$tmp/bin" "$sh_bin" "$script" "$tmp/bare" 2>"$tmp/err"; echo "exit $?")
check "a folder without rust/ is refused" "exit 2" "$got"
check "the no-rust message" "sweep.sh: $tmp/bare has no rust folder" "$(cat "$tmp/err")"

got=$(run "$tmp/with:$tmp/bin" "$sh_bin" "$script" "$tmp/nowhere" 2>"$tmp/err"; echo "exit $?")
check "a missing folder is refused" "exit 2" "$got"

got=$(run "$tmp/with:$tmp/bin" "$sh_bin" "$script" "$tmp/co-other"; echo "exit $?")
check "no target, nothing to sweep" "nothing to sweep: $tmp/co-other/rust/target doesn't exist
exit 0" "$got"

fill
got=$(run "$tmp/bin" "$sh_bin" "$script" "$tmp/co" 2>"$tmp/err"; echo "exit $?")
check "no cargo-sweep is an error" "exit 2" "$got"
check "the no-cargo-sweep message" \
  "sweep.sh: cargo-sweep not found; mise install, or cargo install cargo-sweep --version 0.8.0 --locked" \
  "$(cat "$tmp/err")"

got=$(run "$tmp/with:$tmp/bin" "$sh_bin" "$script" a b 2>"$tmp/err"; echo "exit $?")
check "two arguments is a usage error" "exit 2" "$got"
got=$(run "$tmp/with:$tmp/bin" "$sh_bin" "$script" --help 2>"$tmp/err"; echo "exit $?")
check "an option is a usage error" "exit 2" "$got"

if [ "$failures" -eq 0 ]; then echo "all passed"; else echo "$failures failed"; exit 1; fi
