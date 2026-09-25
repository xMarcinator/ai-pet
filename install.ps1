# AiPet one-line installer for Windows (x64):
#
#   irm https://raw.githubusercontent.com/xMarcinator/ai-pet/main/install.ps1 | iex
#
# Downloads the latest release's installer, checks it against the release's SHA256SUMS and installs AiPet for this
# user, without admin rights: %LOCALAPPDATA%\AiPetApp, a Start menu entry and an entry in Installed apps; nothing
# starts at sign-in. Then it adds the AiPet marketplace and installs the aipet plugin for Claude Code and for Codex,
# whichever of them is on PATH. Running it again updates everything. Works in Windows PowerShell 5.1 and PowerShell 7.
#
# Settings are environment variables, since `irm | iex` can't pass parameters (e.g. $env:AIPET_VERSION = '0.2.0'):
#   AIPET_VERSION     install that release instead of the latest
#   AIPET_NO_PLUGINS  1: install only the app
#   AIPET_NO_START    1: don't start the pet afterwards
#   GITHUB_TOKEN      a token that can read the repositories, while they are private. Fetch the script with it too:
#     (iwr -UseBasicParsing -Headers @{ Authorization = "Bearer $env:GITHUB_TOKEN"; Accept = 'application/vnd.github.raw+json' } https://api.github.com/repos/xMarcinator/ai-pet/contents/install.ps1).Content | iex
#
# The Claude Code plugin runs its hook through Git Bash (Git for Windows). Where there's no Git Bash, the hook is
# registered in ~/.claude/settings.json instead (aipet-hook --install claude), which needs no shell.
# To build and install from a clone of the repository instead: scripts\install-from-source.ps1.
#
# Everything runs inside a script block, so nothing it sets stays behind in your PowerShell session.
& {
    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'   # the progress bar makes downloads very slow in Windows PowerShell
    # Windows PowerShell may not offer TLS 1.2 by default, and GitHub requires it
    try { [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12 } catch { }

    $repo = 'xMarcinator/ai-pet'
    $api = "https://api.github.com/repos/$repo"
    $root = Join-Path $env:LOCALAPPDATA 'AiPetApp'   # where the installer puts the app: %LOCALAPPDATA%\<its packId>
    $token = $env:GITHUB_TOKEN

    function Step([string]$Text) { Write-Host "-> $Text" -ForegroundColor Cyan }
    function Warn([string]$Text) { Write-Host "!  $Text" -ForegroundColor Yellow }

    # The path of an application on PATH (.exe or .cmd; not a .ps1 shim, which the execution policy may block).
    function Find-Exe([string]$Name) {
        $cmd = Get-Command -Name $Name -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($cmd) { $cmd.Path } else { $null }
    }

    # Downloads a release asset: with the token through the API (GitHub redirects to the file without passing the
    # token on), without it from the public download link.
    function Save-Asset([string]$Name, [string]$Path) {
        $asset = @($release.assets) | Where-Object { $_.name -eq $Name } | Select-Object -First 1
        if (-not $asset) { throw "Release $tag has no $Name." }
        if ($token) {
            Invoke-WebRequest -Uri $asset.url -Headers @{ Authorization = "Bearer $token"; Accept = 'application/octet-stream' } -OutFile $Path -UseBasicParsing
        } else {
            Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $Path -UseBasicParsing
        }
    }

    # Runs claude or codex and returns its exit code. While the repositories are private, the git they run
    # authenticates with the token through git's environment configuration, for this one command.
    function Invoke-Agent([string]$Exe, [string[]]$Arguments) {
        $ErrorActionPreference = 'Continue'   # what a native command writes to stderr isn't a PowerShell error
        $vars = @{}
        if ($token) {
            $basic = [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("x-access-token:$token"))
            $vars = @{
                GIT_CONFIG_COUNT = '1'
                GIT_CONFIG_KEY_0 = 'http.https://github.com/.extraheader'
                GIT_CONFIG_VALUE_0 = "AUTHORIZATION: basic $basic"
                CLAUDE_CODE_PLUGIN_PREFER_HTTPS = '1'
            }
        }
        $saved = @{}
        foreach ($name in @($vars.Keys)) {
            $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
            [Environment]::SetEnvironmentVariable($name, $vars[$name], 'Process')
        }
        try {
            & $Exe @Arguments | Out-Host
            $LASTEXITCODE
        } finally {
            foreach ($name in @($saved.Keys)) { [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process') }
        }
    }

    # Git Bash, which the Claude Code plugin's hook runs through.
    function Find-GitBash {
        $candidates = @()
        if ($env:CLAUDE_CODE_GIT_BASH_PATH) { $candidates += $env:CLAUDE_CODE_GIT_BASH_PATH }
        $git = Find-Exe 'git'
        if ($git) { $candidates += Join-Path (Split-Path (Split-Path $git -Parent) -Parent) 'bin\bash.exe' }   # <Git>\cmd\git.exe
        $candidates += (Join-Path $env:ProgramFiles 'Git\bin\bash.exe'), (Join-Path $env:LOCALAPPDATA 'Programs\Git\bin\bash.exe')
        foreach ($c in $candidates) { if ($c -and (Test-Path -LiteralPath $c)) { return $c } }
        $null
    }

    # Whether one of these config files still registers AiPet's hook directly (aipet-hook --install).
    function Test-Registered([string[]]$Files) {
        foreach ($f in $Files) {
            if ($f -and (Test-Path -LiteralPath $f) -and (Select-String -LiteralPath $f -Pattern 'aipet-hook' -SimpleMatch -Quiet)) { return $true }
        }
        $false
    }

    if (-not [Environment]::Is64BitOperatingSystem) { throw 'AiPet needs 64-bit Windows.' }

    # ------------------------------------------------------------ the release
    $headers = @{ Accept = 'application/vnd.github+json' }
    if ($token) { $headers['Authorization'] = "Bearer $token" }
    $url = if ($env:AIPET_VERSION) { "$api/releases/tags/v$($env:AIPET_VERSION.TrimStart('v'))" } else { "$api/releases/latest" }
    try {
        $release = Invoke-RestMethod -Uri $url -Headers $headers -UseBasicParsing
    } catch {
        $hint = if ($token) { '' } else { ' While the repository is private, set $env:GITHUB_TOKEN (see the top of this script).' }
        throw "Couldn't read the release from GitHub ($url): $($_.Exception.Message)$hint"
    }
    $tag = [string]$release.tag_name
    if (-not $tag) { throw "GitHub's answer for $url has no release tag." }
    $version = $tag.TrimStart('v')
    $setupName = "AiPet-Setup-$version-win-x64.exe"

    # ------------------------------------------------------------ the app
    $tmp = Join-Path ([IO.Path]::GetTempPath()) ('aipet-install-' + [Guid]::NewGuid().ToString('N'))
    $log = Join-Path ([IO.Path]::GetTempPath()) 'aipet-setup.log'
    New-Item -ItemType Directory -Path $tmp | Out-Null
    try {
        Step "Downloading AiPet $version"
        $setup = Join-Path $tmp $setupName
        $sums = Join-Path $tmp 'SHA256SUMS'
        Save-Asset $setupName $setup
        Save-Asset 'SHA256SUMS' $sums
        $expected = $null
        foreach ($line in @(Get-Content -LiteralPath $sums)) {
            if ($line -match '^([0-9a-fA-F]{64}) [ *](.+)$' -and $Matches[2] -eq $setupName) { $expected = $Matches[1].ToLowerInvariant(); break }
        }
        if (-not $expected) { throw "The release's SHA256SUMS has no entry for $setupName." }
        $actual = (Get-FileHash -LiteralPath $setup -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $expected) { throw "$setupName doesn't match the release's SHA256SUMS (expected $expected, got $actual)." }
        Step 'Checksum OK'

        # a pet that's still running (this one, or one from an older install) would keep the new one from starting
        $running = @(Get-Process -Name AiPet, ClaudePet -ErrorAction SilentlyContinue)
        if ($running.Count) {
            Step 'Stopping the running pet'
            $running | Stop-Process -Force -ErrorAction SilentlyContinue
            Start-Sleep -Milliseconds 700
        }

        Step "Installing to $root"
        # -PassThru with WaitForExit, not -Wait: that would also wait for anything the installer leaves running
        $p = Start-Process -FilePath $setup -ArgumentList @('--silent', '--log', ('"' + $log + '"')) -PassThru
        $null = $p.Handle   # keeps the process handle, so the exit code can still be read once it has exited
        $p.WaitForExit()
        if ($p.ExitCode -ne 0) { throw "The installer failed (exit code $($p.ExitCode)); its log is $log." }
    } finally {
        Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
    }
    $appExe = Join-Path $root 'current\AiPet.exe'
    $hook = Join-Path $root 'current\aipet-hook.exe'
    if (-not (Test-Path -LiteralPath $appExe)) { throw "The installer finished, but $appExe isn't there (its log is $log)." }

    # ------------------------------------------------------------ the plugins
    $claudeDone = $false
    $claudeRegistered = $false
    $codexDone = $false
    if ($env:AIPET_NO_PLUGINS -ne '1') {
        $claude = Find-Exe 'claude'
        $codex = Find-Exe 'codex'
        if (($claude -or $codex) -and -not (Find-Exe 'git')) {
            Warn 'Claude Code and Codex fetch plugins with git, which was not found: install Git for Windows, then run this again.'
        }

        if ($claude) {
            $claudeDir = if ($env:CLAUDE_CONFIG_DIR) { $env:CLAUDE_CONFIG_DIR } else { Join-Path $HOME '.claude' }
            if (Find-GitBash) {
                Step 'Adding the AiPet plugin to Claude Code'
                # "already added" is fine: the update below refreshes it
                $null = Invoke-Agent $claude @('plugin', 'marketplace', 'add', $repo, '--sparse', '.claude-plugin')
                $claudeDone = ((Invoke-Agent $claude @('plugin', 'marketplace', 'update', 'aipet')) -eq 0) -and
                    ((Invoke-Agent $claude @('plugin', 'install', 'aipet@aipet')) -eq 0) -and
                    ((Invoke-Agent $claude @('plugin', 'update', 'aipet@aipet')) -eq 0)
                if ($claudeDone) {
                    # a direct registration (aipet-hook --install claude, from an older install or from this script
                    # without Git Bash) next to the plugin would report every event twice
                    if ((Test-Path -LiteralPath $hook) -and (Test-Registered @(Join-Path $claudeDir 'settings.json'))) {
                        Step 'Removing the direct hook registration from Claude Code (the plugin replaces it)'
                        & $hook --uninstall claude | Out-Host
                    }
                } else {
                    Warn "Couldn't add the plugin to Claude Code. Try it yourself: claude plugin marketplace add $repo, then claude plugin install aipet@aipet"
                }
            } elseif (Test-Path -LiteralPath $hook) {
                Step 'Registering the hook with Claude Code (the plugin needs Git Bash, which was not found)'
                & $hook --install claude | Out-Host
                $claudeRegistered = $LASTEXITCODE -eq 0
            } else {
                Warn "The Claude Code plugin needs Git Bash (Git for Windows), which was not found, and $hook is missing."
            }
        }

        if ($codex) {
            Step 'Adding the AiPet plugin to Codex'
            $null = Invoke-Agent $codex @('plugin', 'marketplace', 'add', $repo, '--sparse', '.agents/plugins')
            $codexDone = ((Invoke-Agent $codex @('plugin', 'marketplace', 'upgrade', 'aipet')) -eq 0) -and
                ((Invoke-Agent $codex @('plugin', 'add', 'aipet@aipet')) -eq 0)
            if ($codexDone) {
                $codexDir = if ($env:CODEX_HOME) { $env:CODEX_HOME } else { Join-Path $HOME '.codex' }
                if ((Test-Path -LiteralPath $hook) -and (Test-Registered @((Join-Path $codexDir 'config.toml'), (Join-Path $codexDir 'hooks.json')))) {
                    Step 'Removing the direct hook registration from Codex (the plugin replaces it)'
                    & $hook --uninstall codex | Out-Host
                }
            } else {
                Warn "Couldn't add the plugin to Codex. Try it yourself: codex plugin marketplace add $repo, then codex plugin add aipet@aipet"
            }
        }
    }

    if ($env:AIPET_NO_START -ne '1') {
        $stub = Join-Path $root 'AiPet.exe'   # starts current\AiPet.exe
        Start-Process -FilePath $(if (Test-Path -LiteralPath $stub) { $stub } else { $appExe })
    }
    $old = Join-Path $env:LOCALAPPDATA 'Programs\AiPet'
    if (Test-Path -LiteralPath (Join-Path $old 'AiPet.exe')) {
        Warn "An older AiPet install is still in $old; you can delete that folder."
    }

    Write-Host ''
    Write-Host "AiPet $version is installed." -ForegroundColor Green -NoNewline
    Write-Host ' It shows your chats while it runs (Start menu: AiPet); nothing starts it at sign-in.'
    if ($claudeDone) { Write-Host 'Claude Code: new sessions pick up the plugin (in an open one, run /reload-plugins).' }
    if ($claudeRegistered) { Write-Host 'Claude Code: new sessions pick up the hook. Install Git for Windows and run this again to switch to the plugin.' }
    if ($codexDone) {
        Write-Host 'One step left for Codex:' -ForegroundColor Yellow -NoNewline
        Write-Host ' Codex runs only hooks you trust. Run codex, open /hooks and trust the aipet@aipet hooks'
        Write-Host '(or choose Review hooks when Codex asks at startup). Then restart the ChatGPT app if it is open.'
    }
    if (-not ($claudeDone -or $claudeRegistered -or $codexDone) -and $env:AIPET_NO_PLUGINS -ne '1') {
        Write-Host "No plugin was added (Claude Code and Codex weren't found, or adding failed): see the README."
    }
}
