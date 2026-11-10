#!/usr/bin/env bash
# Checks that THIRD-PARTY-NOTICES.md is exactly what cargo-about makes of rust/about.toml and rust/about.hbs, and that
# it gives a license text for every crate the release builds (aipet and aipet-hook, no features) compile in on each
# release target. Exits 1 if not, and prints the difference.
#
#   scripts/check-notices.sh            check
#   scripts/check-notices.sh --write    regenerate THIRD-PARTY-NOTICES.md, then check the crates
#
# Needs cargo-about 0.9.3 (cargo install cargo-about --version 0.9.3 --locked --features cli). It runs offline
# (--frozen), so the crates must be in the local registry already: `cargo fetch --locked` in rust/ fetches them.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

write=0
case "${1:-}" in
  "") ;;
  --write) write=1 ;;
  *) echo "usage: scripts/check-notices.sh [--write]" >&2; exit 2 ;;
esac

about_version=0.9.3
if ! have=$(cargo about --version 2>/dev/null); then
  echo "cargo-about isn't installed: cargo install cargo-about --version $about_version --locked --features cli" >&2
  exit 1
fi
if [ "$have" != "cargo-about $about_version" ]; then
  echo "$have is installed, but the notices are made with cargo-about $about_version" >&2
  exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# The whole file comes from the template. Some crates' license files have CRLF line ends: the notices keep LF.
(cd rust && cargo about generate --frozen --fail -m crates/aipet/Cargo.toml -c about.toml about.hbs) \
  | tr -d '\r' > "$tmp/notices.md"

bad=0
if [ "$write" = 1 ]; then
  cp "$tmp/notices.md" THIRD-PARTY-NOTICES.md
  echo "wrote THIRD-PARTY-NOTICES.md"
elif ! diff -u THIRD-PARTY-NOTICES.md "$tmp/notices.md" > "$tmp/diff"; then
  echo "THIRD-PARTY-NOTICES.md isn't what cargo-about generates (scripts/check-notices.sh --write regenerates it):"
  head -n 200 "$tmp/diff"
  bad=1
fi

# The crates cargo itself builds into the two programs for each target in about.toml's `targets`, against the crates
# the notices give a license text for ("Used by:" lines). cargo-about's graph is rooted at aipet alone (it takes one
# root), so this is also what catches a crate only aipet-hook uses.
targets=$(sed -n 's/^targets *= *\[\(.*\)\]/\1/p' rust/about.toml | tr ',' '\n' | tr -d '" ')
[ -n "$targets" ] || { echo "no targets in rust/about.toml" >&2; exit 1; }
for target in $targets; do
  (cd rust && cargo tree --frozen -p aipet -p aipet-hook -e normal --target "$target" --prefix none -f '{p}')
done | grep -v -e ' (/' -e '^$' | sed 's/ (\*)$//; s/ (proc-macro)$//; s/ v\([^ ]*\)$/ \1/' | sort -u > "$tmp/built"
grep '^Used by: ' "$tmp/notices.md" | sed 's/^Used by: //' | sed 's/, /\n/g' | sort -u > "$tmp/listed"
missing=$(comm -23 "$tmp/built" "$tmp/listed")
if [ -n "$missing" ]; then
  echo "crates the release builds compile in that have no license text in the notices:"
  printf '%s\n' "$missing" | sed 's/^/  /'
  bad=1
fi

if [ "$bad" = 0 ]; then
  echo "THIRD-PARTY-NOTICES.md: up to date, $(wc -l < "$tmp/built") crates, each with its license text"
fi
exit "$bad"
