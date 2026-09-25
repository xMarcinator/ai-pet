#!/bin/sh
# AiPet one-line installer for Linux (x64 and arm64):
#
#   curl -fsSL https://raw.githubusercontent.com/xMarcinator/ai-pet/main/install.sh | sh
#
# Downloads the latest release for this machine, checks it against the release's SHA256SUMS and installs the app to
# ~/.local/share/AiPet/app with a menu entry (nothing starts at sign-in). Then it adds the AiPet marketplace and installs
# the aipet plugin for Claude Code and for Codex, whichever of them is on PATH. Running it again updates everything.
#
# Settings (environment variables, e.g. curl ... | AIPET_VERSION=0.2.0 sh):
#   AIPET_VERSION=0.2.0    install that release of the app instead of the latest (the plugin still comes from the
#                          latest release)
#   AIPET_NO_PLUGINS=1     install only the app
#   AIPET_NO_START=1       don't start the pet afterwards
#   GITHUB_TOKEN=...       a token that can read the repositories, while they are private. Fetch the script with it too:
#     export GITHUB_TOKEN=...
#     curl -fsSL -H "Authorization: Bearer $GITHUB_TOKEN" -H "Accept: application/vnd.github.raw+json" \
#       https://api.github.com/repos/xMarcinator/ai-pet/contents/install.sh | sh
#
# To build and install from a clone of the repository instead: scripts/install-from-source.sh.
#
# Everything runs inside main, so a command that reads stdin can't eat the rest of this script as it arrives from curl.
set -eu

repo=xMarcinator/ai-pet
api=https://api.github.com/repos/$repo

say() { printf '\033[36m→ %s\033[0m\n' "$*"; }
warn() { printf '\033[33m! %s\033[0m\n' "$*" >&2; }
die() { printf '\033[31m%s\033[0m\n' "$*" >&2; exit 1; }

# GET from GitHub's API (with the token when there is one; the header comes from a file so the token isn't in ps).
# Usage: api_get <url> <output file> [accept]
api_get() {
  if [ -s "$tmp/auth" ]; then
    curl -fsSL --retry 3 -H @"$tmp/auth" -H "Accept: ${3:-application/vnd.github+json}" -o "$2" "$1"
  else
    curl -fsSL --retry 3 -H "Accept: ${3:-application/vnd.github+json}" -o "$2" "$1"
  fi
}

# The API URL of a release asset, from the release JSON: an asset's "url" comes before its "name".
# (No [[:space:]] in awk: older mawk doesn't know character classes.)
asset_api_url() {
  tr ',{}' '\n\n\n' < "$tmp/release.json" | awk -v want="$1" '
    /"url"[ \t]*:[ \t]*"https:\/\/api\.github\.com\/repos\/.*\/releases\/assets\/[0-9]+"/ {
      u = $0; sub(/^[^:]*:[ \t]*"/, "", u); sub(/".*$/, "", u) }
    /"name"[ \t]*:/ {
      n = $0; sub(/^[^:]*:[ \t]*"/, "", n); sub(/".*$/, "", n)
      if (n == want && u != "") { print u; exit } }'
}

# Downloads a release asset. Usage: download <asset name> <output file>
download() {
  grep -q "\"name\"[[:space:]]*:[[:space:]]*\"$1\"" "$tmp/release.json" || die "Release $tag has no $1."
  if [ -s "$tmp/auth" ]; then
    asset_url=$(asset_api_url "$1")
    [ -n "$asset_url" ] || die "Couldn't find the download link of $1 in release $tag."
    api_get "$asset_url" "$2" application/octet-stream || die "Couldn't download $1."
  else
    curl -fsSL --retry 3 -o "$2" "https://github.com/$repo/releases/download/$tag/$1" || die "Couldn't download $1."
  fi
}

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{ print $1 }'
  else shasum -a 256 "$1" | awk '{ print $1 }'
  fi
}

# Runs claude or codex. While the repositories are private, the git they run authenticates with the token through
# git's environment configuration, for these commands only. stdin is /dev/null: see the note at the top.
agent() {
  (
    if [ -n "${GITHUB_TOKEN:-}" ]; then
      GIT_CONFIG_COUNT=1
      GIT_CONFIG_KEY_0=http.https://github.com/.extraheader
      GIT_CONFIG_VALUE_0="AUTHORIZATION: basic $(printf 'x-access-token:%s' "$GITHUB_TOKEN" | base64 | tr -d '\n')"
      CLAUDE_CODE_PLUGIN_PREFER_HTTPS=1
      export GIT_CONFIG_COUNT GIT_CONFIG_KEY_0 GIT_CONFIG_VALUE_0 CLAUDE_CODE_PLUGIN_PREFER_HTTPS
    fi
    "$@" < /dev/null
  )
}

# Whether a config file still registers AiPet's hook directly (aipet-hook --install), which the plugin replaces.
registered() {
  for f in "$@"; do
    if [ -f "$f" ] && grep -q aipet-hook "$f"; then return 0; fi
  done
  return 1
}

main() {
  case $(uname -s) in
    Linux) ;;
    Darwin) die "AiPet doesn't support macOS yet." ;;
    MINGW*|MSYS*|CYGWIN*) die "On Windows, run this in PowerShell instead: irm https://raw.githubusercontent.com/$repo/main/install.ps1 | iex" ;;
    *) die "AiPet doesn't support $(uname -s)." ;;
  esac
  case $(uname -m) in
    x86_64|amd64) rid=linux-x64 ;;
    aarch64|arm64) rid=linux-arm64 ;;
    *) die "AiPet is built for x64 and arm64 processors, not $(uname -m)." ;;
  esac
  for t in curl tar awk mktemp; do
    command -v "$t" >/dev/null 2>&1 || die "This installer needs $t."
  done
  command -v sha256sum >/dev/null 2>&1 || command -v shasum >/dev/null 2>&1 || die "This installer needs sha256sum (coreutils)."

  tmp=$(mktemp -d "${TMPDIR:-/tmp}/aipet-install.XXXXXX")
  trap 'rm -rf "$tmp"' EXIT
  trap 'exit 130' INT TERM
  if [ -n "${GITHUB_TOKEN:-}" ]; then
    (umask 077 && printf 'Authorization: Bearer %s\n' "$GITHUB_TOKEN" > "$tmp/auth")
  fi

  # ------------------------------------------------------------ the release
  if [ -n "${AIPET_VERSION:-}" ]; then release_url=$api/releases/tags/v${AIPET_VERSION#v}; else release_url=$api/releases/latest; fi
  if ! api_get "$release_url" "$tmp/release.json"; then
    if [ -n "${GITHUB_TOKEN:-}" ]; then die "Couldn't read the release from GitHub ($release_url)."
    else die "Couldn't read the release from GitHub ($release_url). While the repository is private, set GITHUB_TOKEN (see the top of this script)."
    fi
  fi
  tag=$(tr ',{}' '\n\n\n' < "$tmp/release.json" | sed -n 's/^[[:space:]]*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*$/\1/p' | head -n 1)
  [ -n "$tag" ] || die "GitHub's answer for $release_url has no release tag."
  version=${tag#v}
  package=AiPet-$version-$rid.tar.gz

  say "Downloading AiPet $version for $rid"
  download "$package" "$tmp/$package"
  download SHA256SUMS "$tmp/SHA256SUMS"
  expected=$(awk -v n="$package" '$2 == n || $2 == "*" n { print $1; exit }' "$tmp/SHA256SUMS")
  [ -n "$expected" ] || die "The release's SHA256SUMS has no entry for $package."
  actual=$(sha256 "$tmp/$package")
  [ "$actual" = "$expected" ] || die "$package doesn't match the release's SHA256SUMS (expected $expected, got $actual)."
  say "Checksum OK"

  # ------------------------------------------------------------ the app
  tar -xzf "$tmp/$package" -C "$tmp"
  folder=$tmp/AiPet-$version-$rid
  [ -f "$folder/install.sh" ] || die "$package has no install.sh."
  # without the token: the package's installer doesn't need it, and the pet it starts runs for days
  if [ "${AIPET_NO_START:-0}" = 1 ]; then (unset GITHUB_TOKEN; sh "$folder/install.sh" --no-start < /dev/null)
  else (unset GITHUB_TOKEN; sh "$folder/install.sh" < /dev/null)
  fi
  hook=${XDG_DATA_HOME:-$HOME/.local/share}/AiPet/app/aipet-hook

  # ------------------------------------------------------------ the plugins
  claude_done=0
  codex_done=0
  if [ "${AIPET_NO_PLUGINS:-0}" != 1 ]; then
    if { command -v claude >/dev/null 2>&1 || command -v codex >/dev/null 2>&1; } && ! command -v git >/dev/null 2>&1; then
      warn "Claude Code and Codex fetch plugins with git, which isn't installed: install git, then run this again."
    fi
    # The marketplace on main pins the latest release's plugin, and no tag of this repository pins an older one (a
    # release's tag is made before its plugin is pinned).
    if [ -n "${AIPET_VERSION:-}" ] && { command -v claude >/dev/null 2>&1 || command -v codex >/dev/null 2>&1; }; then
      warn "AIPET_VERSION pins only the app: the plugin for Claude Code and Codex (with its hook) comes from the latest release,"
      warn "and AiPet $version may not understand a newer hook. To leave the plugins as they are, set AIPET_NO_PLUGINS=1."
    fi

    if command -v claude >/dev/null 2>&1; then
      say "Adding the AiPet plugin to Claude Code"
      # "already added" is fine: the update below refreshes it
      agent claude plugin marketplace add "$repo" --sparse .claude-plugin || true
      if agent claude plugin marketplace update aipet && agent claude plugin install aipet@aipet \
          && agent claude plugin update aipet@aipet; then
        claude_done=1
        # hooks registered directly by older installs (aipet-hook --install claude) would report every event twice
        claude_dir=${CLAUDE_CONFIG_DIR:-$HOME/.claude}
        if [ -x "$hook" ] && registered "$claude_dir/settings.json"; then
          say "Removing the older direct hook registration from Claude Code"
          "$hook" --uninstall claude < /dev/null || warn "Couldn't remove it: run $hook --uninstall claude"
        fi
      else
        warn "Couldn't add the plugin to Claude Code. Try it yourself:"
        warn "  claude plugin marketplace add $repo && claude plugin install aipet@aipet"
      fi
    fi

    if command -v codex >/dev/null 2>&1; then
      say "Adding the AiPet plugin to Codex"
      agent codex plugin marketplace add "$repo" --sparse .agents/plugins || true
      if agent codex plugin marketplace upgrade aipet && agent codex plugin add aipet@aipet; then
        codex_done=1
        codex_dir=${CODEX_HOME:-$HOME/.codex}
        if [ -x "$hook" ] && registered "$codex_dir/config.toml" "$codex_dir/hooks.json"; then
          say "Removing the older direct hook registration from Codex"
          "$hook" --uninstall codex < /dev/null || warn "Couldn't remove it: run $hook --uninstall codex"
        fi
      else
        warn "Couldn't add the plugin to Codex. Try it yourself:"
        warn "  codex plugin marketplace add $repo && codex plugin add aipet@aipet"
      fi
    fi
  fi

  printf '\n\033[32mAiPet %s is installed.\033[0m It shows your chats while it runs (start it from the app menu); nothing starts it at sign-in.\n' "$version"
  if [ "$claude_done" = 1 ]; then
    echo "Claude Code: new sessions pick up the plugin (in an open one, run /reload-plugins)."
  fi
  if [ "$codex_done" = 1 ]; then
    printf '\033[33mOne step left for Codex:\033[0m Codex runs only hooks you trust. Run codex, open /hooks and trust the\n'
    echo "aipet@aipet hooks (or choose Review hooks when Codex asks at startup). Then restart the ChatGPT app if it's open."
  fi
  if [ "$claude_done" = 0 ] && [ "$codex_done" = 0 ] && [ "${AIPET_NO_PLUGINS:-0}" != 1 ]; then
    echo "No plugin was added (Claude Code and Codex weren't found, or adding failed): see the README."
  fi
}

main "$@"
