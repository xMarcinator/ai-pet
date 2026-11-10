#!/usr/bin/env bash
# Checks rust/PARITY.md, the ledger of the C# tests' Rust equivalents (spec fn-1-migrate-aipet-from-net-to-rust, R16):
#
#   - every test attribute in tests/AiPet.Tests/*.cs ([Fact], [Theory] and the platform ones) has one row, under its
#     file's heading, with its case count (one per [InlineData]; a [MemberData] theory's count is the ledger's), and
#     no row names a test that isn't there any more;
#   - each row maps to Rust tests, each one a `fn` in the file it names, with at least the C#'s cases, or says why
#     the test no longer applies (n/a: and a reason);
#   - per file, the Rust tests (each counted once) have at least as many cases as the C# tests mapped to them.
#
# Once the C# tests are gone (the cutover), the ledger is checked against the Rust tests alone. Prints each file's
# counts; exits 1 with every problem listed. Run from anywhere: scripts/check-parity.sh
set -euo pipefail
cd "$(dirname "$0")/.."

ledger=rust/PARITY.md
tests=tests/AiPet.Tests
[ -f "$ledger" ] || { echo "check-parity: $ledger isn't there" >&2; exit 1; }

# The C# tests: "<file> <Class.Method> <attribute> <cases>" ("MD" for a [MemberData] theory). A test's class is the
# innermost class declared less indented than its attribute (helper classes nest deeper, or end before the tests).
read -r -d '' extract <<'AWK' || true
function indent(s) { match(s, /^[ \t]*/); return RLENGTH }
/^[ \t]*\/\// { next }
/(^|[ \t])class [A-Za-z_0-9]+/ && !/Attribute[ \t]*:/ {
  k = indent($0); match($0, /class [A-Za-z_0-9]+/); name = substr($0, RSTART + 6, RLENGTH - 6)
  for (i in cls) if (i + 0 > k) delete cls[i]
  cls[k] = name
}
/^[ \t]*\[[A-Za-z]*(Fact|Theory)[]( ]/ {
  pending = 1; n = 0; member = 0; at = indent($0)
  attribute = $0; sub(/^[ \t]*\[/, "", attribute); sub(/[]( ].*/, "", attribute)
  best = -1; for (i in cls) if (i + 0 < at && i + 0 > best) best = i + 0
  owner = cls[best]; next
}
pending && /^[ \t]*\[InlineData/ { n++; next }
pending && /^[ \t]*\[MemberData/ { member = 1; next }
pending && /^[ \t]*\[/ { next }
pending && match($0, /(void|Task)[ \t]+[A-Za-z_0-9]+[ \t]*\(/) {
  s = substr($0, RSTART, RLENGTH); sub(/^(void|Task)[ \t]+/, "", s); sub(/[ \t]*\($/, "", s)
  print area, owner "." s, attribute, (n ? n : (member ? "MD" : 1)); pending = 0
}
AWK

# The ledger against the inventory and the Rust sources.
read -r -d '' check <<'AWK' || true
function problem(text) { problems++; print "check-parity: " text > "/dev/stderr" }
# whether the Rust file (under rust/) has `fn <name>(`
function has_fn(file, name,    line, found) {
  if (!(file in read_)) {
    read_[file] = 1
    if ((getline line < ("rust/" file)) < 0) { missing_file[file] = 1; return 0 }
    do { text[file] = text[file] "\n" line } while ((getline line < ("rust/" file)) > 0)
    close("rust/" file)
  }
  if (file in missing_file) return 0
  return index(text[file], "fn " name "(") > 0
}
FILENAME == ARGV[1] { want[$1 SUBSEP $2] = $4; attr[$1 SUBSEP $2] = $3; areas[$1] = 1; next }
/^## [A-Za-z]+\.cs$/ { area = substr($0, 4); sub(/\.cs$/, "", area); order[++areas_seen] = area; next }
/^\| `/ {
  if (area == "") { problem("a row before any file's heading: " $0); next }
  n = split($0, cell, " \\| ")
  if (n != 5) { problem("a row without its five cells: " $0); next }
  key = cell[1]; sub(/^\| `/, "", key); sub(/`$/, "", key)
  attribute = cell[2]; gsub(/[`\[\]]/, "", attribute)
  cs = cell[3] + 0; rust = cell[4]; total = cell[5]; sub(/ \|$/, "", total); total += 0
  id = area SUBSEP key
  if (id in row) problem(area ".cs: " key " has two rows")
  row[id] = 1; rows++
  if (have_inventory) {
    if (!(id in want)) problem(area ".cs: " key " is in the ledger, but not in " tests)
    else {
      if (attr[id] != attribute) problem(area ".cs: " key " is [" attr[id] "], not [" attribute "]")
      if (want[id] != "MD" && want[id] + 0 != cs) problem(area ".cs: " key " has " want[id] " cases, not " cs)
    }
  }
  if (cs < 1) problem(area ".cs: " key " has no cases")
  if (rust ~ /^n\/a: /) {
    if (length(rust) < 20) problem(area ".cs: " key ": n/a needs a reason")
    if (total != 0) problem(area ".cs: " key ": an n/a row has no Rust cases")
    na++; next
  }
  csharp[area] += cs
  refs = split(rust, ref, "<br>")
  sum = 0
  for (i = 1; i <= refs; i++) {
    r = ref[i]
    if (!match(r, /^`[^`]+` \([0-9]+\)$/)) { problem(area ".cs: " key ": not `<file>::<fn>` (<cases>): " r); continue }
    path = r; sub(/^`/, "", path); sub(/` \(.*$/, "", path)
    count = r; sub(/^.*\(/, "", count); sub(/\)$/, "", count); count += 0
    file = path; sub(/::.*$/, "", file)
    name = path; sub(/^.*::/, "", name)
    if (file == path || name == "") { problem(area ".cs: " key ": " path " names no fn"); continue }
    if (!has_fn(file, name)) problem(area ".cs: " key ": rust/" file " has no fn " name)
    if ((path in cases) && cases[path] != count) problem(path " has " cases[path] " cases in one row and " count " in another")
    cases[path] = count
    if (!((area, path) in counted)) { counted[area, path] = 1; rusts[area] += count }
    sum += count
  }
  if (sum != total) problem(area ".cs: " key ": its Rust tests have " sum " cases, not " total)
  if (sum < cs) problem(area ".cs: " key ": " sum " Rust cases for the C#'s " cs)
  next
}
END {
  if (have_inventory) {
    for (id in want) if (!(id in row)) {
      split(id, part, SUBSEP)
      problem(part[1] ".cs: " part[2] " [" attr[id] "] isn't in the ledger")
    }
  }
  printf "%-22s %9s %9s\n", "file", "C# cases", "Rust"
  for (i = 1; i <= areas_seen; i++) {
    a = order[i]
    printf "%-22s %9d %9d\n", a ".cs", csharp[a], rusts[a]
    if (rusts[a] < csharp[a]) problem(a ".cs: " rusts[a] " Rust cases for the C#'s " csharp[a])
    all_cs += csharp[a]; all_rust += rusts[a]
  }
  printf "%-22s %9d %9d\n", "all", all_cs, all_rust
  printf "%d C# test attributes in the ledger (%d n/a)%s\n", rows, na, have_inventory ? ", every one of " tests "'s" : ""
  if (problems) { printf "check-parity: %d problems\n", problems > "/dev/stderr"; exit 1 }
}
AWK

inventory=$(mktemp)
trap 'rm -f "$inventory"' EXIT
have=0
if [ -d "$tests" ]; then
  have=1
  for f in "$tests"/*.cs; do
    awk -v area="$(basename "$f" .cs)" "$extract" "$f"
  done | LC_ALL=C sort > "$inventory"
  if [ ! -s "$inventory" ]; then echo "check-parity: no tests found in $tests" >&2; exit 1; fi
else
  echo "check-parity: $tests is gone, so the ledger is checked against the Rust tests only"
fi
awk -v have_inventory="$have" -v tests="$tests" "$check" "$inventory" "$ledger"
