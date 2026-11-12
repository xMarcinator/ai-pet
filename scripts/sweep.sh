#!/bin/sh
# Sweeps stale build output from a checkout's rust/target with cargo-sweep. Cargo never deletes old output: every
# change of features, profile, flags or toolchain writes new hashed copies beside the old ones. Run it after a gate
# (the checks before a commit, the end of a task or a wave), in the checkout or worktree that ran the gate.
#
#   scripts/sweep.sh              this repository
#   scripts/sweep.sh <checkout>   another checkout or worktree of it
#   DAYS=3 scripts/sweep.sh       keep output used in the last 3 days (default 7)
#
# It refuses, with exit 1, while a cargo, rustc, rustdoc or clippy-driver process works inside the checkout (its
# working directory, from /proc or lsof): sweeping under a build breaks the build. On Windows (Git Bash) a process's
# working directory can't be read, so any running cargo or rustc counts. Else it runs `cargo-sweep sweep --time DAYS`
# (output not used for DAYS days) and `cargo-sweep sweep --installed` (output of toolchains no longer installed) in
# <checkout>/rust and prints the target's size before and after. It never touches another checkout's target.
# cargo-sweep: `mise install` (mise.toml pins it), or `cargo install cargo-sweep --version 0.8.0 --locked`.
# Its tests: sh scripts/test-sweep.sh
set -eu

usage() { echo "usage: [DAYS=n] scripts/sweep.sh [checkout]" >&2; exit 2; }
case "$#:${1:-}" in
  0:) checkout=$(cd "$(dirname "$0")/.." && pwd -P) ;;
  1:-*) usage ;;
  1:*) [ -d "$1" ] || { echo "sweep.sh: no such folder: $1" >&2; exit 2; }; checkout=$(cd "$1" && pwd -P) ;;
  *) usage ;;
esac
days=${DAYS:-7}
case "$days" in ''|*[!0-9]*) echo "sweep.sh: DAYS must be a whole number of days, not '$days'" >&2; exit 2 ;; esac
[ -d "$checkout/rust" ] || { echo "sweep.sh: $checkout has no rust folder" >&2; exit 2; }

# The working directories of the build processes, one per line; "windows" when they can't be read.
build_cwds() {
  case "$(uname -s)" in
    MINGW* | MSYS* | CYGWIN*)
      if ps -W | grep -Eiq '[\\/](cargo|rustc)\.exe$'; then echo windows; fi
      return 0 ;;
  esac
  if [ -d /proc/self ]; then
    for p in /proc/[0-9]*; do
      comm=$(cat "$p/comm" 2>/dev/null) || continue
      case "$comm" in
        cargo | rustc | rustdoc | clippy-driver) readlink "$p/cwd" 2>/dev/null || true ;;
      esac
    done
  elif command -v lsof >/dev/null 2>&1; then
    lsof -a -d cwd -c cargo -c rustc -c rustdoc -c clippy-driver -Fn 2>/dev/null | sed -n 's/^n//p'
  fi
}

busy=$(build_cwds | while IFS= read -r cwd; do
  case "$cwd" in windows | "$checkout" | "$checkout"/*) echo "$cwd"; break ;; esac
done)
case "$busy" in
  "") ;;
  windows)
    echo "not sweeping: cargo is running (on Windows its folder can't be read, so any build counts)" >&2
    exit 1 ;;
  *) echo "not sweeping: cargo is running in $checkout" >&2; exit 1 ;;
esac

target="$checkout/rust/target"
if [ ! -d "$target" ]; then
  echo "nothing to sweep: $target doesn't exist"
  exit 0
fi
if ! command -v cargo-sweep >/dev/null 2>&1; then
  echo "sweep.sh: cargo-sweep not found; mise install, or cargo install cargo-sweep --version 0.8.0 --locked" >&2
  exit 2
fi

kib() { du -sk "$1" | cut -f1; }
human() { # KiB
  awk -v k="$1" 'BEGIN { if (k >= 1048576) printf "%.1f GiB", k / 1048576; else printf "%.0f MiB", k / 1024 }'
}

before=$(kib "$target")
cd "$checkout/rust"
cargo-sweep sweep --time "$days"
cargo-sweep sweep --installed
after=$(kib "$target")
echo "swept $target: $(human "$before") -> $(human "$after"), freed $(human $((before - after)))"
