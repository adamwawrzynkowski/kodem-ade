# Installs this Kodem package for the current user in Local AppData\Programs with a Start menu
# shortcut. Run with -Uninstall to remove both. Settings and tasks stay.
param([switch]$Uninstall)
$ErrorActionPreference = 'Stop'
$source = (Resolve-Path -LiteralPath $PSScriptRoot).Path
$release = Get-Content -Raw -LiteralPath (Join-Path $source 'release.json') | ConvertFrom-Json
$name = $release.name
if (-not $name) { throw 'release.json is missing or incomplete.' }
$destination = Join-Path $env:LOCALAPPDATA (Join-Path 'Programs' $name)
$shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) "$name.lnk"
# Windows cannot replace a program that is running.
$running = Get-Process -Name kodem, kodem-host -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -and $_.Path.StartsWith("$destination\", [StringComparison]::OrdinalIgnoreCase) }
if ($running) { throw "Quit $name, wait for its tasks to finish, and try again." }
if ($Uninstall) {
    Remove-Item -LiteralPath $shortcut -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $destination -Recurse -Force -ErrorAction SilentlyContinue
    Write-Output "Removed $name."
    exit 0
}
if ($source -ne $destination) {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
    $stage = "$destination.installing"
    Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue
    Copy-Item -LiteralPath $source -Destination $stage -Recurse
    if (Test-Path -LiteralPath $destination) { Remove-Item -LiteralPath $destination -Recurse -Force }
    Move-Item -LiteralPath $stage -Destination $destination
}
$shell = New-Object -ComObject WScript.Shell
$link = $shell.CreateShortcut($shortcut)
$link.TargetPath = Join-Path $destination 'kodem.exe'
$link.WorkingDirectory = $destination
$link.IconLocation = (Join-Path $destination 'kodem.exe') + ',0'
$link.Save()
Write-Output "Installed $name in $destination"
Write-Output 'Open it from the Start menu.'
