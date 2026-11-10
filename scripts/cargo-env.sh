#!/bin/sh
# The build environment for cargo in rust/: sccache as the compiler wrapper, so every checkout and worktree on this
# machine shares one cache of compiled crates. It comes from the environment only, never from a checked-in
# .cargo/config.toml, so a machine or CI runner without sccache builds as before.
#
#   eval "$(scripts/cargo-env.sh)"                one shell
#   . scripts/cargo-env.sh                        one shell, sourced (from the repository root in dash and ash)
#   env $(scripts/cargo-env.sh --env) cargo ...   one command
#
# With a working sccache it prints RUSTC_WRAPPER (the binary's absolute path), SCCACHE_DIR (~/.cache/sccache),
# SCCACHE_CACHE_SIZE (15G) and SCCACHE_ALLOW_INCREMENTAL=1; a SCCACHE_DIR or SCCACHE_CACHE_SIZE you set yourself is
# kept. The binary is `mise which sccache` (mise.toml pins it) or else the sccache on PATH, and it must run from /:
# cargo starts the wrapper in other folders, where a mise shim has no version. Without one it prints nothing, says so
# on stderr and exits 0. Sourced, it never exits or changes the options of the calling shell, and takes no arguments.
# Its tests: sh scripts/test-cargo-env.sh

# The whole script runs in this subshell function, so `set -eu` and `exit` stay inside it, sourced or not. $1 is the
# repository's scripts folder.
cargo_env_print() (
set -eu
here=$1
shift

env_form=false
case "$#:${1:-}" in
  0:) ;;
  1:--env) env_form=true ;;
  *) echo "usage: scripts/cargo-env.sh [--env]" >&2; exit 2 ;;
esac

resolve_sccache() {
  found=""
  if command -v mise >/dev/null 2>&1; then
    found=$(cd "$here/.." && mise which sccache 2>/dev/null) || found=""
  fi
  case "$found" in /*) ;; *) found=$(command -v sccache 2>/dev/null) || found="" ;; esac
  case "$found" in /*) ;; *) return 0 ;; esac
  if (cd / && "$found" --version) >/dev/null 2>&1; then printf '%s' "$found"; fi
}

wrapper=$(resolve_sccache)
if [ -z "$wrapper" ]; then
  echo "sccache not found; building without a compiler cache" >&2
  exit 0
fi

# the same defaults as flow-next-tracker's, so both share one cache
dir="${SCCACHE_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/sccache}"
size="${SCCACHE_CACHE_SIZE:-15G}"

nl='
'
emit() { # name, value
  if $env_form; then
    # the caller's shell splits and globs the --env words
    case "$2" in
      *[[:space:]*?[]*|"")
        echo "cargo-env.sh: --env can't carry '$2' (whitespace or a glob character); use eval" >&2
        exit 2 ;;
    esac
    printf '%s=%s\n' "$1" "$2"
  else
    case "$2" in
      *"'"*|*"$nl"*) echo "cargo-env.sh: $1 has a quote or a newline" >&2; exit 2 ;;
    esac
    printf "export %s='%s'\n" "$1" "$2"
  fi
}

# every value checked before any is printed, so a refusal prints nothing
out=$(emit RUSTC_WRAPPER "$wrapper" && emit SCCACHE_DIR "$dir" && emit SCCACHE_CACHE_SIZE "$size" &&
  emit SCCACHE_ALLOW_INCREMENTAL 1) || exit $?
printf '%s\n' "$out"
)

# Sourced or run: bash names this file in BASH_SOURCE, zsh marks a sourced file in ZSH_EVAL_CONTEXT, and in other
# shells $0 names this file only when it runs as a script.
cargo_env_sourced=false
if [ -n "${BASH_SOURCE:-}" ]; then
  [ "${BASH_SOURCE}" != "$0" ] && cargo_env_sourced=true
elif [ -n "${ZSH_EVAL_CONTEXT:-}" ]; then
  case "$ZSH_EVAL_CONTEXT" in *:file*) cargo_env_sourced=true ;; esac
else
  case "${0##*/}" in cargo-env.sh) ;; *) cargo_env_sourced=true ;; esac
fi

if $cargo_env_sourced; then
  cargo_env_file=${BASH_SOURCE:-${ZSH_VERSION:+$0}}
  case "$cargo_env_file" in
    */*) cargo_env_here=${cargo_env_file%/*} ;;
    "") cargo_env_here=scripts ;; # dash and ash don't name it: the repository root is assumed
    *) cargo_env_here=. ;;
  esac
  # The status rides on the output's last line, outside any `if` (set -e is off in a condition) and with set +e, so
  # a caller's set -e can't end the substitution before the cleanup below.
  cargo_env_out=$(set +e; cargo_env_print "$cargo_env_here"; echo "$?")
  cargo_env_status=${cargo_env_out##*
}
  cargo_env_out=${cargo_env_out%"$cargo_env_status"}
  [ "$cargo_env_status" -eq 0 ] && eval "$cargo_env_out"
  unset -f cargo_env_print
  unset cargo_env_sourced cargo_env_file cargo_env_here cargo_env_out
  eval "unset cargo_env_status; return $cargo_env_status"
fi

case "$0" in */*) cargo_env_here=${0%/*} ;; *) cargo_env_here=. ;; esac
cargo_env_print "$cargo_env_here" "$@"
exit $?
