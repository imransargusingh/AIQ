# AIQ per-user install for Windows (private-alpha design A3). No admin rights; refuses to run elevated.
#
#   install.cmd (double-click)                                        the same, for people who don't use a terminal
#   powershell -ExecutionPolicy Bypass -File .\install.ps1            install from the unzipped release
#   powershell -ExecutionPolicy Bypass -File .\install.ps1 -NoLaunch  same, without starting import or AIQ home
#
# What it does, in order, printing each change:
#   1. copies aiq.exe and aiq-home.exe to %USERPROFILE%\.aiq\bin (keeps any old copy as .old), upgrade.ps1, and
#      aiq.vsix (AIQ's VS Code extension, which step 5 adds to VS Code when it is installed)
#   2. adds that folder to your user PATH (not the machine PATH)
#   3. adds an "AIQ" Start menu shortcut for AIQ home
#   4. records 2 and 3 in %USERPROFILE%\.aiq\windows-install.json, so `aiq uninstall --all` can put them back
#   5. runs `aiq install --yes`: connects every AI tool it finds (Copilot, Claude Code, VS Code), lists each file changed,
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

# A line in upgrade-log.txt (same shape as upgrade.ps1's). Never throws: a held log must not end the install after the
# exes were copied; the line goes to upgrade-log-held.txt, which upgrade.ps1's next line brings back (AIQ-UPDATE-LOG-HELD).
function Log-Line([string]$m) {
    $line = "{0:u} {1}" -f (Get-Date).ToUniversalTime(), $m
    $why = ''
    foreach ($i in 1..5) {
        try { Add-Content -Path (Join-Path $home_ 'upgrade-log.txt') -Value $line -ErrorAction Stop; return } catch { $why = $_.Exception.Message; Start-Sleep -Milliseconds 100 }
    }
    try { Add-Content -Path (Join-Path $home_ 'upgrade-log-held.txt') -Value "$line (upgrade-log.txt could not be written: $why)" -ErrorAction Stop } catch { }
}
# The zip's file times are the build runner's UTC read as local time, hours off (laptop 2026-11-11: 8 h ahead), so a
# copied file carries the time it was installed. Never throws.
function Stamp-Now([string]$path) {
    try { (Get-Item -LiteralPath $path).LastWriteTimeUtc = [DateTime]::UtcNow } catch { }
}

# 1. Copy the exes. A running core or AIQ home holds its exe open, so stop them first.
Get-Process -Name 'aiq', 'aiq-home' -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -and $_.Path.StartsWith($bin, [StringComparison]::OrdinalIgnoreCase) } |
    Stop-Process -Force -ErrorAction SilentlyContinue
foreach ($exe in 'aiq.exe', 'aiq-home.exe') {
    $to = Join-Path $bin $exe
    if (Test-Path $to) { Copy-Item -Force $to "$to.old" }
    Copy-Item -Force (Join-Path $src $exe) $to
    Stamp-Now $to
    Write-Host "Copied: $to"
}
# The hook budget in `aiq status` counts from the last time the exes changed (same line shape as upgrade.ps1).
$ver = try { ((& (Join-Path $bin 'aiq.exe') version) -replace '^aiq\s+', '').Trim() } catch { 'unknown' }
Log-Line "Installed AIQ $ver (install.ps1)."
# An update downloaded before this install and waiting in .aiq\update is now this version or older: drop it, so
# `aiq update` and AIQ home stop offering it and auto-update never swaps it in over the same or a newer build
# (laptop, 2026-11-04: 0.1.2 staged by 0.1.1's check, then 0.1.2 installed with install.cmd). A newer one stays.
$stateFile = Join-Path $home_ 'update.json'
$installedV = $null
if ([version]::TryParse($ver, [ref]$installedV) -and (Test-Path $stateFile)) {
    try {
        $st = @{}
        (Get-Content -Raw $stateFile | ConvertFrom-Json).PSObject.Properties | ForEach-Object { $st[$_.Name] = $_.Value }
        $stagedV = $null
        $tag = if ($st.available) { "$($st.available)" } elseif ("$($st.staged)" -match '^aiq-(.+)-windows-x64\.zip$') { $Matches[1] } else { '' }
        if ($st.state -eq 'waiting' -and [version]::TryParse(($tag -replace '^v', ''), [ref]$stagedV) -and $stagedV -le $installedV) {
            foreach ($k in 'available', 'staged', 'blocked') { $st.Remove($k) }
            $st.state = 'current'
            $st.installed = $ver
            $tmp = "$stateFile.tmp"
            $st | ConvertTo-Json | Set-Content -Encoding UTF8 $tmp
            Move-Item -Force $tmp $stateFile
            Remove-Item -Recurse -Force (Join-Path $home_ 'update') -ErrorAction SilentlyContinue
            Log-Line "AIQ-INSTALL-CLEARED-WAITING: dropped the waiting $tag; AIQ $ver is installed."
            Write-Host "Removed: the waiting update $tag (AIQ $ver is now installed)."
        }
    } catch {
        Write-Host "Could not read $stateFile ($($_.Exception.Message)); left as it is."
    }
}
foreach ($f in 'upgrade.ps1', 'aiq.vsix') {
    $from = Join-Path $src $f
    if (Test-Path $from) {
        Copy-Item -Force $from (Join-Path $bin $f)
        Stamp-Now (Join-Path $bin $f)
        Write-Host "Copied: $(Join-Path $bin $f)"
    }
}
# aiq-statusline.exe, Copilot's status-line program, stays the same file across releases (design auto-update U5n:
# Defender's ransomware rule refused a freshly installed aiq.exe started by cmd.exe). Copied only when it differs;
# a copy that fails (Copilot running it this instant) keeps the one in place.
$slFrom = Join-Path $src 'aiq-statusline.exe'
$slTo = Join-Path $bin 'aiq-statusline.exe'
if ((Test-Path $slFrom) -and (-not (Test-Path $slTo) -or (Get-FileHash $slFrom).Hash -ne (Get-FileHash $slTo).Hash)) {
    try { Copy-Item -Force $slFrom $slTo; Stamp-Now $slTo; Write-Host "Copied: $slTo" } catch { Write-Host "Kept $slTo ($($_.Exception.Message))." }
}
# Copies keep the zip's "downloaded from the internet" mark. Running install.cmd was the person's yes, so take
# the mark off the installed files: otherwise Windows can stop every later launch of AIQ home (Start menu,
# update relaunch) on an "Open File - Security Warning" prompt, and a hidden launch would wait on it forever
# (CI runner, 2026-11-03, measured: with the mark kept, the auto-update step hung 35 min where it starts
# aiq-home.exe; that the prompt caused it is Inferred, and this smoke test now proves the fix or not).
Get-ChildItem -File $bin | Where-Object { $_.Name -in 'aiq.exe', 'aiq-home.exe', 'aiq-statusline.exe', 'upgrade.ps1', 'aiq.vsix' } | Unblock-File

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
