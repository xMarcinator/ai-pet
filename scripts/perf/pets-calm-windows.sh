#!/usr/bin/env bash
# Calm CPU and memory of every built-in Rust pet beside the C# pet on Windows (fnt-76 task 2), from Git Bash: the
# Windows twin of pets-calm.sh, one after another with pet-calm-windows.sh: the C# pet first and last (its Sprout, the
# baseline, so drift shows), each Rust pet in between.
#
#   scripts/perf/pets-calm-windows.sh [rust exe] [C# exe] > results.tsv
# A pet that can't be measured prints its reason and the run goes on.
set -uo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
exe=; [[ ${OS:-} == Windows_NT ]] && exe=.exe
rust=${1:-$root/rust/target/release/AiPet$exe}
csharp=${2:-${LOCALAPPDATA:-}/AiPet/current/AiPet.exe}

printf 'label\tcpu_median\tcpu_samples\tworking_set_kb\tprivate_kb\n'
"$here/pet-calm-windows.sh" csharp-sprout Sprout -- "$csharp"
while IFS='|' read -r label avatar theme; do
    # shellcheck disable=SC2086
    "$here/pet-calm-windows.sh" "$label" "$avatar" $theme -- "$rust"
done <<'LIST'
sprout|Sprout|
hood|Hood|
hood-green|Hood|Green
hood-vector|Hood (vector)|
pip|Pip|
pip-painted|Pip (painted)|
sprout-vector|Sprout (vector)|
LIST
"$here/pet-calm-windows.sh" csharp-sprout-again Sprout -- "$csharp"
