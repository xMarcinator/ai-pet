# Builds AiPet for every supported platform into ./artifacts and bundles the hooks into the plugin.
#
#   pwsh ./build.ps1                 # Windows + Linux
#   pwsh ./build.ps1 -Only win-x64   # just one platform (win-x64, linux-x64, linux-arm64)
#
# artifacts/<rid>/app    the desktop app, AiPet.exe or AiPet (rust/crates/aipet)
# artifacts/<rid>/hook   aipet-hook (rust/crates/aipet-hook)
#                        Both are built with cargo as the release builds them: for Windows with the C runtime linked
#                        in; for Linux with cargo-zigbuild (for glibc 2.27 and newer), when cargo-zigbuild and zig are
#                        installed and pkg-config finds that platform's libxkbcommon (build.sh on Linux needs neither)
# plugins/aipet/native/<rid>/   the same hooks, next to aipet-hook.sh, which picks the right one per OS. They're
#                        gitignored: the release workflow adds them to the release commit in the plugin repository.
# artifacts/marketplace/ a local marketplace with a copy of the plugin, to try the plugin before a release:
#                        claude --plugin-dir plugins/aipet    (one session), or
#                        claude plugin marketplace add ./artifacts/marketplace; claude plugin install aipet@aipet
#                        codex plugin marketplace add ./artifacts/marketplace; codex plugin add aipet@aipet
#                        (it's named aipet like the real one, so remove that first: ... marketplace remove aipet)
# Needs Rust (rustup) with Visual Studio's C++ build tools and Windows SDK. Releases are built by
# .github/workflows/release.yml, not by this script.
param([string[]]$Only = @('win-x64', 'linux-x64'))
$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
$cargoToml = Join-Path $root 'rust\Cargo.toml'
$triples = @{ 'win-x64' = 'x86_64-pc-windows-msvc'; 'linux-x64' = 'x86_64-unknown-linux-gnu'; 'linux-arm64' = 'aarch64-unknown-linux-gnu' }
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw 'build.ps1 needs Rust (cargo, from https://rustup.rs)' }

# The pet and the hook for a target, with cargo build, or cargo zigbuild for Linux; returns the folder they're in.
function Build([string]$command, [string]$triple, [string]$target) {
    cargo $command -p aipet -p aipet-hook --release --locked --manifest-path $cargoToml --target $target | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "Building the pet and the hook failed: cargo $command --target $target" }
    Join-Path $root "rust\target\$triple\release"
}

foreach ($rid in $Only) {
    Write-Host "→ $rid" -ForegroundColor Cyan
    $triple = $triples[$rid]
    if (-not $triple) { throw "unknown platform: $rid" }
    $art = Join-Path $root "artifacts\$rid"
    if (Test-Path "$art\app") { Remove-Item "$art\app" -Recurse -Force }
    if (Test-Path "$art\hook") { Remove-Item "$art\hook" -Recurse -Force }

    $out = $null
    $exe = ''
    $flags = $env:RUSTFLAGS
    if ($rid -like 'win-*') {
        # with the C runtime linked in, so that they need only Windows' own DLLs
        $exe = '.exe'
        $env:RUSTFLAGS = '-C target-feature=+crt-static'
        try { $out = Build 'build' $triple $triple }
        finally { $env:RUSTFLAGS = $flags }
    } elseif (Get-Command cargo-zigbuild -ErrorAction SilentlyContinue) {
        rustup target add $triple | Out-Null
        # as release.yml links them: the system's libxkbcommon may ask for a newer glibc itself
        $env:RUSTFLAGS = '-C link-arg=-Wl,--allow-shlib-undefined'
        try { $out = Build 'zigbuild' $triple "$triple.2.27" }
        finally { $env:RUSTFLAGS = $flags }
    } else {
        Write-Warning "No $rid build: building it here needs cargo-zigbuild and zig (cargo install cargo-zigbuild; pip install ziglang), or build.sh on Linux"
    }
    $hook = $null
    if ($out) {
        New-Item -ItemType Directory -Force "$art\app" | Out-Null
        Copy-Item (Join-Path $out "AiPet$exe") "$art\app" -Force
        $hook = Join-Path $out "aipet-hook$exe"
    }

    $dest = Join-Path $root "plugins\aipet\native\$rid"
    if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }
    if ($hook) {
        New-Item -ItemType Directory -Force "$art\hook", $dest | Out-Null
        Copy-Item $hook "$art\hook" -Force
        Copy-Item $hook $dest -Force
    }
}

# A local marketplace with a copy of the plugin and the hooks built so far, to try the plugin before a release. JSON
# is written without a byte order mark (Windows PowerShell's Set-Content would add one, which the agents reject).
$market = Join-Path $root 'artifacts\marketplace'
if (Test-Path $market) { Remove-Item $market -Recurse -Force }
New-Item -ItemType Directory -Force (Join-Path $market 'plugins'), (Join-Path $market '.claude-plugin'), (Join-Path $market '.agents\plugins') | Out-Null
Copy-Item (Join-Path $root 'plugins\aipet') (Join-Path $market 'plugins\aipet') -Recurse
$claudeMarket = @'
{
  "name": "aipet",
  "description": "AiPet, local build (made by build.ps1 or build.sh, for trying the plugin before a release)",
  "owner": { "name": "xMarcinator" },
  "plugins": [
    { "name": "aipet", "source": "./plugins/aipet", "description": "Shows each Claude Code chat's status on the AiPet desktop pet." }
  ]
}
'@
$codexMarket = @'
{
  "name": "aipet",
  "interface": { "displayName": "AiPet (local build)" },
  "plugins": [
    { "name": "aipet", "source": { "source": "local", "path": "./plugins/aipet" }, "policy": { "installation": "AVAILABLE" }, "category": "Productivity" }
  ]
}
'@
[IO.File]::WriteAllText((Join-Path $market '.claude-plugin\marketplace.json'), $claudeMarket)
[IO.File]::WriteAllText((Join-Path $market '.agents\plugins\marketplace.json'), $codexMarket)

Write-Host "Done: $(Join-Path $root 'artifacts')" -ForegroundColor Green
