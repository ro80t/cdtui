# Run the picker straight from this clone and cd where you picked, without
# installing anything.
#
#   .\scripts\dev.ps1                 # start at the current directory
#   .\scripts\dev.ps1 packages        # start somewhere else
#
# Unlike scripts/dev.sh, this does NOT need to be dot-sourced: a PowerShell
# script runs in a child scope of the same process, not a subprocess, and
# Set-Location is session state rather than scope state, so it persists to
# the caller either way. Measured, not assumed — calling a plain .ps1 (no
# leading ". ") and checking Get-Location afterwards shows the change stuck.
#
# Once the wrapper from `cdt --init powershell` is installed, `cargo build`
# + `cdt` is the faster loop; this is for before that.

$root = Split-Path -Parent $PSScriptRoot
$manifest = Join-Path $root 'Cargo.toml'
if (-not (Test-Path $manifest)) {
    Write-Error "dev.ps1: cannot find the cdtui workspace next to this script."
    exit 1
}

# cargo writes progress and the UI to stderr, so stdout is only the picked
# path. Nothing printed means the picker was quit: stay put.
$dir = cargo run -q --manifest-path $manifest -- @args

if ($dir -and (Test-Path -LiteralPath $dir -PathType Container)) {
    Set-Location -LiteralPath $dir
} elseif ($dir) {
    Write-Output $dir
}
