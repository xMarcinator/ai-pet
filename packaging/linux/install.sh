#!/bin/sh
# AiPet installer for Linux, shipped in AiPet-<version>-<rid>.tar.gz. Run it from the extracted folder:
#
#   ./install.sh               install (or update) AiPet from this folder, then start it
#   ./install.sh --no-start    install without starting it
#   ./install.sh --uninstall   remove the app, its menu entry and its icons (your settings and logs stay)
#
# App:   ~/.local/share/AiPet/app   AiPet and aipet-hook, and uninstall.sh (~/.local/share is $XDG_DATA_HOME if set)
# Menu:  ~/.local/share/applications/aipet.desktop, icons in ~/.local/share/icons/hicolor
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
    -h|--help) sed -n '2,11p' "$0"; exit 0 ;;
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

# ---------------------------------------------------------------- uninstall
if [ "$action" = uninstall ]; then
  stop_pet
  step "Removing $app and the menu entry"
  rm -rf "$app"
  rm -f "$desktop"
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
step "Installing the app to $app"
mkdir -p "$data"
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

missing=
for t in playerctl secret-tool; do
  command -v "$t" >/dev/null 2>&1 || missing="$missing $t"
done
if [ -n "$missing" ]; then
  echo "Optional:$missing (packages playerctl, libsecret-tools) add music and keyring support."
fi

if [ "$start" = 1 ]; then
  if [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
    echo "There's no desktop session here, so AiPet isn't started: start it from the app menu."
  elif command -v setsid >/dev/null 2>&1; then
    (setsid "$app/AiPet" >/dev/null 2>&1 < /dev/null &)
  else
    (nohup "$app/AiPet" >/dev/null 2>&1 < /dev/null &)
  fi
fi
printf '\033[32mDone.\033[0m While AiPet runs it shows your Claude Code and Codex chats; nothing starts it at sign-in.\n'
printf 'To remove it: %s\n' "$app/uninstall.sh"
