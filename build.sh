#!/usr/bin/env bash
# Builds AiPet on Linux into ./artifacts and bundles the hook into the plugin: the Linux counterpart of build.ps1.
#
#   ./build.sh                       # this machine's platform (linux-x64 or linux-arm64)
#   ./build.sh linux-x64 win-x64     # the platforms named (win-x64, linux-x64, linux-arm64)
#
# artifacts/<rid>/app    the desktop app, AiPet (rust/crates/aipet)
# artifacts/<rid>/hook   aipet-hook (rust/crates/aipet-hook)
#                        Both are built with cargo: for this machine's platform with cargo itself; for the other Linux
#                        platform with cargo-zigbuild, as the release builds them (for glibc 2.27 and newer), when
#                        cargo-zigbuild and zig are installed and pkg-config finds that platform's libxkbcommon. There's
#                        no win-x64 build from Linux: build.ps1 builds it on Windows.
# plugins/aipet/native/<rid>/   the same hook, next to aipet-hook.sh (gitignored: the release workflow adds the hooks
#                        to the release commit in the plugin repository)
# artifacts/marketplace/ a local marketplace with a copy of the plugin, to try the plugin before a release:
#                        claude --plugin-dir plugins/aipet    (one session), or
#                        claude plugin marketplace add ./artifacts/marketplace && claude plugin install aipet@aipet
#                        codex plugin marketplace add ./artifacts/marketplace && codex plugin add aipet@aipet
#                        (it's named aipet like the real one, so remove that first: ... marketplace remove aipet)
# Needs Rust (rustup) with a C linker (gcc or clang), and pkg-config with libxkbcommon's development files. scripts/install-from-source.sh installs the
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
command -v cargo >/dev/null || { echo "build.sh needs Rust (cargo, from https://rustup.rs)." >&2; exit 1; }

cargo_build() { # cargo command (build or zigbuild), extra arguments...
  local command=$1
  shift
  cargo "$command" -p aipet -p aipet-hook --release --locked --manifest-path "$root/rust/Cargo.toml" "$@"
}

for rid in "${rids[@]}"; do
  case "$rid" in win-x64|linux-x64|linux-arm64) ;; *) echo "unknown platform: $rid" >&2; exit 2 ;; esac
  step "$rid"
  art="$root/artifacts/$rid"
  rm -rf "$art/app" "$art/hook"

  out=""
  if [ "$rid" = "$host" ]; then
    cargo_build build
    out="$root/rust/target/release"
  elif [ "$rid" = win-x64 ]; then
    warn "No win-x64 build: build.ps1 builds it on Windows"
  elif command -v cargo-zigbuild >/dev/null; then
    triple=x86_64-unknown-linux-gnu
    if [ "$rid" = linux-arm64 ]; then triple=aarch64-unknown-linux-gnu; fi
    if command -v rustup >/dev/null; then rustup target add "$triple" >/dev/null; fi
    # as release.yml links them: the system's libxkbcommon may ask for a newer glibc itself
    RUSTFLAGS="-C link-arg=-Wl,--allow-shlib-undefined" cargo_build zigbuild --target "$triple.2.27"
    out="$root/rust/target/$triple/release"
  else
    warn "No $rid build: building it here needs cargo-zigbuild and zig (cargo install cargo-zigbuild; pip install ziglang)"
  fi
  hook=""
  if [ -n "$out" ]; then
    mkdir -p "$art/app"
    cp "$out/AiPet" "$art/app/AiPet"
    chmod 755 "$art/app/AiPet"
    hook="$out/aipet-hook"
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
