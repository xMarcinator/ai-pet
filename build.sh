#!/usr/bin/env bash
# Builds AiPet on Linux into ./artifacts and bundles the hook into the plugin: the Linux counterpart of build.ps1.
#
#   ./build.sh                       # this machine's platform (linux-x64 or linux-arm64)
#   ./build.sh linux-x64 win-x64     # the platforms named (win-x64, linux-x64, linux-arm64)
#
# artifacts/<rid>/app    the desktop app (self-contained: no .NET install needed)
# artifacts/<rid>/hook   aipet-hook: native (NativeAOT) for this machine's platform, which needs clang and zlib1g-dev
#                        (sudo apt install clang zlib1g-dev); otherwise a trimmed self-contained single file, since
#                        NativeAOT can't cross-compile
# plugins/aipet/native/<rid>/   the same hook, next to aipet-hook.sh (gitignored: the release workflow adds the hooks
#                        to the release commit in the plugin repository)
# artifacts/marketplace/ a local marketplace with a copy of the plugin, to try the plugin before a release:
#                        claude --plugin-dir plugins/aipet    (one session), or
#                        claude plugin marketplace add ./artifacts/marketplace && claude plugin install aipet@aipet
#                        codex plugin marketplace add ./artifacts/marketplace && codex plugin add aipet@aipet
#                        (it's named aipet like the real one, so remove that first: ... marketplace remove aipet)
# Needs the .NET 10 SDK. scripts/install-from-source.sh installs the result; releases are built by
# .github/workflows/release.yml.
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

publish() { # project, output folder, extra arguments...
  local project=$1 out=$2
  shift 2
  dotnet publish "$project" -c Release -o "$out" --nologo -v quiet -p:DebugType=none "$@"
}
single_file_hook() { # output folder, rid
  publish "$root/src/AiPet.Hook/AiPet.Hook.csproj" "$1" -r "$2" --self-contained \
    -p:PublishAot=false -p:PublishSingleFile=true -p:PublishTrimmed=true
}

for rid in "${rids[@]}"; do
  case "$rid" in win-x64|linux-x64|linux-arm64) ;; *) echo "unknown platform: $rid" >&2; exit 2 ;; esac
  step "$rid"
  art="$root/artifacts/$rid"
  rm -rf "$art/app" "$art/hook"

  publish "$root/src/AiPet.UI/AiPet.UI.csproj" "$art/app" -r "$rid" --self-contained

  if [ "$rid" = "$host" ] && command -v clang >/dev/null; then
    # native (AOT): starts in a few milliseconds, runs on every agent event (PublishAot is set in the project)
    if ! publish "$root/src/AiPet.Hook/AiPet.Hook.csproj" "$art/hook" -r "$rid" -p:StripSymbols=true; then
      warn "The native build of the hook failed (zlib1g-dev missing?); building a single-file hook instead"
      rm -rf "$art/hook"
      single_file_hook "$art/hook" "$rid"
    fi
  else
    if [ "$rid" = "$host" ]; then
      warn "clang isn't installed (sudo apt install clang zlib1g-dev), so the hook is a single file instead of native"
    fi
    single_file_hook "$art/hook" "$rid"
  fi
  rm -f "$art/hook"/*.dbg "$art/hook"/*.pdb

  dest="$root/plugins/aipet/native/$rid"
  rm -rf "$dest"
  mkdir -p "$dest"
  cp "$art/hook"/aipet-hook* "$dest/"
  chmod 755 "$dest"/aipet-hook*
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
