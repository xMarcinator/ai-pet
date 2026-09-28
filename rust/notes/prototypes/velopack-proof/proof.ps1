# The Velopack proof's steps on Windows. .github/workflows/velopack-proof.yml runs them, and the results are in
# rust/proofs/velopack.md. Run them in this order, as one user, where AiPet has never been installed (a fresh runner,
# or Windows Sandbox: they change %LOCALAPPDATA%\AiPet and %LOCALAPPDATA%\AiPetApp):
#
#   proof.ps1 install              seed the data folder, then install the .NET release <From> with its Setup
#   proof.ps1 update -Mode <mode>  update it to the Rust release <To> the way the installed .NET pet does
#   proof.ps1 check                what the update left behind
#   proof.ps1 restart              start the app again from its Start menu entry
#   proof.ps1 uninstall            uninstall it with the command Settings > Apps runs
#   proof.ps1 logs                 print Velopack's logs and the stand-in's notes, and stop what's still running
#
# -Proof is the folder the workflow's pack job makes: setup-<From>.exe, feed\ (vpk's output for both releases),
# rust\AiPet.exe (the Rust stand-in, src/main.rs) and driver\ (the update driver, driver/Program.cs). -From and -To
# are the two versions (by default $env:FROM_VERSION and $env:TO_VERSION). A failed check prints ::error:: and the
# step exits 1 once all its checks have run. Works in PowerShell 7 and Windows PowerShell 5.1.
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('install', 'update', 'check', 'restart', 'uninstall', 'logs')]
    [string]$Step,
    [string]$Proof = 'proof',
    # full: the feed has the full package only. delta: it lists both, but only the delta's file is there, so a delta
    # the client can't apply, or won't take, fails the update instead of falling back to the full package.
    [ValidateSet('full', 'delta')]
    [string]$Mode = 'full',
    [string]$From = $env:FROM_VERSION,
    [string]$To = $env:TO_VERSION,
    [string]$Logs = (Join-Path ([IO.Path]::GetTempPath()) 'velopack-proof-logs')
)

$ErrorActionPreference = 'Stop'
if (-not $From -or -not $To) { throw 'Give the versions: -From and -To, or FROM_VERSION and TO_VERSION' }
if (Test-Path -LiteralPath $Proof) { $Proof = (Resolve-Path -LiteralPath $Proof).Path }

$Root = Join-Path $env:LOCALAPPDATA 'AiPetApp'  # the install: %LOCALAPPDATA%\<packId>
$Current = Join-Path $Root 'current'
$Data = Join-Path $env:LOCALAPPDATA 'AiPet'     # the data folder (Paths.cs)
$Notes = Join-Path $Data 'velopack-proof'       # the stand-in's notes (src/main.rs)
$UninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\AiPetApp'
$Before = Join-Path $Logs 'data-before.txt'     # the data folder as it was before the install
New-Item -ItemType Directory -Force -Path $Logs | Out-Null
$failed = New-Object System.Collections.Generic.List[string]

function Check([bool]$Ok, [string]$What) {
    if ($Ok) { Write-Host "ok: $What" } else { Write-Host "::error::$What"; $failed.Add($What) }
}

function Info([string]$What, $Value) { Write-Host "info: ${What}: $Value" }

function Done {
    if ($failed.Count -gt 0) { Write-Host "$($failed.Count) check(s) failed"; exit 1 }
}

function WaitFor([scriptblock]$Until, [int]$Seconds, [string]$For) {
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    while (-not (& $Until)) {
        if ([DateTime]::UtcNow -gt $deadline) { Write-Host "::error::Waited $Seconds s for $For"; exit 1 }
        Start-Sleep -Milliseconds 500
    }
    Write-Host "ok: $For"
}

# Starts a program and waits for it (not for what it starts), then gives its exit code.
function Run([string]$Exe, [string[]]$Arguments) {
    $quoted = @($Arguments | ForEach-Object { if ($_ -match '\s') { '"' + $_ + '"' } else { $_ } })
    Write-Host "> $Exe $($quoted -join ' ')"
    $process = Start-Process -FilePath $Exe -ArgumentList $quoted -PassThru
    $null = $process.Handle  # without it, the exit code is lost once the process has exited
    $process.WaitForExit()
    $process.ExitCode
}

# The stand-in's notes for one event (src/main.rs): one line per run.
function Notes([string]$Name) {
    $file = Join-Path $Notes "$Name.txt"
    if (Test-Path -LiteralPath $file) { @(Get-Content -LiteralPath $file) } else { @() }
}

# Every file in the data folder but the stand-in's notes, with its SHA-256.
function Snapshot {
    if (-not (Test-Path -LiteralPath $Data)) { return @() }
    @(Get-ChildItem -LiteralPath $Data -Recurse -File -Force |
        Where-Object { -not $_.FullName.StartsWith("$Notes\", [StringComparison]::OrdinalIgnoreCase) } |
        ForEach-Object { '{0} {1}' -f $_.FullName.Substring($Data.Length + 1), (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash } |
        Sort-Object)
}

# What changed in the data folder since the install: nothing, if it's untouched.
function DataChanges {
    $was = @(Get-Content -LiteralPath $Before)
    $now = @(Snapshot)
    if ($now.Count -eq 0) { return @('the data folder is empty or gone') }
    @(Compare-Object -ReferenceObject $was -DifferenceObject $now | ForEach-Object { "$($_.SideIndicator) $($_.InputObject)" })
}

function InstalledVersion {
    $manifest = Join-Path $Current 'sq.version'
    if (-not (Test-Path -LiteralPath $manifest)) { return '(no sq.version)' }
    ([xml](Get-Content -Raw -LiteralPath $manifest)).package.metadata.version
}

function Listing([string]$Dir) {
    if (-not (Test-Path -LiteralPath $Dir)) { return '(none)' }
    (@(Get-ChildItem -LiteralPath $Dir -Force | ForEach-Object { if ($_.PSIsContainer) { "$($_.Name)\" } else { "$($_.Name) ($($_.Length) bytes)" } }) -join ', ')
}

switch ($Step) {
    'install' {
        if (Test-Path -LiteralPath $Root) { throw "$Root exists: run this where AiPet has never been installed" }
        if (Test-Path -LiteralPath $Data) { throw "$Data exists: run this where AiPet has never been installed" }
        # a user's data folder, with the files the pet keeps there; an update mustn't touch any of them
        $files = [ordered]@{
            'config.json'                 = '{"Left":120,"Top":340,"Pills":true,"OnTop":true,"Avatar":"cat","Music":false,"WindowHeight":320}'
            'jira.json'                   = '{"proof":"the Jira settings"}'
            'github.json'                 = '{"proof":"the GitHub settings"}'
            'aipet.log'                   = "proof: the pet's log`r`n"
            'hook-events.log'             = "proof: the hook events`r`n"
            'avatars\proof\avatar.json'   = '{"proof":"an avatar"}'
        }
        foreach ($name in $files.Keys) {
            $path = Join-Path $Data $name
            New-Item -ItemType Directory -Force -Path (Split-Path -Parent $path) | Out-Null
            [IO.File]::WriteAllText($path, $files[$name])
        }
        Snapshot | Set-Content -LiteralPath $Before
        Info 'data folder' (Listing $Data)

        $code = Run (Join-Path $Proof "setup-$From.exe") @('--silent', '--log', (Join-Path $Logs 'setup.log'))
        if ($code -ne 0) { Write-Host "::error::Setup $From exited with $code"; exit 1 }
        Check (Test-Path -LiteralPath (Join-Path $Current 'AiPet.dll')) "Setup installed the .NET app in $Current (AiPet.dll)"
        Check ((InstalledVersion) -eq $From) "current\sq.version says $From"
        Info 'current\aipet-hook.exe' (Test-Path -LiteralPath (Join-Path $Current 'aipet-hook.exe'))
        Info "$Root" (Listing $Root)
        # the base package a delta update starts from
        Info 'packages\' (Listing (Join-Path $Root 'packages'))
        Info 'Settings > Apps' "$((Get-ItemProperty -LiteralPath $UninstallKey).DisplayName) $((Get-ItemProperty -LiteralPath $UninstallKey).DisplayVersion)"
        Info 'AiPet processes after a silent Setup' (@(Get-Process AiPet -ErrorAction SilentlyContinue).Count)
    }

    'update' {
        # this release's packages, as each release's feed lists them (release.yml)
        $all = Get-Content -Raw -LiteralPath (Join-Path $Proof 'feed\releases.win.json') | ConvertFrom-Json
        $assets = @($all.Assets | Where-Object { $_.Version -eq $To -and ($Mode -eq 'delta' -or $_.Type -eq 'Full') })
        $full = @($assets | Where-Object { $_.Type -eq 'Full' })
        $delta = @($assets | Where-Object { $_.Type -eq 'Delta' })
        if ($full.Count -ne 1 -or ($Mode -eq 'delta' -and $delta.Count -ne 1)) {
            throw "vpk's feed doesn't have the $To packages the $Mode update needs: $($all.Assets | ConvertTo-Json -Depth 4)"
        }
        $feed = Join-Path ([IO.Path]::GetTempPath()) "velopack-proof-feed-$Mode"
        New-Item -ItemType Directory -Force -Path $feed | Out-Null
        ConvertTo-Json -InputObject @{ Assets = $assets } -Depth 4 | Set-Content -LiteralPath (Join-Path $feed 'releases.win.json')
        $files = if ($Mode -eq 'delta') { $delta } else { $full }
        foreach ($asset in $files) { Copy-Item -LiteralPath (Join-Path $Proof "feed\$($asset.FileName)") -Destination $feed }
        foreach ($asset in $assets) { Info "listed" "$($asset.FileName) ($($asset.Type), $($asset.Size) bytes)" }
        Info "feed ($Mode)" (Listing $feed)

        & (Join-Path $Proof 'driver\velopack-proof-driver.exe') $Root $feed
        if ($LASTEXITCODE -ne 0) { Write-Host "::error::The update driver failed (exit code $LASTEXITCODE)"; exit 1 }
        # Update.exe waits for the driver to exit, applies the package and starts the new AiPet.exe
        WaitFor { @(Notes 'restarted').Count -gt 0 } 180 'Update.exe to apply the update and start the app again'
        WaitFor { @(Get-Process Update -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$Root\*" }).Count -eq 0 } 60 'Update.exe to exit'
    }

    'check' {
        $exe = Join-Path $Current 'AiPet.exe'
        $stub = (Get-FileHash -LiteralPath (Join-Path $Proof 'rust\AiPet.exe') -Algorithm SHA256).Hash
        Check ((Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash -eq $stub) "current\AiPet.exe is the Rust stand-in (SHA-256 $stub)"
        Check (-not (Test-Path -LiteralPath (Join-Path $Current 'AiPet.dll'))) "the .NET app's files are gone from current\"
        Check ((InstalledVersion) -eq $To) "current\sq.version says $To"
        $updated = @(Notes 'updated')
        Check ($updated.Count -eq 1 -and $updated[0] -like "* version=$To *") "Update.exe ran the Rust app's --veloapp-updated hook once: $updated"
        $restarted = @(Notes 'restarted')
        Check ($restarted.Count -eq 1 -and $restarted[0] -like "* version=$To *") "Velopack told the restarted app it was restarted: $restarted"
        $start = @(Notes 'start')
        Check ($start.Count -eq 1 -and $start[0] -like "* exe=$exe *") "Update.exe started the Rust app from current\: $start"
        $changes = @(DataChanges)
        Check ($changes.Count -eq 0) "the data folder is as it was $($changes -join '; ')"
        Info 'current\' (Listing $Current)
        Info 'packages\ (the base of the next delta)' (Listing (Join-Path $Root 'packages'))
        Info 'Settings > Apps' "$((Get-ItemProperty -LiteralPath $UninstallKey).DisplayName) $((Get-ItemProperty -LiteralPath $UninstallKey).DisplayVersion)"
        Info 'Update.exe' ((Get-Item -LiteralPath (Join-Path $Root 'Update.exe')).VersionInfo.ProductVersion)
    }

    'restart' {
        # the Start menu entry Setup made (--shortcuts StartMenuRoot): it starts the launcher AiPetApp\AiPet.exe, which
        # starts current\AiPet.exe
        $programs = [Environment]::GetFolderPath('Programs')
        $shell = New-Object -ComObject WScript.Shell
        $links = @(Get-ChildItem -LiteralPath $programs -Filter *.lnk -Recurse |
            Where-Object { $shell.CreateShortcut($_.FullName).TargetPath -like "$Root\*" })
        Check ($links.Count -eq 1) "one Start menu entry starts AiPet: $($links | ForEach-Object { "$($_.FullName) -> $($shell.CreateShortcut($_.FullName).TargetPath)" })"
        $target = if ($links.Count -gt 0) { $links[0].FullName } else { Join-Path $Root 'AiPet.exe' }
        $starts = @(Notes 'start').Count
        Write-Host "> $target"
        Start-Process -FilePath $target
        WaitFor { @(Notes 'start').Count -gt $starts } 60 'the app to start again'
        $start = @(Notes 'start')
        Check ($start[-1] -like "* exe=$(Join-Path $Current 'AiPet.exe') *") "the launcher started the Rust app from current\: $($start[-1])"
        Check (@(Notes 'restarted').Count -eq 1) "Velopack doesn't call an ordinary start a restart"
    }

    'uninstall' {
        # what Settings > Apps runs, made silent: a runner has nobody to close the "uninstalled" dialog
        $command = (Get-ItemProperty -LiteralPath $UninstallKey).UninstallString
        Info 'UninstallString' $command
        if ($command -notmatch '^"([^"]+)"\s*(.*)$') { throw "An UninstallString this doesn't know: $command" }
        $exe = $Matches[1]
        $arguments = @($Matches[2] -split '\s+' | Where-Object { $_ }) + @('--silent', '--log', (Join-Path $Logs 'uninstall.log'))
        $code = Run $exe $arguments
        Check ($code -eq 0) "the uninstaller exited with 0 ($code)"
        $uninstall = @(Notes 'uninstall')
        Check ($uninstall.Count -eq 1 -and $uninstall[0] -like "* version=$To *") "the uninstaller ran the Rust app's uninstall hook: $uninstall"
        Check (-not (Test-Path -LiteralPath $Current)) "the app is gone from $Root"
        Check (-not (Test-Path -LiteralPath $UninstallKey)) 'Settings > Apps no longer lists AiPet'
        $changes = @(DataChanges)
        Check ($changes.Count -eq 0) "the data folder is still as it was $($changes -join '; ')"
        Info "$Root" (Listing $Root)
    }

    'logs' {
        # a failed update starts the old app again, and a pet would run until the job ends
        Get-Process AiPet, Update -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$Root\*" } | ForEach-Object {
            Info 'still running' "$($_.Name) (pid $($_.Id)) $($_.Path)"
            Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
        }
        foreach ($dir in @((Join-Path $env:LOCALAPPDATA 'velopack'), $Notes)) {
            if (Test-Path -LiteralPath $dir) { Copy-Item -Recurse -Force -LiteralPath $dir -Destination (Join-Path $Logs (Split-Path -Leaf $dir)) }
        }
        Get-ChildItem -LiteralPath $Logs -Recurse -File | Where-Object { $_.Extension -in '.log', '.txt' } | ForEach-Object {
            Write-Host "::group::$($_.FullName)"
            Get-Content -LiteralPath $_.FullName | ForEach-Object { Write-Host $_ }
            Write-Host '::endgroup::'
        }
    }
}
Done
