#!/bin/sh
# Tests of scripts/cargo-env.sh: sh scripts/test-cargo-env.sh
# The script under test runs under TEST_SH (default: the sh on PATH), so run it again with TEST_SH=dash or another
# shell /bin/sh may be. CI runs it under dash and bash.
set -u  # not -e: the cases below check exit codes themselves

here=$(cd "$(dirname "$0")" && pwd)
script="$here/cargo-env.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
# The script under test gets a PATH of sh and env, plus a fake sccache or none, so a real sccache can't leak in.
mkdir "$tmp/bin" "$tmp/with" "$tmp/broken" "$tmp/real" "$tmp/mise" "$tmp/misefail" "$tmp/miserel" "$tmp/local"
ln -s "$(command -v "${TEST_SH:-sh}")" "$tmp/bin/sh"
ln -s "$(command -v env)" "$tmp/bin/env"
printf '#!/bin/sh\necho "sccache 0.19.0"\n' > "$tmp/with/sccache"
printf '#!/bin/sh\necho "mise ERROR No version is set for shim: sccache" >&2\nexit 1\n' > "$tmp/broken/sccache"
# A shim that has a version only in the caller's folder: it fails from /, where cargo may start the wrapper.
printf '#!/bin/sh\n[ "$PWD" = / ] && { echo "No version is set for shim: sccache" >&2; exit 1; }\necho "sccache 0.19.0"\n' > "$tmp/local/sccache"
printf '#!/bin/sh\necho "sccache 0.19.0"\n' > "$tmp/real/sccache"
printf '#!/bin/sh\n[ "$1 $2" = "which sccache" ] && echo "%s/real/sccache"\n' "$tmp" > "$tmp/mise/mise"
printf '#!/bin/sh\necho "mise ERROR sccache is not a mise bin" >&2\nexit 1\n' > "$tmp/misefail/mise"
printf '#!/bin/sh\necho "sccache"\n' > "$tmp/miserel/mise"
chmod +x "$tmp/with/sccache" "$tmp/broken/sccache" "$tmp/local/sccache" "$tmp/real/sccache" "$tmp/mise/mise" \
  "$tmp/misefail/mise" "$tmp/miserel/mise"
sh_bin="$tmp/bin/sh"
failures=0

check() { # name, expected, actual
  if [ "$2" = "$3" ]; then echo "ok   $1"; else echo "FAIL $1"; echo "  want: $2"; echo "  got:  $3"; failures=$((failures + 1)); fi
}

run() { # PATH, then env assignments, then the script's args
  path=$1; shift
  "$tmp/bin/env" -i HOME=/home/tester PATH="$path" "$@"
}

nl='
'
got=$(run "$tmp/with:$tmp/bin" "$sh_bin" "$script" 2>"$tmp/err")
check "four exports with sccache" "export RUSTC_WRAPPER='$tmp/with/sccache'${nl}export SCCACHE_DIR='/home/tester/.cache/sccache'${nl}export SCCACHE_CACHE_SIZE='15G'${nl}export SCCACHE_ALLOW_INCREMENTAL='1'" "$got"
check "no stderr with sccache" "" "$(cat "$tmp/err")"

got=$(run "$tmp/with:$tmp/bin" "$sh_bin" "$script" --env)
check "--env drops export and quotes" "RUSTC_WRAPPER=$tmp/with/sccache${nl}SCCACHE_DIR=/home/tester/.cache/sccache${nl}SCCACHE_CACHE_SIZE=15G${nl}SCCACHE_ALLOW_INCREMENTAL=1" "$got"

got=$(run "$tmp/with:$tmp/bin" XDG_CACHE_HOME=/xdg "$sh_bin" "$script" --env | sed -n 2p)
check "XDG_CACHE_HOME default" "SCCACHE_DIR=/xdg/sccache" "$got"

got=$(run "$tmp/with:$tmp/bin" SCCACHE_DIR=/mine SCCACHE_CACHE_SIZE=10G "$sh_bin" "$script" --env | sed -n 2,3p)
check "caller values kept" "SCCACHE_DIR=/mine${nl}SCCACHE_CACHE_SIZE=10G" "$got"

got=$(run "$tmp/bin" "$sh_bin" "$script" 2>"$tmp/err"; echo "exit $?")
check "nothing without sccache" "exit 0" "$got"
check "stderr note without sccache" "sccache not found; building without a compiler cache" "$(cat "$tmp/err")"

got=$(run "$tmp/broken:$tmp/bin" "$sh_bin" "$script" 2>"$tmp/err"; echo "exit $?")
check "nothing with an sccache that does not run (a shim without a version)" "exit 0" "$got"
check "stderr note with a broken sccache" "sccache not found; building without a compiler cache" "$(cat "$tmp/err")"

got=$(run "$tmp/with:$tmp/bin" "SCCACHE_DIR=/Users/Jane Doe/cache" "$sh_bin" "$script" | sed -n 2p)
check "a space survives the eval form" "export SCCACHE_DIR='/Users/Jane Doe/cache'" "$got"
got=$(run "$tmp/with:$tmp/bin" "SCCACHE_DIR=/Users/Jane Doe/cache" "$sh_bin" -c "eval \"\$('$sh_bin' '$script')\"; printf '%s' \"\$SCCACHE_DIR\"")
check "the eval form sets the spaced value" "/Users/Jane Doe/cache" "$got"

got=$(run "$tmp/with:$tmp/bin" "SCCACHE_DIR=/a b" "$sh_bin" "$script" --env 2>/dev/null; echo "exit $?")
check "--env refuses whitespace and prints nothing" "exit 2" "$got"

got=$(run "$tmp/with:$tmp/bin" "SCCACHE_DIR=/a*" "$sh_bin" "$script" --env 2>/dev/null; echo "exit $?")
check "--env refuses a glob character" "exit 2" "$got"

got=$(run "$tmp/with:$tmp/bin" "SCCACHE_DIR=/it's" "$sh_bin" "$script" 2>/dev/null; echo "exit $?")
check "a quote is refused and nothing printed" "exit 2" "$got"

got=$(run "$tmp/with:$tmp/bin" "$sh_bin" "$script" --bogus 2>/dev/null; echo "exit $?")
check "a bad argument exits 2" "exit 2" "$got"
got=$(run "$tmp/with:$tmp/bin" "$sh_bin" "$script" --env extra 2>/dev/null; echo "exit $?")
check "an extra argument exits 2" "exit 2" "$got"

# The --env form works as one command's environment.
got=$(run "$tmp/with:$tmp/bin" "$sh_bin" -c "env \$('$sh_bin' '$script' --env) '$sh_bin' -c 'echo \$RUSTC_WRAPPER \$SCCACHE_ALLOW_INCREMENTAL'")
check "--env feeds env(1)" "$tmp/with/sccache 1" "$got"

# The wrapper is the real binary: mise's when mise knows one, else PATH's, and only one that runs from /.
got=$(run "$tmp/mise:$tmp/broken:$tmp/bin" "$sh_bin" "$script" --env | sed -n 1p)
check "mise which wins over a broken shim on PATH" "RUSTC_WRAPPER=$tmp/real/sccache" "$got"
got=$(run "$tmp/misefail:$tmp/with:$tmp/bin" "$sh_bin" "$script" --env | sed -n 1p)
check "a failing mise falls back to PATH" "RUSTC_WRAPPER=$tmp/with/sccache" "$got"
got=$(run "$tmp/miserel:$tmp/with:$tmp/bin" "$sh_bin" "$script" --env | sed -n 1p)
check "a relative mise answer falls back to PATH" "RUSTC_WRAPPER=$tmp/with/sccache" "$got"
got=$(run "$tmp/local:$tmp/bin" "$sh_bin" "$script" 2>"$tmp/err"; echo "exit $?")
check "nothing with a shim that fails outside its folder" "exit 0" "$got"
check "stderr note with a folder-only shim" "sccache not found; building without a compiler cache" "$(cat "$tmp/err")"

# Run as a script (not sourced) under any shell, it prints; dash and ash accept a top-level `return` in a script too.
got=$(cd "$tmp" && run "$tmp/with:$tmp/bin" sh "$script" --env | sed -n 1p)
check "run as a script it prints, whatever the shell" "RUSTC_WRAPPER=$tmp/with/sccache" "$got"

# Sourced, it exports into the calling shell and never ends it: a refusal returns, no set -e or set -u
# leaks into the caller, the caller's positional parameters are not read as arguments, and no helper is left behind.
src() { # PATH, env assignments, then the caller's script after sourcing
  path=$1; shift
  body=$1; shift
  run "$path" "$@" "$sh_bin" -c ". '$script'; echo \"rc=\$?\"; $body" 2>"$tmp/err"
}
got=$(src "$tmp/with:$tmp/bin" 'echo "$RUSTC_WRAPPER $SCCACHE_DIR $SCCACHE_CACHE_SIZE $SCCACHE_ALLOW_INCREMENTAL"')
check "sourced exports into the shell" "rc=0${nl}$tmp/with/sccache /home/tester/.cache/sccache 15G 1" "$got"
got=$(src "$tmp/with:$tmp/bin" "'$sh_bin' -c 'echo \$RUSTC_WRAPPER \$SCCACHE_ALLOW_INCREMENTAL'")
check "sourced values are exported" "rc=0${nl}$tmp/with/sccache 1" "$got"
got=$(src "$tmp/with:$tmp/bin" 'case $- in *e*|*u*) echo leaked ;; *) echo clean ;; esac; echo "$undefined_var" after')
check "sourced leaks no shell option" "rc=0${nl}clean${nl} after" "$got"
got=$(src "$tmp/with:$tmp/bin" 'command -v cargo_env_print; echo "${cargo_env_sourced-}${cargo_env_file-}${cargo_env_here-}${cargo_env_out-}${cargo_env_status-}gone"')
check "sourced leaves no helper behind" "rc=0${nl}gone" "$got"
got=$(src "$tmp/with:$tmp/bin" 'echo "${RUSTC_WRAPPER-unset}" still running' "SCCACHE_DIR=/it's")
check "a sourced refusal returns 2 and the shell goes on" "rc=2${nl}unset still running" "$got"
got=$(src "$tmp/bin" 'echo "${RUSTC_WRAPPER-unset}" still running')
check "sourced without sccache returns 0 and the shell goes on" "rc=0${nl}unset still running" "$got"
check "sourced note without sccache" "sccache not found; building without a compiler cache" "$(cat "$tmp/err")"
got=$(run "$tmp/with:$tmp/bin" "$sh_bin" -c "set -- --bogus extra; . '$script'; echo \"rc=\$? \$RUSTC_WRAPPER\"")
check "sourced ignores the caller's positional parameters" "rc=0 $tmp/with/sccache" "$got"
got=$(run "$tmp/with:$tmp/bin" "SCCACHE_DIR=/it's" "$sh_bin" -c "trap 'echo \"exit \$? \${cargo_env_out-clean}\"' EXIT; set -e; . '$script'; echo not reached" 2>/dev/null)
check "a sourced refusal under set -e cleans up before the caller's set -e ends it" "exit 2 clean" "$got"
case $("$sh_bin" -c 'echo "${BASH_VERSION:-}${ZSH_VERSION:-}"') in
  "") ;; # dash, ash: a sourced file is not named (the checkout root is assumed) and a failing `.` ends set -e
  *)
    # dash and ash end a set -e shell on any failing `.`, even inside `||`; bash and zsh do not.
    got=$(run "$tmp/with:$tmp/bin" "SCCACHE_DIR=/it's" "$sh_bin" -c "set -e; . '$script' || echo \"rc=\$?\"; echo still running" 2>/dev/null)
    check "a sourced refusal under the caller's set -e returns" "rc=2${nl}still running" "$got"
    got=$(cd "$tmp" && run "$tmp/mise:$tmp/broken:$tmp/bin" "$sh_bin" -c ". '$script'; echo \"\$RUSTC_WRAPPER\"")
    check "sourced from another folder still asks mise" "$tmp/real/sccache" "$got" ;;
esac

[ "$failures" -eq 0 ] || { echo "$failures failed"; exit 1; }
echo "all passed"
