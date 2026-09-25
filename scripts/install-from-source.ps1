# AiPet installer for Windows, from a clone of the repository: builds from source (build.ps1) and installs the result.
# End users install a release with the one-line installer (install.ps1 in the repository root) instead.
#
#   pwsh ./scripts/install-from-source.ps1              # build if needed, install the app, register hooks for Claude Code and Codex
#   pwsh ./scripts/install-from-source.ps1 -NoHooks     # just the app
#   pwsh ./scripts/install-from-source.ps1 -Rebuild     # build even if the last build looks current
#
# App:   %LOCALAPPDATA%\Programs\AiPet      (+ Start menu shortcut)
# Hook:  %LOCALAPPDATA%\AiPet\hooks\aipet-hook.exe, registered in ~/.claude/settings.json and ~/.codex/config.toml
#        (Codex then asks you to trust it: run `codex` once and choose Review hooks; see aipet-hook --doctor codex)
# These registrations stand in for the plugins: don't use both, or every event reaches the pet twice.
param([switch]$NoHooks, [switch]$NoStart, [switch]$Rebuild)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$art = Join-Path $root 'artifacts\win-x64'
$appDir = Join-Path $env:LOCALAPPDATA 'Programs\AiPet'
$hookDir = Join-Path $env:LOCALAPPDATA 'AiPet\hooks'
function Step($text) { Write-Host "→ $text" -ForegroundColor Cyan }

# Build when there's no build yet, or the source changed since the last one (never install stale binaries).
$built = @("$art\app\AiPet.exe", "$art\hook\aipet-hook.exe") | Where-Object { Test-Path $_ } |
    ForEach-Object { (Get-Item $_).LastWriteTimeUtc } | Sort-Object | Select-Object -First 1
$stale = $built -and (Get-ChildItem "$root\src" -Recurse -File | Where-Object {
    $_.FullName -notmatch '\\(bin|obj)\\' -and $_.LastWriteTimeUtc -gt $built } | Select-Object -First 1)
if ($Rebuild -or -not (Test-Path "$art\app\AiPet.exe") -or -not (Test-Path "$art\hook\aipet-hook.exe") -or $stale) {
    Step 'Building'
    & "$root\build.ps1" -Only win-x64
}

Step 'Stopping the running pet'
Get-Process AiPet, ClaudePet -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 700

Step "Installing the app to $appDir"
New-Item -ItemType Directory -Force $appDir | Out-Null
robocopy "$art\app" $appDir /MIR /NFL /NDL /NJH /NJS /NP | Out-Null
if ($LASTEXITCODE -ge 8) { throw "Copying the app failed (robocopy $LASTEXITCODE)" }

Step "Installing the hook to $hookDir"
New-Item -ItemType Directory -Force $hookDir | Out-Null
Copy-Item "$art\hook\aipet-hook.exe" $hookDir -Force

$shell = New-Object -ComObject WScript.Shell
$lnk = $shell.CreateShortcut((Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\AiPet.lnk'))
$lnk.TargetPath = Join-Path $appDir 'AiPet.exe'; $lnk.WorkingDirectory = $appDir; $lnk.Description = 'AiPet desktop pet'; $lnk.Save()
# Leftovers from older versions: the sign-in entry, the reply inbox (the pet no longer sends replies), the app-path
# file, and the files the hooks and the pet used to share (the hooks now hand events to the running pet directly,
# and do nothing while it's closed).
Remove-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name AiPet -ErrorAction SilentlyContinue
$data = Join-Path $env:LOCALAPPDATA 'AiPet'
Remove-Item ('inbox', 'app-path.txt', 'state.json', 'state.json.*.tmp', 'state.lock', 'heartbeat', 'autostart-disabled',
    'claude-hook.log', 'codex-hook.log' | ForEach-Object { Join-Path $data $_ }) -Recurse -Force -ErrorAction SilentlyContinue

if (-not $NoHooks) {
    $hook = Join-Path $hookDir 'aipet-hook.exe'
    if ((Get-Command claude -ErrorAction SilentlyContinue) -or (Test-Path "$HOME\.claude")) {
        Step 'Registering the hook with Claude Code'; & $hook --install claude
    }
    if ((Get-Command codex -ErrorAction SilentlyContinue) -or (Test-Path "$HOME\.codex")) {
        Step 'Registering the hook with Codex'; & $hook --install codex
    }
}

if (-not $NoStart) { Start-Process (Join-Path $appDir 'AiPet.exe') }
Write-Host 'Done. While the pet runs it picks up your Claude Code and Codex chats; while it is closed the hooks do nothing (start it from the Start menu).' -ForegroundColor Green
