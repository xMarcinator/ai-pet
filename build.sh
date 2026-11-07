#!/usr/bin/env bash
# Builds AiPet on Linux into ./artifacts and bundles the hook into the plugin: the Linux counterpart of build.ps1.
#
#   ./build.sh                       # this machine's platform (linux-x64 or linux-arm64)
#   ./build.sh linux-x64 win-x64     # the platforms named (win-x64, linux-x64, linux-arm64)
#
# artifacts/<rid>/app    the desktop app (self-contained: no .NET install needed)
# artifacts/<rid>/hook   aipet-hook, built with cargo from rust/crates/aipet-hook: for this machine's platform with
#                        cargo itself; for the other Linux platform with cargo-zigbuild, as the release builds it (for
#                        glibc 2.27 and newer), when cargo-zigbuild and zig are installed. There's no win-x64 hook from
#                        Linux: build.ps1 builds it on Windows.
# plugins/aipet/native/<rid>/   the same hook, next to aipet-hook.sh (gitignored: the release workflow adds the hooks
#                        to the release commit in the plugin repository)
# artifacts/marketplace/ a local marketplace with a copy of the plugin, to try the plugin before a release:
#                        claude --plugin-dir plugins/aipet    (one session), or
#                        claude plugin marketplace add ./artifacts/marketplace && claude plugin install aipet@aipet
#                        codex plugin marketplace add ./artifacts/marketplace && codex plugin add aipet@aipet
#                        (it's named aipet like the real one, so remove that first: ... marketplace remove aipet)
# Needs the .NET 10 SDK, and Rust (rustup) with a C linker (gcc or clang). scripts/install-from-source.sh installs the
# result; releases are built by .github/workflows/release.yml.
set -euo pipefail

root="$(cd "$(dirname "$0")" && pwd)"
step() { printf '\033[36m→ %s\033[0m\n' "$*"; }
warn() { printf '\033[33m! %s\033[0m\n' "$*" >&2; }

host=""
if [ "$(uname -s)" = Linux ]; then
  case "$(uname -m)" in x86_64|amd64) host=linux-x64 ;; aarch64|arm64) host=linux-arm64 ;; esac
fi
rids=("$@")
if [ ${#rids[@]} -eq 0 ]; then
  [ -n "$host" ] || { echo "Name the platforms to build (win-x64, linux-x64, linux-arm64)." >&2; exit 2; }
  rids=("$host")
fi
command -v dotnet >/dev/null || { echo "build.sh needs the .NET 10 SDK (dotnet)." >&2; exit 1; }
command -v cargo >/dev/null || { echo "build.sh needs Rust (cargo, from https://rustup.rs)." >&2; exit 1; }

publish() { # project, output folder, extra arguments...
  local project=$1 out=$2
  shift 2
  dotnet publish "$project" -c Release -o "$out" --nologo -v quiet -p:DebugType=none "$@"
}
cargo_hook() { # cargo command (build or zigbuild), extra arguments...
  local command=$1
  shift
  cargo "$command" -p aipet-hook --release --locked --manifest-path "$root/rust/Cargo.toml" "$@"
}

for rid in "${rids[@]}"; do
  case "$rid" in win-x64|linux-x64|linux-arm64) ;; *) echo "unknown platform: $rid" >&2; exit 2 ;; esac
  step "$rid"
  art="$root/artifacts/$rid"
  rm -rf "$art/app" "$art/hook"

  publish "$root/src/AiPet.UI/AiPet.UI.csproj" "$art/app" -r "$rid" --self-contained

  # the hook runs on every agent event: native, so it starts in a millisecond or two
  hook=""
  if [ "$rid" = "$host" ]; then
    cargo_hook build
    hook="$root/rust/target/release/aipet-hook"
  elif [ "$rid" = win-x64 ]; then
    warn "No win-x64 hook: build.ps1 builds it on Windows"
  elif command -v cargo-zigbuild >/dev/null; then
    triple=x86_64-unknown-linux-gnu
    if [ "$rid" = linux-arm64 ]; then triple=aarch64-unknown-linux-gnu; fi
    if command -v rustup >/dev/null; then rustup target add "$triple" >/dev/null; fi
    cargo_hook zigbuild --target "$triple.2.27"
    hook="$root/rust/target/$triple/release/aipet-hook"
  else
    warn "No $rid hook: building it here needs cargo-zigbuild and zig (cargo install cargo-zigbuild; pip install ziglang)"
  fi

  dest="$root/plugins/aipet/native/$rid"
  rm -rf "$dest"
  if [ -n "$hook" ]; then
    mkdir -p "$art/hook" "$dest"
    cp "$hook" "$art/hook/aipet-hook"
    cp "$hook" "$dest/aipet-hook"
    chmod 755 "$art/hook/aipet-hook" "$dest/aipet-hook"
  fi
done

# A local marketplace with a copy of the plugin and the hooks built so far, to try the plugin before a release.
market="$root/artifacts/marketplace"
rm -rf "$market"
mkdir -p "$market/plugins" "$market/.claude-plugin" "$market/.agents/plugins"
cp -R "$root/plugins/aipet" "$market/plugins/aipet"
cat > "$market/.claude-plugin/marketplace.json" <<'EOF'
{
  "name": "aipet",
  "description": "AiPet, local build (made by build.ps1 or build.sh, for trying the plugin before a release)",
  "owner": { "name": "xMarcinator" },
  "plugins": [
    { "name": "aipet", "source": "./plugins/aipet", "description": "Shows each Claude Code chat's status on the AiPet desktop pet." }
  ]
}
EOF
cat > "$market/.agents/plugins/marketplace.json" <<'EOF'
{
  "name": "aipet",
  "interface": { "displayName": "AiPet (local build)" },
  "plugins": [
    { "name": "aipet", "source": { "source": "local", "path": "./plugins/aipet" }, "policy": { "installation": "AVAILABLE" }, "category": "Productivity" }
  ]
}
EOF

printf '\033[32mDone: %s\033[0m\n' "$root/artifacts"
