# Builds AiPet for every supported platform into ./artifacts and bundles the hooks into the plugin.
#
#   pwsh ./build.ps1                 # Windows + Linux
#   pwsh ./build.ps1 -Only win-x64   # just one platform (win-x64, linux-x64, linux-arm64)
#
# artifacts/<rid>/app    the desktop app (self-contained: no .NET install needed)
# artifacts/<rid>/hook   aipet-hook (native on Windows; a trimmed single file for Linux, since NativeAOT can't
#                        cross-compile: build.sh on Linux builds it natively)
# plugins/aipet/native/<rid>/   the same hooks, next to aipet-hook.sh, which picks the right one per OS. They're
#                        gitignored: the release workflow adds them to the release commit in the plugin repository.
# artifacts/marketplace/ a local marketplace with a copy of the plugin, to try the plugin before a release:
#                        claude --plugin-dir plugins/aipet    (one session), or
#                        claude plugin marketplace add ./artifacts/marketplace; claude plugin install aipet@aipet
#                        codex plugin marketplace add ./artifacts/marketplace; codex plugin add aipet@aipet
#                        (it's named aipet like the real one, so remove that first: ... marketplace remove aipet)
# Releases are built by .github/workflows/release.yml, not by this script.
param([string[]]$Only = @('win-x64', 'linux-x64'))
$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot

# NativeAOT's linker lookup needs vswhere on PATH (Windows hook only).
$vsInstaller = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer"
if ((Test-Path $vsInstaller) -and ($env:PATH -notlike "*$vsInstaller*")) { $env:PATH += ";$vsInstaller" }

function Publish($project, $out, [string[]]$extra) {
    dotnet publish $project -c Release -o $out --nologo -v quiet -p:DebugType=none @extra
    if ($LASTEXITCODE -ne 0) { throw "Build failed: $project -> $out" }
}

foreach ($rid in $Only) {
    Write-Host "→ $rid" -ForegroundColor Cyan
    $art = Join-Path $root "artifacts\$rid"
    if (Test-Path "$art\app") { Remove-Item "$art\app" -Recurse -Force }
    if (Test-Path "$art\hook") { Remove-Item "$art\hook" -Recurse -Force }

    Publish "$root\src\AiPet.UI\AiPet.UI.csproj" "$art\app" @('-r', $rid, '--self-contained')

    if ($rid -like 'win-*') {
        # native (AOT): starts in a few milliseconds, runs on every agent event. Needs the Visual Studio C++
        # tools; when they're missing (or mid-update) fall back to a trimmed self-contained single file.
        try { Publish "$root\src\AiPet.Hook\AiPet.Hook.csproj" "$art\hook" @('-r', $rid) }
        catch {
            Write-Warning "Native build of the hook failed (C++ tools unavailable?); building a single-file hook instead"
            Publish "$root\src\AiPet.Hook\AiPet.Hook.csproj" "$art\hook" @('-r', $rid, '--self-contained',
                '-p:PublishAot=false', '-p:PublishSingleFile=true', '-p:PublishTrimmed=true')
        }
    } else {
        # AOT can't cross-compile to Linux from Windows; a trimmed self-contained single file needs no .NET either
        Publish "$root\src\AiPet.Hook\AiPet.Hook.csproj" "$art\hook" @('-r', $rid, '--self-contained',
            '-p:PublishAot=false', '-p:PublishSingleFile=true', '-p:PublishTrimmed=true')
    }

    $dest = Join-Path $root "plugins\aipet\native\$rid"
    if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }
    New-Item -ItemType Directory -Force $dest | Out-Null
    Get-ChildItem "$art\hook" -Filter 'aipet-hook*' | Where-Object { $_.Extension -notin '.pdb', '.dbg' } | Copy-Item -Destination $dest -Force
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
