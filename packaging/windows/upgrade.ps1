# AIQ upgrade for Windows (private-alpha design A5; auto-update design docs/design/auto-update.md). No admin rights.
# AIQ-AUTO-UPDATE: 1   (the core starts only a script with this line; keep it in the first 40 lines)
#
#   powershell -ExecutionPolicy Bypass -File $env:USERPROFILE\.aiq\bin\upgrade.ps1            newest published release, now
#   powershell -ExecutionPolicy Bypass -File $env:USERPROFILE\.aiq\bin\upgrade.ps1 -Rollback  put the previous exes back
#   powershell -ExecutionPolicy Bypass -File .\upgrade.ps1 -Zip <path>                         from a zip you already have
#
# Auto-update (started by the AIQ core, or by AIQ home's "Install now" / "Go back"):
#   -Check          ask GitHub for a newer published release; download it and its SHA256SUMS into
#                   %USERPROFILE%\.aiq\update and check the SHA-256. Nothing is swapped.
#   -Apply          install the release -Check left waiting, only when AIQ is idle: AIQ home closed and no
#                   hook, judge or import running. Swaps the exes only (tool connections are left as they
#                   are). If the swap fails part-way, the exes it changed are put back.
#   -WaitPid <id>   first wait up to 20 s for that process to end (AIQ home closing itself).
#   -Relaunch       start AIQ home at the end.
#   -Feed <folder>  tests only: latest.json and the files in that folder stand in for GitHub.
#
# The manual run asks GitHub for the newest PUBLISHED release (drafts are invisible), downloads its zip and
# SHA256SUMS, refuses the zip unless its SHA-256 matches, unzips it to a temp folder and runs that release's
# install.ps1 -NoLaunch, which stops AIQ, keeps the current exes as .old and copies the new ones in. Your data
# and settings stay; the new core migrates the store on its next start. A line per run goes to
# %USERPROFILE%\.aiq\upgrade-log.txt, and the outcome to %USERPROFILE%\.aiq\update.json (AIQ home reads it).
# The one network call: api.github.com and the release download, over HTTPS. Nothing about you is sent.
[CmdletBinding()]
param(
    [switch]$Rollback,
    [string]$Zip,
    [string]$Repo = 'imransargusingh/AIQ',
    [switch]$AllowElevated,
    [switch]$Check,
    [switch]$Apply,
    [int]$WaitPid = 0,
    [switch]$Relaunch,
    [string]$Feed
)
$ErrorActionPreference = 'Stop'

$aiqHome = Join-Path $env:USERPROFILE '.aiq'
$bin = Join-Path $aiqHome 'bin'
$log = Join-Path $aiqHome 'upgrade-log.txt'
$stateFile = Join-Path $aiqHome 'update.json'
$stage = Join-Path $aiqHome 'update'
$exes = 'aiq.exe', 'aiq-home.exe'
New-Item -ItemType Directory -Force $aiqHome | Out-Null

function Log([string]$m) {
    Write-Host $m
    Add-Content -Path $log -Value ("{0:u} {1}" -f (Get-Date).ToUniversalTime(), $m)
}
function Now-Utc { (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ') }
function Stop-Aiq {
    Get-Process -Name 'aiq', 'aiq-home' -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -and $_.Path.StartsWith($bin, [StringComparison]::OrdinalIgnoreCase) } |
        Stop-Process -Force -ErrorAction SilentlyContinue
}
function Installed-Version {
    try { ((& (Join-Path $bin 'aiq.exe') version) -replace '^aiq\s+', '').Trim() } catch { 'unknown' }
}
# [version] or $null: a tag that is not plain numbers (a pre-release) is never "newer".
function Ver([string]$s) {
    $v = $null
    if ([version]::TryParse(($s -replace '^v', ''), [ref]$v)) { $v } else { $null }
}

# update.json: the outcome AIQ home and `aiq update` show. Read, change some keys, write back whole.
function Read-State {
    $h = @{}
    if (Test-Path $stateFile) {
        try { (Get-Content -Raw $stateFile | ConvertFrom-Json).PSObject.Properties | ForEach-Object { $h[$_.Name] = $_.Value } } catch { }
    }
    $h
}
function Write-State([hashtable]$changes) {
    $h = Read-State
    foreach ($k in $changes.Keys) {
        if ($null -eq $changes[$k]) { $h.Remove($k) } else { $h[$k] = $changes[$k] }
    }
    $tmp = "$stateFile.tmp"
    $h | ConvertTo-Json | Set-Content -Encoding UTF8 $tmp
    Move-Item -Force $tmp $stateFile
}

# AIQ-UPGRADE-CHECKSUM: the zip must match the checksum published with it.
function Checksum-Ok([string]$zipPath, [string]$sums) {
    if (-not (Test-Path $sums)) { return $false }
    $name = Split-Path -Leaf $zipPath
    $want = (Get-Content $sums | Where-Object { $_ -match "\s\*?$([Regex]::Escape($name))$" } | Select-Object -First 1) -replace '\s.*$', ''
    $got = (Get-FileHash $zipPath -Algorithm SHA256).Hash.ToLower()
    return [bool]$want -and $want.ToLower() -eq $got
}

function Exe-Hashes {
    $h = @{}
    foreach ($e in $exes) {
        $p = Join-Path $bin $e
        $h[$e] = if (Test-Path $p) { (Get-FileHash $p -Algorithm SHA256).Hash } else { '' }
    }
    $h
}

# Swap each named exe with its .old copy.
function Swap-Old([string[]]$names) {
    foreach ($exe in $names) {
        $cur = Join-Path $bin $exe
        $tmp = "$cur.swap"
        if (Test-Path $cur) { Move-Item -Force $cur $tmp }
        Move-Item -Force "$cur.old" $cur
        if (Test-Path $tmp) { Move-Item -Force $tmp "$cur.old" }
    }
}

# Why AIQ is not idle, or $null. Idle: AIQ home closed and no aiq.exe from bin other than the core.
function Busy-Reason {
    $procs = $null
    try {
        $procs = @(Get-CimInstance Win32_Process -Filter "Name='aiq.exe' OR Name='aiq-home.exe'" |
            Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith($bin, [StringComparison]::OrdinalIgnoreCase) })
    } catch { }
    if ($null -eq $procs) {
        # No process list with command lines (a locked-down machine): count processes instead.
        $p = @(Get-Process -Name 'aiq', 'aiq-home' -ErrorAction SilentlyContinue |
            Where-Object { $_.Path -and $_.Path.StartsWith($bin, [StringComparison]::OrdinalIgnoreCase) })
        if ($p | Where-Object { $_.Name -eq 'aiq-home' }) { return 'AIQ home is open' }
        if ($p.Count -gt 1) { return 'AIQ is busy' }
        return $null
    }
    if ($procs | Where-Object { $_.Name -eq 'aiq-home.exe' }) { return 'AIQ home is open' }
    foreach ($p in $procs) {
        $rest = ("$($p.CommandLine)" -replace '^\s*("[^"]*"|\S+)\s*', '').Trim()
        if ($rest -ne 'core') { return "AIQ is busy ($(($rest -split '\s+')[0]))" }
    }
    return $null
}

# The newest published release: tag and where its zip and SHA256SUMS are.
function Latest-Release {
    if ($Feed) {
        $j = Get-Content -Raw (Join-Path $Feed 'latest.json') | ConvertFrom-Json
        return @{ tag = $j.tag_name; zip = (Join-Path $Feed $j.zip); sums = (Join-Path $Feed 'SHA256SUMS'); zipName = $j.zip; local = $true }
    }
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    $rel = Invoke-RestMethod -UseBasicParsing -Headers @{ 'User-Agent' = 'aiq-upgrade' } "https://api.github.com/repos/$Repo/releases/latest"
    $za = $rel.assets | Where-Object { $_.name -like 'aiq-*-windows-x64.zip' } | Select-Object -First 1
    $sa = $rel.assets | Where-Object { $_.name -eq 'SHA256SUMS' } | Select-Object -First 1
    if (-not $za -or -not $sa) { throw "release $($rel.tag_name) has no Windows zip or SHA256SUMS" }
    return @{ tag = $rel.tag_name; zip = $za.browser_download_url; sums = $sa.browser_download_url; zipName = $za.name; local = $false }
}
function Fetch($from, [string]$to, [bool]$local) {
    if ($local) { Copy-Item -Force $from $to } else { Invoke-WebRequest -UseBasicParsing $from -OutFile $to }
}

# One update run at a time: an exclusive handle on update.lock, released when this process ends.
try {
    $script:lockHandle = [IO.File]::Open((Join-Path $aiqHome 'update.lock'), 'OpenOrCreate', 'ReadWrite', 'None')
} catch {
    Log 'Another AIQ update is running. Nothing changed.'
    exit 0
}

if ($Rollback) {
    if ($WaitPid) { Wait-Process -Id $WaitPid -Timeout 20 -ErrorAction SilentlyContinue }
    $old = $exes | Where-Object { Test-Path (Join-Path $bin "$_.old") }
    if (-not $old) { Log 'Nothing to roll back: no .old copies in the bin folder.'; exit 1 }
    $from = Installed-Version
    $st = Read-State
    # The release tag being left: the one an update installed, else the version's own tag (they match for
    # every release; release.yml refuses a tag that differs from the Cargo version).
    $leaving = if ($st.installed_tag) { $st.installed_tag } else { "v$from" }
    Stop-Aiq
    Swap-Old $old
    $to = Installed-Version
    # Auto-update must not put back the version the person just left; a newer release still installs.
    Write-State @{ state = 'rolled_back'; from = $from; installed = $to; skip = $leaving; installed_tag = $null; at = (Now-Utc); blocked = $null; message = $null }
    Log "Rolled back to AIQ $to. AIQ won't install $from again by itself. Run -Rollback again to return to it."
    if ($Relaunch) { Start-Process -FilePath (Join-Path $bin 'aiq-home.exe') }
    exit 0
}

if ($Check -or $Apply) {
    $code = 0
    if ($Check) {
        $st = Read-State
        $have = Installed-Version
        try {
            $rel = Latest-Release
        } catch {
            # No published release yet is a 404 here; a proxy or no network fails the same way. Retry next day.
            Write-State @{ checked_at = (Now-Utc); installed = $have; check_error = "$($_.Exception.Message)" }
            Log "AIQ-UPDATE-CHECK: could not ask for the newest release: $($_.Exception.Message)"
            exit 1
        }
        $newV = Ver $rel.tag
        $haveV = Ver $have
        if (-not $newV -or ($haveV -and $newV -le $haveV)) {
            Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
            $keep = if ($st.state -in 'applied', 'rolled_back', 'failed') { $st.state } else { 'current' }
            Write-State @{ checked_at = (Now-Utc); installed = $have; latest = $rel.tag; state = $keep; check_error = $null; available = $null }
            Log "AIQ $have is up to date (newest release $($rel.tag))."
        } elseif ($rel.tag -eq $st.skip) {
            Write-State @{ checked_at = (Now-Utc); installed = $have; latest = $rel.tag; check_error = $null }
            Log "Release $($rel.tag) is the one you rolled back from. AIQ won't install it by itself."
        } else {
            Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
            New-Item -ItemType Directory -Force $stage | Out-Null
            $zipPath = Join-Path $stage $rel.zipName
            $sums = Join-Path $stage 'SHA256SUMS'
            try {
                Fetch $rel.zip $zipPath $rel.local
                Fetch $rel.sums $sums $rel.local
            } catch {
                Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
                Write-State @{ checked_at = (Now-Utc); installed = $have; check_error = "$($_.Exception.Message)" }
                Log "AIQ-UPDATE-CHECK: download of $($rel.tag) failed: $($_.Exception.Message)"
                exit 1
            }
            if (-not (Checksum-Ok $zipPath $sums)) {
                Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
                Write-State @{ checked_at = (Now-Utc); installed = $have; state = 'failed'; available = $null; message = "the download of $($rel.tag) did not match its checksum"; at = (Now-Utc) }
                Log "AIQ-UPGRADE-CHECKSUM: $($rel.zipName) does not match SHA256SUMS. Deleted; nothing changed."
                exit 1
            }
            Write-State @{ checked_at = (Now-Utc); installed = $have; latest = $rel.tag; state = 'waiting'; available = $rel.tag; staged = $rel.zipName; check_error = $null; message = $null }
            Log "AIQ $($rel.tag) downloaded and checked. It installs when AIQ is idle."
        }
    }
    if ($Apply) {
        $st = Read-State
        $zipPath = if ($st.staged) { Join-Path $stage $st.staged } else { '' }
        if ($st.state -ne 'waiting' -or -not $zipPath -or -not (Test-Path $zipPath)) {
            if ($Relaunch) { Start-Process -FilePath (Join-Path $bin 'aiq-home.exe') }
            exit $code
        }
        if ($WaitPid) { Wait-Process -Id $WaitPid -Timeout 20 -ErrorAction SilentlyContinue }
        $busy = Busy-Reason
        if ($busy) {
            Write-State @{ blocked = $busy }
            Log "AIQ $($st.available) is waiting: $busy."
            if ($Relaunch) { Start-Process -FilePath (Join-Path $bin 'aiq-home.exe') }
            exit 3
        }
        # Checked again: the folder is the person's own, but a file changed since the check is not installed.
        if (-not (Checksum-Ok $zipPath (Join-Path $stage 'SHA256SUMS'))) {
            Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
            Write-State @{ state = 'failed'; available = $null; staged = $null; blocked = $null; at = (Now-Utc); message = "the downloaded $($st.available) changed after it was checked" }
            Log "AIQ-UPGRADE-CHECKSUM: staged $($st.staged) no longer matches SHA256SUMS. Deleted; nothing changed."
            exit 1
        }
        $work = Join-Path ([IO.Path]::GetTempPath()) ("aiq-upgrade-" + [Guid]::NewGuid().ToString('N'))
        $from = Installed-Version
        $before = Exe-Hashes
        try {
            Expand-Archive $zipPath $work
            foreach ($exe in $exes) {
                if (-not (Test-Path (Join-Path $work $exe))) { throw "$exe is missing from the zip" }
            }
            Stop-Aiq
            foreach ($exe in $exes) {
                $to = Join-Path $bin $exe
                if (Test-Path $to) { Copy-Item -Force $to "$to.old" }
                Copy-Item -Force (Join-Path $work $exe) $to
            }
            $up = Join-Path $work 'upgrade.ps1'
            if (Test-Path $up) { Copy-Item -Force $up (Join-Path $bin 'upgrade.ps1') }
            $now = Installed-Version
            Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
            Write-State @{ state = 'applied'; from = $from; installed = $now; installed_tag = $st.available; at = (Now-Utc); available = $null; staged = $null; blocked = $null; message = $null; skip = $null }
            Log "AIQ updated itself from $from to $now ($($st.available)). Your data and settings are kept. To go back: upgrade.ps1 -Rollback"
        } catch {
            # AIQ-UPGRADE-SWAP: put back only the exes this run changed, so .old copies from an earlier
            # upgrade are never swapped in by mistake.
            $after = Exe-Hashes
            $changed = $exes | Where-Object { $after[$_] -ne $before[$_] -and (Test-Path (Join-Path $bin "$_.old")) }
            if ($changed) { Stop-Aiq; Swap-Old $changed }
            Write-State @{ state = 'failed'; at = (Now-Utc); blocked = $null; message = "installing $($st.available) failed, so AIQ kept $from" }
            Log "AIQ-UPGRADE-SWAP: installing $($st.available) failed ($($_.Exception.Message)); kept AIQ $(Installed-Version). It tries again later."
            $code = 1
        } finally {
            Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
        }
        if ($Relaunch) { Start-Process -FilePath (Join-Path $bin 'aiq-home.exe') }
    }
    exit $code
}

$work = Join-Path ([IO.Path]::GetTempPath()) ("aiq-upgrade-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force $work | Out-Null
try {
    if ($Zip) {
        $zipPath = (Resolve-Path $Zip).Path
        $sums = Join-Path (Split-Path $zipPath) 'SHA256SUMS'
        $tag = '(local zip)'
    } else {
        $rel = Latest-Release
        $tag = $rel.tag
        $have = Installed-Version
        if ("v$have" -eq $tag) { Log "AIQ $have is the newest release. Nothing to do."; exit 0 }
        $zipPath = Join-Path $work $rel.zipName
        $sums = Join-Path $work 'SHA256SUMS'
        Fetch $rel.zip $zipPath $rel.local
        Fetch $rel.sums $sums $rel.local
    }

    if (-not (Checksum-Ok $zipPath $sums)) {
        Log "AIQ-UPGRADE-CHECKSUM: $(Split-Path -Leaf $zipPath) does not match SHA256SUMS (or SHA256SUMS is missing). Nothing changed."
        exit 1
    }

    $un = Join-Path $work 'unzipped'
    Expand-Archive $zipPath $un
    $installArgs = @('-ExecutionPolicy', 'Bypass', '-File', (Join-Path $un 'install.ps1'), '-NoLaunch')
    if ($AllowElevated) { $installArgs += '-AllowElevated' }
    $before = Installed-Version
    & powershell @installArgs
    if ($LASTEXITCODE -ne 0) {
        Log "AIQ-UPGRADE-SWAP: install from $tag failed (exit $LASTEXITCODE). To go back: upgrade.ps1 -Rollback"
        exit 1
    }
    $now = Installed-Version
    Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
    $itag = if ($Zip) { $null } else { $tag }
    Write-State @{ state = 'applied'; from = $before; installed = $now; installed_tag = $itag; at = (Now-Utc); available = $null; staged = $null; blocked = $null; message = $null; skip = $null }
    Log "Upgraded AIQ $before to $now ($tag). Your data and settings are kept. To go back: upgrade.ps1 -Rollback"
} finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
