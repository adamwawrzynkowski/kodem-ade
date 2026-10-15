# Detached Windows updater. Arguments are passed directly by Kodem, never interpolated as code.
param(
    [Parameter(Mandatory = $true)][int]$ParentPid,
    [Parameter(Mandatory = $true)][string]$Destination,
    [Parameter(Mandatory = $true)][string]$Package,
    [Parameter(Mandatory = $true)][string]$Stage
)
$ErrorActionPreference = 'Stop'
$backup = Join-Path $Stage 'previous'
if (-not (Test-Path -LiteralPath $Destination -PathType Container) -or
    -not (Test-Path -LiteralPath (Join-Path $Package 'kodem.exe') -PathType Leaf) -or
    -not (Test-Path -LiteralPath (Join-Path $Package 'kodem-host.exe') -PathType Leaf) -or
    (Test-Path -LiteralPath $backup)) {
    Write-Output 'Invalid update arguments.'
    exit 1
}
New-Item -ItemType File -Path (Join-Path $Stage 'ready') | Out-Null
# Never replace an app that is still saving its state. Time out without touching it.
$parent = Get-Process -Id $ParentPid -ErrorAction SilentlyContinue
if ($parent -and -not $parent.WaitForExit(120000)) {
    Write-Output 'Desktop did not exit; update was not installed.'
    exit 1
}
# Windows refuses to move a folder while one of its programs runs. The host can take a moment
# to exit after the desktop stops it.
$moved = $false
for ($attempt = 0; $attempt -lt 60 -and -not $moved; $attempt++) {
    try {
        Move-Item -LiteralPath $Destination -Destination $backup
        $moved = $true
    } catch {
        Start-Sleep -Seconds 1
    }
}
if (-not $moved) {
    Write-Output 'The installed app is still in use; update was not installed.'
    exit 1
}
try {
    Move-Item -LiteralPath $Package -Destination $Destination
    Start-Process -FilePath (Join-Path $Destination 'kodem.exe') -WorkingDirectory $Destination
} catch {
    Write-Output "Update failed: $_"
    if (Test-Path -LiteralPath $Destination) {
        Move-Item -LiteralPath $Destination -Destination (Join-Path $Stage 'failed')
    }
    Move-Item -LiteralPath $backup -Destination $Destination
    Start-Process -FilePath (Join-Path $Destination 'kodem.exe') -WorkingDirectory $Destination -ErrorAction SilentlyContinue
    Write-Output "Restored the previous app. Log: $(Join-Path $Stage 'install.log')"
    exit 1
}
Remove-Item -LiteralPath $backup -Recurse -Force -ErrorAction SilentlyContinue
# This process still writes install.log, so the stage folder can outlive it with only the log.
Remove-Item -LiteralPath $Stage -Recurse -Force -ErrorAction SilentlyContinue
