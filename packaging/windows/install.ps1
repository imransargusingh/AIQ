# AIQ per-user install for Windows (private-alpha design A3). No admin rights; refuses to run elevated.
#
#   install.cmd (double-click)                                        the same, for people who don't use a terminal
#   powershell -ExecutionPolicy Bypass -File .\install.ps1            install from the unzipped release
#   powershell -ExecutionPolicy Bypass -File .\install.ps1 -NoLaunch  same, without starting import or AIQ home
#
# What it does, in order, printing each change:
#   1. copies aiq.exe and aiq-home.exe to %USERPROFILE%\.aiq\bin (keeps any old copy as .old), and upgrade.ps1
#   2. adds that folder to your user PATH (not the machine PATH)
#   3. adds an "AIQ" Start menu shortcut for AIQ home
#   4. records 2 and 3 in %USERPROFILE%\.aiq\windows-install.json, so `aiq uninstall --all` can put them back
#   5. runs `aiq install --yes`: connects every AI tool it finds (Copilot, Claude Code), lists each file changed,
#      and turns background judging on unless the person turned it off (undo: aiq config set judge off)
#   6. starts `aiq import` in the background and opens AIQ home
# Undo: aiq uninstall --all (your data stays; aiq forget --all deletes it).
[CmdletBinding()]
# -AllowElevated exists only for CI runners, which run as administrator.
param([switch]$NoLaunch, [switch]$AllowElevated)
$ErrorActionPreference = 'Stop'

$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $AllowElevated -and $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Host 'AIQ installs for you only. Run this from a normal (not administrator) PowerShell window.'
    exit 2
}

$src = $PSScriptRoot
foreach ($exe in 'aiq.exe', 'aiq-home.exe') {
    if (-not (Test-Path (Join-Path $src $exe))) {
        Write-Host "$exe is missing next to install.ps1. Unzip the whole release, then run install.ps1 again."
        exit 1
    }
}

$home_ = Join-Path $env:USERPROFILE '.aiq'
$bin = Join-Path $home_ 'bin'
New-Item -ItemType Directory -Force -Path $bin | Out-Null

# 1. Copy the exes. A running core or AIQ home holds its exe open, so stop them first.
Get-Process -Name 'aiq', 'aiq-home' -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -and $_.Path.StartsWith($bin, [StringComparison]::OrdinalIgnoreCase) } |
    Stop-Process -Force -ErrorAction SilentlyContinue
foreach ($exe in 'aiq.exe', 'aiq-home.exe') {
    $to = Join-Path $bin $exe
    if (Test-Path $to) { Copy-Item -Force $to "$to.old" }
    Copy-Item -Force (Join-Path $src $exe) $to
    Write-Host "Copied: $to"
}
# The hook budget in `aiq status` counts from the last time the exes changed (same line shape as upgrade.ps1).
$ver = try { ((& (Join-Path $bin 'aiq.exe') version) -replace '^aiq\s+', '').Trim() } catch { 'unknown' }
Add-Content -Path (Join-Path $home_ 'upgrade-log.txt') -Value ("{0:u} Installed AIQ {1} (install.ps1)." -f (Get-Date).ToUniversalTime(), $ver)
$up = Join-Path $src 'upgrade.ps1'
if (Test-Path $up) {
    Copy-Item -Force $up (Join-Path $bin 'upgrade.ps1')
    Write-Host "Copied: $(Join-Path $bin 'upgrade.ps1')"
}

# 2. User PATH. SetEnvironmentVariable at User scope writes HKCU\Environment and tells running programs.
$record = @{ path_added = $false; shortcut = $null }
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$parts = @()
if ($userPath) { $parts = $userPath.Split(';') | Where-Object { $_ } }
if ($parts -notcontains $bin) {
    [Environment]::SetEnvironmentVariable('Path', (($parts + $bin) -join ';'), 'User')
    $record.path_added = $true
    Write-Host "Changed: your user PATH (added $bin). New terminals see it."
}
$env:Path = "$env:Path;$bin"

# 3. Start menu shortcut.
$programs = [Environment]::GetFolderPath('Programs')
$lnk = Join-Path $programs 'AIQ.lnk'
$shell = New-Object -ComObject WScript.Shell
$s = $shell.CreateShortcut($lnk)
$s.TargetPath = Join-Path $bin 'aiq-home.exe'
$s.WorkingDirectory = $bin
$s.Description = 'AIQ: your private AI usage coach'
$s.Save()
$record.shortcut = $lnk
Write-Host "Added: Start menu shortcut $lnk"

# 4. Record what this script changed, merged with any earlier record (a reinstall must not forget the first PATH add).
$recFile = Join-Path $home_ 'windows-install.json'
if (Test-Path $recFile) {
    try {
        $old = Get-Content -Raw $recFile | ConvertFrom-Json
        if ($old.path_added) { $record.path_added = $true }
    } catch { }
}
$record.bin = $bin
$record | ConvertTo-Json | Set-Content -Encoding UTF8 $recFile

# 5. Connect every AI tool found. It prints each file it changed and how to undo.
& (Join-Path $bin 'aiq.exe') install --yes
$connect = $LASTEXITCODE

Write-Host ''
Write-Host 'Undo everything: aiq uninstall --all   (your data stays; aiq forget --all deletes it)'

# 6. History and first run.
if (-not $NoLaunch) {
    Start-Process -WindowStyle Hidden -FilePath (Join-Path $bin 'aiq.exe') -ArgumentList 'import'
    Start-Process -FilePath (Join-Path $bin 'aiq-home.exe')
}
exit $connect
