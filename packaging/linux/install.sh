#!/bin/sh
# AiPet installer for Linux, shipped in AiPet-<version>-<rid>.tar.gz. Run it from the extracted folder:
#
#   ./install.sh               install (or update) AiPet from this folder, then start it
#   ./install.sh --no-start    install without starting it
#   ./install.sh --uninstall   remove the app, its menu entry and its icons, the hooks registered at its aipet-hook,
#                              and its Hyprland rules with the line that includes them (your settings and logs stay)
#
# App:   ~/.local/share/AiPet/app   AiPet and aipet-hook, and uninstall.sh (~/.local/share is $XDG_DATA_HOME if set)
# Menu:  ~/.local/share/applications/aipet.desktop, icons in ~/.local/share/icons/hicolor
# Hyprland: window rules in ~/.local/share/AiPet/hyprland, for your config to include (the installer prints the line)
# Needs no .NET (the app is self-contained) and starts nothing at sign-in. The Claude Code and Codex plugins are
# separate: the one-line installer adds them, or see the README.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
share=${XDG_DATA_HOME:-$HOME/.local/share}
data=$share/AiPet
app=$data/app
desktop=$share/applications/aipet.desktop
icons=$share/icons/hicolor

action=install
start=1
# the copy installed as app/uninstall.sh removes by default
if [ "$(basename "$0")" = uninstall.sh ]; then action=uninstall; fi
for a in "$@"; do
  case $a in
    --uninstall) action=uninstall ;;
    --no-start) start=0 ;;
    -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
    *) echo "unknown option: $a (see --help)" >&2; exit 2 ;;
  esac
done

step() { printf '\033[36m→ %s\033[0m\n' "$*"; }
warn() { printf '\033[33m! %s\033[0m\n' "$*" >&2; }
die() { printf '\033[31m%s\033[0m\n' "$*" >&2; exit 1; }

stop_pet() {
  if command -v pkill >/dev/null 2>&1 && pkill -u "$(id -u)" -x AiPet 2>/dev/null; then
    step "Stopping the running pet"
    n=0
    while [ "$n" -lt 5 ] && pgrep -u "$(id -u)" -x AiPet >/dev/null 2>&1; do sleep 1; n=$((n + 1)); done
  fi
}

# Menus and icon themes that keep caches pick up the change.
refresh_menu() {
  if [ -f "$icons/icon-theme.cache" ] && command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t -f "$icons" >/dev/null 2>&1 || true
  fi
  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q "$share/applications" >/dev/null 2>&1 || true
  fi
}

# Older versions started the pet at sign-in with this entry; now nothing does. A copy of the menu entry (it has
# Icon=aipet), which desktop settings make when you choose to start AiPet at sign-in yourself, stays until the
# uninstall (remove_old_autostart all).
remove_old_autostart() {
  for f in "$HOME/.config/autostart/aipet.desktop" "${XDG_CONFIG_HOME:-$HOME/.config}/autostart/aipet.desktop"; do
    if [ -f "$f" ] && { [ "${1:-}" = all ] || ! grep -qx 'Icon=aipet' "$f"; }; then
      step "Removing the sign-in entry $f"
      rm -f "$f"
    fi
  done
}

# Hooks registered straight at the app's aipet-hook (aipet-hook --install) would fail on every event once it's gone.
# Only when the config names that path: --uninstall removes every AiPet hook, also one a build from source registered.
# aipet-hook registers the path with symlinks resolved, so that one counts too.
# Usage: unregister <agent> <config file>...
unregister() {
  agent=$1
  shift
  real=$(cd "$app" 2>/dev/null && pwd -P || true)
  for f in "$@"; do
    if [ -f "$f" ] && { grep -qF "$app/aipet-hook" "$f" || { [ -n "$real" ] && grep -qF "$real/aipet-hook" "$f"; }; }; then
      step "Removing the hooks registered with $agent"
      "$app/aipet-hook" --uninstall "$agent" < /dev/null || warn "Couldn't remove them: remove the hooks that run $app/aipet-hook from $f by hand."
      return 0
    fi
  done
}

# Hyprland frames, blurs and animates every window, which the pet's transparent one doesn't want: the rules in
# hyprland/ turn that off for AiPet's windows. The config is the user's, so the installer only says which line
# includes them (the Lua config wins over hyprland.conf when both exist, as in Hyprland), and the uninstall takes
# that line out again.
hypr=${XDG_CONFIG_HOME:-$HOME/.config}/hypr
# the ~ is for the line the user copies, which Hyprland expands
# shellcheck disable=SC2088
case $data in
  "$HOME"/*) lua_path="os.getenv(\"HOME\") .. \"/${data#"$HOME"/}/hyprland/aipet.lua\""; conf_path="~/${data#"$HOME"/}/hyprland/aipet.conf" ;;
  *) lua_path="\"$data/hyprland/aipet.lua\""; conf_path="$data/hyprland/aipet.conf" ;;
esac

hyprland_hint() {
  if [ -f "$hypr/hyprland.lua" ]; then
    if ! grep -q 'AiPet/hyprland/aipet\.lua' "$hypr/hyprland.lua"; then
      echo "Hyprland: to give the pet no frame or blur and keep it on every workspace, add this to $hypr/hyprland.lua:"
      echo "  pcall(dofile, $lua_path)"
    fi
  elif [ -f "$hypr/hyprland.conf" ] && ! grep -q 'AiPet/hyprland/aipet\.conf' "$hypr/hyprland.conf"; then
    echo "Hyprland: to give the pet no frame or blur and keep it on every workspace, add this to $hypr/hyprland.conf"
    echo "(Hyprland 0.53 or later; for older ones see the end of $data/hyprland/aipet.conf):"
    echo "  source = $conf_path"
  fi
}

# Removes the lines that include AiPet's rules file (any line naming it that isn't a comment), and an "AiPet:" comment
# right above one. The config is backed up first, and written in place, so a symlink to a dotfiles repo stays one. Best
# effort: a config that can't be written (home-manager's is read-only) gets a note, and the uninstall goes on.
remove_hyprland_include() {
  for f in "$hypr/hyprland.lua" "$hypr/hyprland.conf"; do
    if [ -f "$f" ] && grep -q -e 'AiPet/hyprland/aipet\.lua' -e 'AiPet/hyprland/aipet\.conf' "$f"; then
      tmp=${TMPDIR:-/tmp}/aipet-hypr.$$
      if awk '
        /AiPet\/hyprland\/aipet\.(lua|conf)/ && !/^[ \t]*(--|#)/ { held = ""; next }
        { if (held != "") print held; held = "" }
        /^[ \t]*(--|#)[ \t]*AiPet:/ { held = $0; next }
        { print }
        END { if (held != "") print held }
      ' "$f" > "$tmp" 2>/dev/null && ! cmp -s "$f" "$tmp"; then
        step "Removing the line that includes AiPet's rules from $f"
        if ! { cp -p "$f" "$f.aipet-$(date +%Y%m%d-%H%M%S).bak" && cat "$tmp" > "$f"; } 2>/dev/null; then
          warn "Couldn't edit $f: remove the line that includes AiPet's rules (AiPet/hyprland/aipet.*) by hand."
        fi
      fi
      rm -f "$tmp"
    fi
  done
}

# ---------------------------------------------------------------- uninstall
if [ "$action" = uninstall ]; then
  stop_pet
  if [ -x "$app/aipet-hook" ]; then
    unregister claude "${CLAUDE_CONFIG_DIR:-$HOME/.claude}/settings.json"
    unregister codex "${CODEX_HOME:-$HOME/.codex}/config.toml" "${CODEX_HOME:-$HOME/.codex}/hooks.json"
  fi
  remove_hyprland_include
  step "Removing $app and the menu entry"
  rm -rf "$app" "$data/hyprland"
  rm -f "$desktop"
  remove_old_autostart all
  for f in "$icons"/*/apps/aipet.png "$icons"/*/apps/aipet.svg; do
    if [ -f "$f" ]; then rm -f "$f"; fi
  done
  refresh_menu
  printf '\033[32mAiPet is removed.\033[0m Your settings and logs are still in %s (delete that folder to remove them too).\n' "$data"
  echo "The agent plugins are separate: claude plugin uninstall aipet@aipet, and codex plugin remove aipet@aipet."
  exit 0
fi

# ---------------------------------------------------------------- install
[ -f "$here/app/AiPet" ] || die "There's no app/AiPet next to this script: run install.sh from the extracted AiPet-<version>-<platform> folder."
case $here in
  "$app"|"$app"/*) die "Run install.sh from the extracted AiPet-<version>-<platform> folder, not from $app." ;;
esac
# the package's processor (the ELF header's machine field) has to match this machine's
machine=$(od -An -tx1 -j18 -N1 "$here/app/AiPet" 2>/dev/null | tr -d ' \n' || true)
case $(uname -m) in x86_64|amd64) want=3e ;; aarch64|arm64) want=b7 ;; *) want= ;; esac
if [ -n "$want" ] && [ -n "$machine" ] && [ "$machine" != "$want" ]; then
  die "This package is for another processor than this machine's ($(uname -m)): download the other AiPet package."
fi

stop_pet
remove_old_autostart
step "Installing the app to $app"
# the data folder holds the settings and logs: only its user may read it. An existing one keeps its mode.
mkdir -p "$share"
if [ ! -d "$data" ]; then mkdir -m 700 "$data"; fi
new=$data/.app-new
rm -rf "$new"
cp -R "$here/app" "$new"
chmod 755 "$new/AiPet"
if [ -f "$new/aipet-hook" ]; then chmod 755 "$new/aipet-hook"; fi
if [ -f "$here/install.sh" ]; then cp "$here/install.sh" "$new/uninstall.sh" && chmod 755 "$new/uninstall.sh"; fi
rm -rf "$app"
mv "$new" "$app"

# Exec= quoting (Desktop Entry spec): the path goes in double quotes; paths with characters that would need escaping
# get no menu entry rather than a wrong one.
nl='
'
case $app in
  *"$nl"*|*'"'*|*'`'*|*'$'*|*'\'*|*'%'*)
    warn "The app's path ($app) has characters a menu entry can't hold, so there's no menu entry: start $app/AiPet directly." ;;
  *)
    if [ -f "$here/aipet.desktop.in" ]; then
      step "Adding AiPet to the app menu"
      mkdir -p "$share/applications"
      while IFS= read -r line || [ -n "$line" ]; do
        case $line in
          *@EXEC@*) printf '%s"%s"%s\n' "${line%%@EXEC@*}" "$app/AiPet" "${line#*@EXEC@}" ;;
          *) printf '%s\n' "$line" ;;
        esac
      done < "$here/aipet.desktop.in" > "$desktop.tmp"
      mv "$desktop.tmp" "$desktop"
    else
      warn "aipet.desktop.in is missing from this folder, so there's no menu entry."
    fi ;;
esac
if [ -d "$here/icons/hicolor" ]; then
  mkdir -p "$icons"
  cp -R "$here/icons/hicolor/." "$icons/"
fi
refresh_menu
if [ -d "$here/hyprland" ]; then
  rm -rf "$data/hyprland"
  cp -R "$here/hyprland" "$data/hyprland"
  if [ -n "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]; then hyprland_hint; fi
fi

missing=
for t in playerctl secret-tool; do
  command -v "$t" >/dev/null 2>&1 || missing="$missing $t"
done
if [ -n "$missing" ]; then
  echo "Optional:$missing (packages playerctl, libsecret-tools) add music and keyring support."
fi

# Started without the token the one-line installer may have been given: the pet runs for days, and the browsers and
# apps it opens would get the token too.
if [ "$start" = 1 ]; then
  if [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
    echo "There's no desktop session here, so AiPet isn't started: start it from the app menu."
  elif command -v setsid >/dev/null 2>&1; then
    (unset GITHUB_TOKEN; setsid "$app/AiPet" >/dev/null 2>&1 < /dev/null &)
  else
    (unset GITHUB_TOKEN; nohup "$app/AiPet" >/dev/null 2>&1 < /dev/null &)
  fi
fi
printf '\033[32mDone.\033[0m While AiPet runs it shows your Claude Code and Codex chats; nothing starts it at sign-in.\n'
printf 'To remove it: %s\n' "$app/uninstall.sh"
