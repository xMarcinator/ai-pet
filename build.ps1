# Builds AiPet for every supported platform into ./artifacts and bundles the hooks into the plugin.
#
#   pwsh ./build.ps1                 # Windows + Linux
#   pwsh ./build.ps1 -Only win-x64   # just one platform (win-x64, linux-x64, linux-arm64)
#
# artifacts/<rid>/app    the desktop app (self-contained: no .NET install needed)
# artifacts/<rid>/hook   aipet-hook, built with cargo from rust/crates/aipet-hook as the release builds it: for Windows
#                        with the C runtime linked in; for Linux with cargo-zigbuild (for glibc 2.27 and newer), when
#                        cargo-zigbuild and zig are installed (build.sh on Linux builds it without them)
# plugins/aipet/native/<rid>/   the same hooks, next to aipet-hook.sh, which picks the right one per OS. They're
#                        gitignored: the release workflow adds them to the release commit in the plugin repository.
# artifacts/marketplace/ a local marketplace with a copy of the plugin, to try the plugin before a release:
#                        claude --plugin-dir plugins/aipet    (one session), or
#                        claude plugin marketplace add ./artifacts/marketplace; claude plugin install aipet@aipet
#                        codex plugin marketplace add ./artifacts/marketplace; codex plugin add aipet@aipet
#                        (it's named aipet like the real one, so remove that first: ... marketplace remove aipet)
# Needs the .NET 10 SDK, and Rust (rustup) with Visual Studio's C++ build tools and Windows SDK. Releases are built by
# .github/workflows/release.yml, not by this script.
param([string[]]$Only = @('win-x64', 'linux-x64'))
$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
$cargoToml = Join-Path $root 'rust\Cargo.toml'
$triples = @{ 'win-x64' = 'x86_64-pc-windows-msvc'; 'linux-x64' = 'x86_64-unknown-linux-gnu'; 'linux-arm64' = 'aarch64-unknown-linux-gnu' }
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw 'build.ps1 needs Rust (cargo, from https://rustup.rs)' }

function Publish($project, $out, [string[]]$extra) {
    dotnet publish $project -c Release -o $out --nologo -v quiet -p:DebugType=none @extra
    if ($LASTEXITCODE -ne 0) { throw "Build failed: $project -> $out" }
}

# The hook for a target, with cargo build, or cargo zigbuild for Linux; returns the path of what it built.
function Hook([string]$command, [string]$triple, [string]$target, [string]$exe) {
    cargo $command -p aipet-hook --release --locked --manifest-path $cargoToml --target $target | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "Building the hook failed: cargo $command --target $target" }
    Join-Path $root "rust\target\$triple\release\$exe"
}

foreach ($rid in $Only) {
    Write-Host "→ $rid" -ForegroundColor Cyan
    $triple = $triples[$rid]
    if (-not $triple) { throw "unknown platform: $rid" }
    $art = Join-Path $root "artifacts\$rid"
    if (Test-Path "$art\app") { Remove-Item "$art\app" -Recurse -Force }
    if (Test-Path "$art\hook") { Remove-Item "$art\hook" -Recurse -Force }

    Publish "$root\src\AiPet.UI\AiPet.UI.csproj" "$art\app" @('-r', $rid, '--self-contained')

    # the hook runs on every agent event: native, so it starts in a millisecond or two
    $hook = $null
    if ($rid -like 'win-*') {
        # with the C runtime linked in, so that it needs only Windows' own DLLs
        $flags = $env:RUSTFLAGS
        $env:RUSTFLAGS = '-C target-feature=+crt-static'
        try { $hook = Hook 'build' $triple $triple 'aipet-hook.exe' }
        finally { $env:RUSTFLAGS = $flags }
    } elseif (Get-Command cargo-zigbuild -ErrorAction SilentlyContinue) {
        rustup target add $triple | Out-Null
        $hook = Hook 'zigbuild' $triple "$triple.2.27" 'aipet-hook'
    } else {
        Write-Warning "No $rid hook: building it here needs cargo-zigbuild and zig (cargo install cargo-zigbuild; pip install ziglang), or build.sh on Linux"
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
