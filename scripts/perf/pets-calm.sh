#!/usr/bin/env bash
# Calm CPU and memory of every built-in Rust pet beside the C# pet (fn-2 task 8), one after another with
# pet-calm.sh: the C# pet first and last (its Sprout, the baseline, so drift shows), each Rust pet in between.
#
#   scripts/perf/pets-calm.sh [rust binary] [C# binary] > results.tsv
# A pet that can't be measured prints its reason and the run goes on.
set -uo pipefail

here=$(dirname "$0")
rust=${1:-rust/target/release/AiPet}
csharp=${2:-$HOME/.local/share/AiPet/app/AiPet}

printf 'label\tcpu_median\tcpu_samples\trss_kb\tpss_kb\n'
"$here/pet-calm.sh" csharp-sprout Sprout -- "$csharp"
while IFS='|' read -r label avatar theme; do
    "$here/pet-calm.sh" "$label" "$avatar" $theme -- "$rust"
done <<'EOF'
sprout|Sprout|
hood|Hood|
hood-green|Hood|Green
hood-vector|Hood (vector)|
pip|Pip|
pip-painted|Pip (painted)|
hamster|Hamster|
hamster-pixel|Hamster (pixel)|
sprout-vector|Sprout (vector)|
EOF
"$here/pet-calm.sh" csharp-sprout-again Sprout -- "$csharp"
