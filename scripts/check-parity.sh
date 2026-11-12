#!/usr/bin/env bash
# Checks rust/PARITY.md, the ledger of the C# tests' Rust equivalents (spec fn-1-migrate-aipet-from-net-to-rust, R16).
# The C# tests went at the cutover (task 25); their rows stay as the record, and are checked against the Rust tests:
#
#   - every row is under its C# file's heading, names its test once, with its attribute and case count (one per
#     [InlineData]);
#   - each row maps to Rust tests, each one a `fn` in the file it names, with at least the C#'s cases, or says why
#     the test no longer applies (n/a: and a reason);
#   - per file, the Rust tests (each counted once) have at least as many cases as the C# tests mapped to them.
#
# Until the cutover it also checked the rows against the C# tests' attributes. Prints each file's counts; exits 1
# with every problem listed. Run from anywhere: scripts/check-parity.sh
set -euo pipefail
cd "$(dirname "$0")/.."

ledger=rust/PARITY.md
[ -f "$ledger" ] || { echo "check-parity: $ledger isn't there" >&2; exit 1; }

# The ledger against the Rust sources.
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
/^## [A-Za-z]+\.cs$/ { area = substr($0, 4); sub(/\.cs$/, "", area); order[++areas_seen] = area; next }
/^\| `/ {
  if (area == "") { problem("a row before any file's heading: " $0); next }
  n = split($0, cell, " \\| ")
  if (n != 5) { problem("a row without its five cells: " $0); next }
  key = cell[1]; sub(/^\| `/, "", key); sub(/`$/, "", key)
  if (cell[2] !~ /^`\[[A-Za-z]*(Fact|Theory)\]`$/) problem(area ".cs: " key ": not a test attribute: " cell[2])
  cs =cell[3] + 0; rust = cell[4]; total = cell[5]; sub(/ \|$/, "", total); total += 0
  id = area SUBSEP key
  if (id in row) problem(area ".cs: " key " has two rows")
  row[id] = 1; rows++
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
  printf "%-22s %9s %9s\n", "file", "C# cases", "Rust"
  for (i = 1; i <= areas_seen; i++) {
    a = order[i]
    printf "%-22s %9d %9d\n", a ".cs", csharp[a], rusts[a]
    if (rusts[a] < csharp[a]) problem(a ".cs: " rusts[a] " Rust cases for the C#'s " csharp[a])
    all_cs += csharp[a]; all_rust += rusts[a]
  }
  printf "%-22s %9d %9d\n", "all", all_cs, all_rust
  printf "%d C# test attributes in the ledger (%d n/a), checked against the Rust tests\n", rows, na
  if (problems) { printf "check-parity: %d problems\n", problems > "/dev/stderr"; exit 1 }
}
AWK

awk "$check" "$ledger"
