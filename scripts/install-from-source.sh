#!/usr/bin/env bash
# AiPet installer for Linux, from a clone of the repository: installs a build from source. End users install a
# release with the one-line installer (install.sh in the repository root) instead.
#
#   ./scripts/install-from-source.sh               install the app, register hooks for Claude Code and Codex, start the pet
#   ./scripts/install-from-source.sh --no-hooks    just the app
#   ./scripts/install-from-source.sh --rebuild     build even if the last build looks current
#
# Uses artifacts/<rid> from build.sh or build.ps1; rebuilds them first when they're missing or older than the source
# (that needs the .NET 10 SDK).
# App:   ~/.local/share/AiPet/app      (+ menu entry)
# Hook:  ~/.local/share/AiPet/hooks/aipet-hook, registered in ~/.claude/settings.json and ~/.codex/config.toml
#        (Codex then asks you to trust it: run `codex` once and choose Review hooks; see aipet-hook --doctor codex)
# These registrations stand in for the plugins: don't use both, or every event reaches the pet twice.
set -euo pipefail

HOOKS=1; START=1; REBUILD=0
for a in "$@"; do
  case "$a" in
    --rebuild) REBUILD=1 ;;
    --no-hooks) HOOKS=0 ;;
    --no-start) START=0 ;;
    *) echo "unknown option: $a" >&2; exit 2 ;;
  esac
done

root="$(cd "$(dirname "$0")/.." && pwd)"
case "$(uname -m)" in aarch64|arm64) rid=linux-arm64 ;; *) rid=linux-x64 ;; esac
art="$root/artifacts/$rid"
data="${XDG_DATA_HOME:-$HOME/.local/share}/AiPet"
step() { printf '\033[36m→ %s\033[0m\n' "$*"; }

# Build when there's no build yet, the source changed since the last one, or --rebuild (never install stale binaries).
stale=0
if [ ! -f "$art/app/AiPet" ] || [ ! -f "$art/hook/aipet-hook" ]; then stale=1
elif [ -n "$(find "$root/src" -type f -not -path '*/bin/*' -not -path '*/obj/*' \
          \( -newer "$art/app/AiPet" -o -newer "$art/hook/aipet-hook" \) -print -quit)" ]; then stale=1
fi
if [ "$stale" = 1 ] || [ "$REBUILD" = 1 ]; then
  if command -v dotnet >/dev/null && dotnet --list-sdks | grep -q '^10\.'; then
    step "Building for $rid"
    dotnet publish "$root/src/AiPet.UI" -c Release -r "$rid" --self-contained -o "$art/app" -p:DebugType=none --nologo -v quiet
    dotnet publish "$root/src/AiPet.Hook" -c Release -r "$rid" --self-contained -p:PublishAot=false \
      -p:PublishSingleFile=true -p:PublishTrimmed=true -p:DebugType=none -o "$art/hook" --nologo -v quiet
  else
    echo "The build in artifacts/$rid is missing or older than the source, and there's no .NET 10 SDK here to" >&2
    echo "rebuild it. Run build.sh (or build.ps1) first." >&2
    exit 1
  fi
fi

step "Stopping the running pet"
pkill -x AiPet 2>/dev/null || true
sleep 0.5

step "Installing the app to $data/app"
mkdir -p "$data"
rm -rf "$data/app"
cp -r "$art/app" "$data/app"
chmod +x "$data/app/AiPet"

step "Installing the hook to $data/hooks"
mkdir -p "$data/hooks"
cp "$art/hook/aipet-hook" "$data/hooks/aipet-hook"
chmod +x "$data/hooks/aipet-hook"

apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
mkdir -p "$apps"
desktop="[Desktop Entry]
Type=Application
Name=AiPet
Comment=Desktop pet for your coding agents
Exec=\"$data/app/AiPet\"
Terminal=false
Categories=Utility;Development;"
printf '%s\n' "$desktop" > "$apps/aipet.desktop"
# Leftovers from older versions: the login entry (older versions always wrote it under ~/.config), the reply inbox
# (the pet no longer sends replies), the app-path file, and the files the hooks and the pet used to share (the hooks
# now hand events to the running pet directly, and do nothing while it's closed).
rm -f "$HOME/.config/autostart/aipet.desktop" "${XDG_CONFIG_HOME:-$HOME/.config}/autostart/aipet.desktop"
rm -rf "$data/inbox" "$data/app-path.txt"
rm -f "$data"/{state.json,state.lock,heartbeat,autostart-disabled,claude-hook.log,codex-hook.log} "$data"/state.json.*.tmp

if [ "$HOOKS" = 1 ]; then
  if command -v claude >/dev/null || [ -d "$HOME/.claude" ]; then
    step "Registering the hook with Claude Code"; "$data/hooks/aipet-hook" --install claude
  fi
  if command -v codex >/dev/null || [ -d "$HOME/.codex" ]; then
    step "Registering the hook with Codex"; "$data/hooks/aipet-hook" --install codex
  fi
fi

missing=()
for t in playerctl secret-tool; do command -v "$t" >/dev/null || missing+=("$t"); done
if [ ${#missing[@]} -gt 0 ]; then
  echo "Optional: install ${missing[*]} (packages playerctl, libsecret-tools) for music and keyring support."
fi

if [ "$START" = 1 ]; then
  (setsid "$data/app/AiPet" >/dev/null 2>&1 < /dev/null &)
fi
printf '\033[32mDone. While the pet runs it picks up your Claude Code and Codex chats; while it is closed the hooks do nothing (start it from the app menu).\033[0m\n'
