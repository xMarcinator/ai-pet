#!/usr/bin/env bash
# Checks that nothing in the repository still names what the cutover to Rust removed (spec
# fn-1-migrate-aipet-from-net-to-rust, task 25): the C# projects under src/ and tests/, the solution and its build
# props and targets, the golden generators, the icon generator, the Velopack proof's .NET harness, the cross-runtime
# workflow, and the test variables that pointed tests at the C#.
#
# Not checked: CHANGELOG.md (it tells what was removed), .flow/ (the plans and their history), and the golden
# folders rust/crates/*/tests/golden/: the C#'s last outputs, kept byte for byte, and the README that says what wrote
# them and at which commit. Elsewhere the C# is named as its projects name it (AiPet.Core/Ipc.cs), at that commit.
#
# Each pattern is written so that it doesn't match itself ([.] for a dot, [n] for an n). Prints every line found;
# exits 1 if there is one. Run from anywhere: scripts/check-removed-paths.sh
set -euo pipefail
cd "$(dirname "$0")/.."

patterns=(
  'src/AiPet[.]'                 # the projects AiPet.Core, AiPet.UI and AiPet.Hook
  'tests/AiPet[.]Tests'
  'AiPet[.]slnx'
  'Directory[.]Build[.]props'
  'Win32Resources[.]targets'
  'rust/golde[n]'                # the generators; the fixtures are rust/crates/*/tests/golden
  'tools/IconGe[n]'
  'prototypes/velopack-proo[f]'
  'cross-runtime[.]yml'
  'AIPET_TEST_HOO[K]'
  'AIPET_GOLDE[N]'
)
args=()
for p in "${patterns[@]}"; do args+=(-e "$p"); done

if git grep -n -I -E "${args[@]}" -- . ':(exclude)CHANGELOG.md' ':(exclude).flow' \
     ':(glob,exclude)rust/crates/*/tests/golden/**'; then
  echo "check-removed-paths: the lines above name what the cutover removed" >&2
  exit 1
fi
echo "No references to the removed .NET paths (outside CHANGELOG.md, .flow/ and the frozen golden fixtures)"
