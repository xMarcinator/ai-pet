# The build environment for cargo in rust/, the Windows twin of cargo-env.sh: sccache as the compiler wrapper, from
# the environment only, never from a checked-in .cargo/config.toml.
#
#   pwsh ./scripts/cargo-env.ps1 | Out-String | Invoke-Expression
#
# (Windows PowerShell 5.1 may need -ExecutionPolicy Bypass.) With a working sccache it prints $env: lines for
# RUSTC_WRAPPER (its absolute path), SCCACHE_DIR (%LOCALAPPDATA%\sccache), SCCACHE_CACHE_SIZE (15G) and
# SCCACHE_ALLOW_INCREMENTAL; a SCCACHE_DIR or SCCACHE_CACHE_SIZE you set yourself is kept. Without sccache it prints
# nothing, says so on stderr and exits 0.
$ErrorActionPreference = 'Stop'

# `mise which sccache` (mise.toml pins it), else the sccache on PATH; it must run from the drive's root, since cargo
# starts the wrapper in other folders, where a mise shim has no version.
function Resolve-Sccache {
    $found = $null
    if (Get-Command mise -ErrorAction SilentlyContinue) {
        # a notice from mise on stderr mustn't throw
        $ErrorActionPreference = 'Continue'
        Push-Location (Join-Path $PSScriptRoot '..')
        try { $found = (& mise which sccache 2> $null | Select-Object -First 1) } catch { $found = $null }
        finally { Pop-Location }
        if ($LASTEXITCODE -ne 0) { $found = $null }
        $ErrorActionPreference = 'Stop'
    }
    if (-not $found -or -not [IO.Path]::IsPathRooted($found)) {
        $cmd = Get-Command sccache -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
        $found = if ($cmd) { $cmd.Source } else { $null }
    }
    if (-not $found -or -not [IO.Path]::IsPathRooted($found)) { return $null }
    Push-Location ([IO.Path]::GetPathRoot($found))
    try {
        & $found --version *> $null
        if ($LASTEXITCODE -eq 0) { return $found }
    } catch {
    } finally {
        Pop-Location
    }
    return $null
}

$wrapper = Resolve-Sccache
if (-not $wrapper) {
    [Console]::Error.WriteLine('sccache not found; building without a compiler cache')
    exit 0
}

$cacheRoot = if ($env:LOCALAPPDATA) { $env:LOCALAPPDATA } else { [Environment]::GetFolderPath('LocalApplicationData') }
$dir = if ($env:SCCACHE_DIR) { $env:SCCACHE_DIR } else { Join-Path $cacheRoot 'sccache' }
$size = if ($env:SCCACHE_CACHE_SIZE) { $env:SCCACHE_CACHE_SIZE } else { '15G' }

function Quote([string] $value) { "'" + $value.Replace("'", "''") + "'" }

"`$env:RUSTC_WRAPPER = $(Quote $wrapper)"
"`$env:SCCACHE_DIR = $(Quote $dir)"
"`$env:SCCACHE_CACHE_SIZE = $(Quote $size)"
"`$env:SCCACHE_ALLOW_INCREMENTAL = '1'"
