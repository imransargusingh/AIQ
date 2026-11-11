# AIQ upgrade for Windows (private-alpha design A5; auto-update design docs/design/auto-update.md). No admin rights.
# AIQ-AUTO-UPDATE: 1   (the core starts only a script with this line; keep it in the first 40 lines)
#
#   powershell -ExecutionPolicy Bypass -File $env:USERPROFILE\.aiq\bin\upgrade.ps1            newest published release, now
#   powershell -ExecutionPolicy Bypass -File $env:USERPROFILE\.aiq\bin\upgrade.ps1 -Rollback  put the previous exes back
#   powershell -ExecutionPolicy Bypass -File .\upgrade.ps1 -Zip <path>                         from a zip you already have
#
# Auto-update (started by the AIQ core, or by AIQ home's "Install now" / "Go back"):
#   -Check          ask GitHub for a newer published release; download it and its SHA256SUMS into
#                   %USERPROFILE%\.aiq\update and check the SHA-256. Nothing is swapped. A release already
#                   waiting is kept until a newer one is downloaded and checked, then replaced by it; the same
#                   release is not downloaded again. With -Apply, a failed check still installs what waits.
#   -Apply          install the release -Check left waiting, only when AIQ is idle: AIQ home closed and no
#                   hook, judge or import running. Swaps the exes only (tool connections are left as they
#                   are). If the swap fails part-way, the exes it changed are put back.
#   -WaitPid <id>   first wait up to 20 s for that process to end (AIQ home closing itself).
#   -Relaunch       start AIQ home at the end (tried again for up to 15 s if Windows refuses it; never an error).
#                   With -Apply or -Rollback, a run that finds another update running waits for it (up to
#                   -LockWaitSec, 300 s) instead of giving up: AIQ home closed itself for this run.
#   -LockWaitSec <s>   tests: how long that wait lasts.
#   -FailLaunches <n>  tests only: the first n starts of the core or AIQ home fail as if Windows refused them.
#   -FailExplorer      tests only: with -FailLaunches, only the starts through Explorer fail.
#   -Feed <folder>  tests only: latest.json and the files in that folder stand in for GitHub.
#   -Site <url> -Api <url>   tests only: stand-ins for https://github.com and https://api.github.com.
#
# The manual run asks GitHub for the newest PUBLISHED release (drafts are invisible), downloads its zip and
# SHA256SUMS, refuses the zip unless its SHA-256 matches, unzips it to a temp folder and runs that release's
# install.ps1 -NoLaunch, which stops AIQ, keeps the current exes as .old and copies the new ones in. Your data
# and settings stay; the new core migrates the store on its next start. A line per run goes to
# %USERPROFILE%\.aiq\upgrade-log.txt, and the outcome to %USERPROFILE%\.aiq\update.json (AIQ home reads it).
# The network calls, over HTTPS, nothing about you sent: github.com/<repo>/releases/latest (its redirect names the
# newest tag) and that release's download URLs. Neither counts against GitHub's API limit of 60 calls an hour per
# address, which a shared office address used up on 2026-11-04 (403). api.github.com is asked only when the
# redirect gives no tag; when it refuses for its limit, update.json gets retry_at (when the limit resets).
# update.json's `source` names which one answered the last good check (redirect or api): AIQ home checks every
# 15 minutes only after the redirect answered, hourly after the API did, so it asks the API at most once an hour.
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
    [int]$FailLaunches = 0,
    [switch]$FailExplorer,
    [string]$Feed,
    [string]$Site = 'https://github.com',
    [string]$Api = 'https://api.github.com',
    [int]$LockWaitSec = 300
)
$ErrorActionPreference = 'Stop'

$aiqHome = Join-Path $env:USERPROFILE '.aiq'
$bin = Join-Path $aiqHome 'bin'
$log = Join-Path $aiqHome 'upgrade-log.txt'
$stateFile = Join-Path $aiqHome 'update.json'
$stage = Join-Path $aiqHome 'update'
$exes = 'aiq.exe', 'aiq-home.exe'
New-Item -ItemType Directory -Force $aiqHome | Out-Null

# Never throws: a log line is a record of the work, so a failed write must not end the update. Something reading the
# log at that moment (`aiq status`, a scanner, a test) can make the append fail ("Stream was not readable", release
# build of 0.1.15 on main 9cfa689, 2026-11-07, right after a rollback), so it is tried 5 times, 100 ms apart.
# AIQ-UPDATE-LOG-HELD: a line that still can't be written goes to upgrade-log-held.txt with the reason, and the next
# line that can be written brings those lines back into upgrade-log.txt first. Before, it was dropped without a
# trace: laptop, 2026-11-11, the 0.1.38 check and install (04:52:50Z, 04:53:53Z) and the 05:09:01Z check wrote
# update.json but not one log line.
$heldLog = Join-Path $aiqHome 'upgrade-log-held.txt'
function Log([string]$m) {
    Write-Host $m
    $line = "{0:u} {1}" -f (Get-Date).ToUniversalTime(), $m
    $why = ''
    foreach ($i in 1..5) {
        try {
            $held = @()
            if (Test-Path $heldLog) { $held = @(Get-Content $heldLog -ErrorAction Stop) }
            Add-Content -Path $log -Value ($held + $line) -ErrorAction Stop
            if ($held.Count) { Remove-Item -Force $heldLog -ErrorAction SilentlyContinue }
            return
        } catch { $why = $_.Exception.Message; Start-Sleep -Milliseconds 100 }
    }
    try { Add-Content -Path $heldLog -Value "$line (upgrade-log.txt could not be written: $why)" -ErrorAction Stop } catch { }
}
# The zip holds zone-less (DOS) file times in the build runner's zone, UTC, and Expand-Archive reads them as local
# time, so a copy kept them hours off: laptop (UTC-8), 2026-11-11, 0.1.38's files said 11:43-11:49Z, 8 h ahead (the
# zip says 03:43-03:49). A file copied into bin carries the time it was installed instead. Never throws.
function Stamp-Now([string]$path) {
    try { (Get-Item -LiteralPath $path).LastWriteTimeUtc = [DateTime]::UtcNow } catch { }
}
function Now-Utc { (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ') }
function Stop-Aiq {
    $p = @(Get-Process -Name 'aiq', 'aiq-home' -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -and $_.Path.StartsWith($bin, [StringComparison]::OrdinalIgnoreCase) })
    # Any aiq.exe counts as the core: -Apply runs only when no other aiq.exe is (Busy-Reason), and a hook that
    # happens to be stopped during -Rollback only means the core is started once more than needed (it exits).
    if ($p | Where-Object { $_.Name -eq 'aiq' }) { $script:coreWasRunning = $true }
    $p | Stop-Process -Force -ErrorAction SilentlyContinue
    # Wait for them to end, so their exes are free to replace.
    $p | Wait-Process -Timeout 10 -ErrorAction SilentlyContinue
}
# Start the core again after Stop-Aiq stopped one. Without it AIQ stayed down until the next hook started it (laptop,
# 2026-11-04: Home's Install now swapped at 04:17:03Z, core back at 04:22:53Z; Home said "AIQ isn't running").
# A second core exits at once (CORE_ALREADY_RUNNING), so a start is harmless when one is already up; still, a core
# that came back by itself (a hook) is not started again.
$script:coreWasRunning = $false
# Is an `aiq core` from bin running? $false when the process list can't be read (then a start is tried; it is harmless).
function Core-Running {
    try {
        return [bool](@(Get-CimInstance Win32_Process -Filter "Name='aiq.exe'" | Where-Object {
            $_.ExecutablePath -and $_.ExecutablePath.StartsWith($bin, [StringComparison]::OrdinalIgnoreCase) -and
            ("$($_.CommandLine)" -replace '^\s*("[^"]*"|\S+)\s*', '').Trim() -eq 'core' }).Count)
    } catch { return $false }
}
function Home-Running {
    [bool](@(Get-Process -Name 'aiq-home' -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -and $_.Path.StartsWith($bin, [StringComparison]::OrdinalIgnoreCase) }).Count)
}
# Start an exe just swapped in, and never throw. Microsoft Defender's attack surface reduction rule "Use advanced
# protection against ransomware" (set by IT; event 1121) refuses a freshly installed AIQ exe started by a script host.
# Measured on the laptop, 11-05 to 11-08: every AIQ block was a start by powershell.exe or cmd.exe within about an hour
# of an update (upgrade.ps1 3, the Copilot status line 2, Defender's own scan script 6); none by Explorer, by AIQ home
# starting the core, or by the hooks. On 11-08 04:09:55Z AIQ home, refused directly, opened through Explorer.
# So each start goes through Explorer first (the parent the Start menu gives it), and starts the exe from this script
# only after two rounds of that failed. A refused start also ended the whole script once (11-05 23:22Z): never throw.
# Rounds after 0, 1, 2, 4 and 8 s; stop as soon as $running says it is up (a hook, the person). Which way worked is
# logged. $true when up.
$script:failLaunches = $FailLaunches
function Refused-For-Test([string]$kind) {
    if ($FailExplorer -and $kind -ne 'explorer') { return }
    if ($script:failLaunches -gt 0) { $script:failLaunches--; throw 'Access is denied (test).' }
}
# One way to start something: Explorer opening $target (an exe or a shortcut), or this script starting it.
function Way-Explorer([string]$target) {
    @{ kind = 'explorer'; via = ' through Explorer'; run = { Start-Process -FilePath (Join-Path $env:SystemRoot 'explorer.exe') -ArgumentList "`"$target`"" }.GetNewClosure() }
}
function Way-Direct([string]$exe, [string[]]$argv, [string]$via) {
    @{ kind = 'direct'; via = $via; run = {
        $a = @{ FilePath = $exe; WindowStyle = 'Hidden' }
        if ($argv) { $a.ArgumentList = $argv }
        Start-Process @a
    }.GetNewClosure() }
}
function Start-Again([object[]]$ways, [scriptblock]$running, [string]$code, [string]$what) {
    $last = ''
    $round = 0
    foreach ($wait in 0, 1, 2, 4, 8) {
        if ($wait) { Start-Sleep -Seconds $wait }
        $round++
        foreach ($w in $ways) {
            if ($w.kind -eq 'direct' -and $round -le 2) { continue }
            if (& $running) { Log "${code}: $what was already running again."; return $true }
            try {
                Refused-For-Test $w.kind
                & $w.run
                # Neither Explorer nor a detached start reports back, so the process list says whether it is up.
                foreach ($i in 1..15) {
                    if (& $running) {
                        $tries = if ($last) { " (refused first: $last)" } else { '' }
                        Log "${code}: started $what again$($w.via).$tries"
                        return $true
                    }
                    Start-Sleep -Milliseconds 200
                }
                if (-not $last) { $last = "not running 3 s after starting it$($w.via)" }
            } catch { $last = $_.Exception.Message }
        }
    }
    Log "${code}: could not start $what ($last)."
    return $false
}
# A shortcut to AIQ home's windowless start-only mode, so Explorer can start the core (Explorer passes no arguments).
function Core-Shortcut {
    $lnk = Join-Path $bin 'aiq-start-core.lnk'
    $s = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk)
    $s.TargetPath = Join-Path $bin 'aiq-home.exe'
    $s.Arguments = '--start-core'
    $s.WorkingDirectory = $bin
    $s.WindowStyle = 7
    $s.Save()
    $lnk
}
function Start-Core {
    $exe = Join-Path $bin 'aiq.exe'
    if (-not $script:coreWasRunning -or -not (Test-Path $exe)) { return }
    $ways = @()
    try { $ways += Way-Explorer (Core-Shortcut) } catch { Log "AIQ-UPDATE-CORE: no shortcut for Explorer ($($_.Exception.Message))." }
    # AIQ home starts the core itself: measured to work on the laptop (11-07 18:21:05Z) when this script's start was refused.
    $ways += Way-Direct (Join-Path $bin 'aiq-home.exe') @('--start-core') ' through AIQ home'
    $ways += Way-Direct $exe @('core') ''
    if (-not (Start-Again $ways { Core-Running } 'AIQ-UPDATE-CORE' 'the AIQ core')) {
        Log 'AIQ-UPDATE-CORE: the next hook, AIQ home or the next sign-in starts it.'
    }
    # Its start-only run must have ended before Start-Home asks whether AIQ home is running.
    Get-Process -Name 'aiq-home' -ErrorAction SilentlyContinue | Where-Object { $_.Path -and $_.Path.StartsWith($bin, [StringComparison]::OrdinalIgnoreCase) } |
        Wait-Process -Timeout 5 -ErrorAction SilentlyContinue
}
# -Relaunch: open AIQ home again (it closed itself for the update).
function Start-Home {
    if (-not $Relaunch) { return }
    $exe = Join-Path $bin 'aiq-home.exe'
    if (-not (Test-Path $exe)) { return }
    # The closing AIQ home must not count as "already running" (some paths return before the swap waited for it).
    if ($WaitPid) { Wait-Process -Id $WaitPid -Timeout 20 -ErrorAction SilentlyContinue }
    $ways = @((Way-Explorer $exe), (Way-Direct $exe @() ''))
    $ways[1].run = { Start-Process -FilePath $exe }.GetNewClosure()  # a window, not hidden
    if (-not (Start-Again $ways { Home-Running } 'AIQ-UPDATE-HOME' 'AIQ home')) {
        Log 'AIQ-UPDATE-HOME: open AIQ from the Start menu; AIQ home starts the core too.'
    }
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
# First the web redirect: github.com/<repo>/releases/latest answers 302 to .../releases/tag/<tag>. The release
# workflow names the assets aiq-<version>-windows-x64.zip and SHA256SUMS, so their URLs follow from the tag.
function Tag-From-Redirect {
    $req = [Net.HttpWebRequest]::Create("$Site/$Repo/releases/latest")
    $req.Method = 'HEAD'
    $req.AllowAutoRedirect = $false
    $req.UserAgent = 'aiq-upgrade'
    $req.Timeout = 30000
    # A 3xx with redirects off may return or throw, depending on the .NET version; either way read its headers.
    try { $resp = $req.GetResponse() } catch {
        $e = $_.Exception
        while ($e -and -not ($e -is [Net.WebException])) { $e = $e.InnerException }
        if (-not $e -or -not $e.Response) { throw }
        $resp = $e.Response
    }
    try { $code = [int]$resp.StatusCode; $loc = "$($resp.Headers['Location'])" } finally { $resp.Close() }
    if ($loc -match '/releases/tag/([^/?#]+)$') { return [Uri]::UnescapeDataString($Matches[1]) }
    throw "releases/latest answered $code without a release tag"
}
# When api.github.com refused for its rate limit, the time it resets ('yyyy-MM-ddTHH:mm:ssZ'), else $null.
# X-RateLimit-Reset is Unix seconds; Retry-After (a 429 or a secondary limit) is seconds from now.
function Retry-At($resp) {
    if (-not $resp) { return $null }
    function Resp-Header([string]$n) {
        $v = $null
        try { $v = $resp.Headers[$n] } catch { }
        if (-not $v) { try { $vals = $null; if ($resp.Headers.TryGetValues($n, [ref]$vals)) { $v = @($vals)[0] } } catch { } }
        "$v"
    }
    $code = 0
    try { $code = [int]$resp.StatusCode } catch { }
    if ($code -ne 403 -and $code -ne 429) { return $null }
    $sec = 0L
    if ([long]::TryParse((Resp-Header 'Retry-After'), [ref]$sec)) {
        return (Get-Date).ToUniversalTime().AddSeconds($sec).ToString('yyyy-MM-ddTHH:mm:ssZ')
    }
    if ((Resp-Header 'X-RateLimit-Remaining') -eq '0' -and [long]::TryParse((Resp-Header 'X-RateLimit-Reset'), [ref]$sec)) {
        return [DateTimeOffset]::FromUnixTimeSeconds($sec).UtcDateTime.ToString('yyyy-MM-ddTHH:mm:ssZ')
    }
    return $null
}
function Latest-Release {
    if ($Feed) {
        $j = Get-Content -Raw (Join-Path $Feed 'latest.json') | ConvertFrom-Json
        return @{ tag = $j.tag_name; zip = (Join-Path $Feed $j.zip); sums = (Join-Path $Feed 'SHA256SUMS'); zipName = $j.zip; local = $true }
    }
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    $why = ''
    try {
        $tag = Tag-From-Redirect
        $zipName = "aiq-$($tag -replace '^v', '')-windows-x64.zip"
        $base = "$Site/$Repo/releases/download/$([Uri]::EscapeDataString($tag))"
        return @{ tag = $tag; zip = "$base/$zipName"; sums = "$base/SHA256SUMS"; zipName = $zipName; local = $false; via = 'redirect' }
    } catch {
        $why = "$($_.Exception.Message)"
        Log "AIQ-UPDATE-REDIRECT: no tag from github.com ($why); asking api.github.com."
    }
    try {
        $rel = Invoke-RestMethod -UseBasicParsing -Headers @{ 'User-Agent' = 'aiq-upgrade' } "$Api/repos/$Repo/releases/latest"
    } catch {
        $e = $_.Exception
        while ($e -and -not $e.PSObject.Properties['Response']) { $e = $e.InnerException }
        $script:retryAt = if ($e) { Retry-At $e.Response } else { $null }
        throw
    }
    $za = $rel.assets | Where-Object { $_.name -like 'aiq-*-windows-x64.zip' } | Select-Object -First 1
    $sa = $rel.assets | Where-Object { $_.name -eq 'SHA256SUMS' } | Select-Object -First 1
    if (-not $za -or -not $sa) { throw "release $($rel.tag_name) has no Windows zip or SHA256SUMS" }
    return @{ tag = $rel.tag_name; zip = $za.browser_download_url; sums = $sa.browser_download_url; zipName = $za.name; local = $false; via = 'api' }
}
function Fetch($from, [string]$to, [bool]$local) {
    if ($local) { Copy-Item -Force $from $to } else { Invoke-WebRequest -UseBasicParsing $from -OutFile $to }
}

# One update run at a time: an exclusive handle on update.lock, released when this process ends.
function Take-Lock {
    try { $script:lockHandle = [IO.File]::Open((Join-Path $aiqHome 'update.lock'), 'OpenOrCreate', 'ReadWrite', 'None'); $true }
    catch { $false }
}
if (-not (Take-Lock)) {
    # AIQ-UPDATE-LOCK: an install or a rollback that AIQ home closed itself for (-Relaunch: Install now, Go back, an
    # away install) waits for the other run (a check downloading, the core's run) instead of giving up. Laptop,
    # 2026-11-09 02:52:28Z: Home's check took the lock 18 ms before its away install, which logged "Nothing
    # changed", and 0.1.29 stayed waiting for the next try 30 minutes later. A check, or the core's run, gives up.
    $waited = $false
    if ($Relaunch -and ($Apply -or $Rollback) -and -not $Check) {
        Log "AIQ-UPDATE-LOCK: an update run already holds the lock; waiting up to $LockWaitSec s for it to finish."
        $until = (Get-Date).AddSeconds($LockWaitSec)
        while (-not $waited -and (Get-Date) -lt $until) {
            Start-Sleep -Milliseconds 500
            $waited = Take-Lock
        }
        if ($waited) { Log 'AIQ-UPDATE-LOCK: the other update run finished; going on.' }
    }
    if (-not $waited) {
        # AIQ home may have closed itself for this run: open it again, or it stays closed until the person opens it
        # from the Start menu.
        Log 'Another AIQ update is running. Nothing changed.'
        Start-Home
        exit 0
    }
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
    Start-Core
    Start-Home
    exit 0
}

if ($Check -or $Apply) {
    $code = 0
    if ($Check) {
        $st = Read-State
        $have = Installed-Version
        $script:retryAt = $null
        # A release already waiting stays waiting until a newer one is downloaded and checked: a failed check or
        # download never drops it, and with -Apply the run still goes on to install it (2026-11-05, laptop: 0.1.8
        # waited from 18:58Z while AIQ home was open, and no check ran after it, so 0.1.9 at 20:32Z was never seen).
        $waiting = $st.state -eq 'waiting' -and $st.staged -and (Test-Path (Join-Path $stage $st.staged))
        $rel = $null
        try {
            $rel = Latest-Release
        } catch {
            # No published release, a proxy or no network: the core retries within the hour. GitHub's API limit
            # (403 with X-RateLimit-Remaining 0): retry_at says when it resets, and the core retries then.
            Write-State @{ checked_at = (Now-Utc); installed = $have; check_error = "$($_.Exception.Message)"; retry_at = $script:retryAt }
            $after = if ($script:retryAt) { " Rate limited; retry after $($script:retryAt)." } else { '' }
            Log "AIQ-UPDATE-CHECK: could not ask for the newest release: $($_.Exception.Message)$after"
            if (-not ($Apply -and $waiting)) { exit 1 }
        }
        if ($rel) {
            if ($rel.via) { Log "AIQ-UPDATE-SOURCE: the $($rel.via) named $($rel.tag)." }
            $newV = Ver $rel.tag
            $haveV = Ver $have
            if (-not $newV -or ($haveV -and $newV -le $haveV)) {
                Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
                $keep = if ($st.state -in 'applied', 'rolled_back', 'failed') { $st.state } else { 'current' }
                Write-State @{ checked_at = (Now-Utc); installed = $have; latest = $rel.tag; source = $rel.via; state = $keep; check_error = $null; retry_at = $null; available = $null; staged = $null; blocked = $null }
                Log "AIQ $have is up to date (newest release $($rel.tag))."
            } elseif ($rel.tag -eq $st.skip) {
                Write-State @{ checked_at = (Now-Utc); installed = $have; latest = $rel.tag; source = $rel.via; check_error = $null; retry_at = $null }
                Log "Release $($rel.tag) is the one you rolled back from. AIQ won't install it by itself."
            } elseif ($waiting -and $rel.tag -eq $st.available -and (Checksum-Ok (Join-Path $stage $st.staged) (Join-Path $stage 'SHA256SUMS'))) {
                # AIQ-UPDATE-STILL-NEWEST: the waiting release is still the newest. Not downloaded again, so AIQ home
                # can check every 15 minutes while one waits at the cost of one HEAD, not a 5 MB zip.
                Write-State @{ checked_at = (Now-Utc); installed = $have; latest = $rel.tag; source = $rel.via; check_error = $null; retry_at = $null }
                Log "AIQ-UPDATE-STILL-NEWEST: $($rel.tag) is already downloaded and waiting."
            } else {
                # Downloaded beside the waiting one and swapped in only once its checksum matches, so a failed
                # download leaves the waiting release as it was.
                $next = "$stage.new"
                Remove-Item -Recurse -Force $next -ErrorAction SilentlyContinue
                New-Item -ItemType Directory -Force $next | Out-Null
                $zipPath = Join-Path $next $rel.zipName
                $sums = Join-Path $next 'SHA256SUMS'
                $got = $false
                try {
                    Fetch $rel.zip $zipPath $rel.local
                    Fetch $rel.sums $sums $rel.local
                    $got = $true
                } catch {
                    Write-State @{ checked_at = (Now-Utc); installed = $have; check_error = "$($_.Exception.Message)"; retry_at = $null }
                    Log "AIQ-UPDATE-CHECK: download of $($rel.tag) failed: $($_.Exception.Message)"
                }
                if ($got -and -not (Checksum-Ok $zipPath $sums)) {
                    $got = $false
                    if ($waiting) {
                        Write-State @{ checked_at = (Now-Utc); installed = $have; check_error = "the download of $($rel.tag) did not match its checksum"; retry_at = $null }
                    } else {
                        Write-State @{ checked_at = (Now-Utc); installed = $have; state = 'failed'; available = $null; message = "the download of $($rel.tag) did not match its checksum"; at = (Now-Utc); retry_at = $null }
                    }
                    Log "AIQ-UPGRADE-CHECKSUM: $($rel.zipName) does not match SHA256SUMS. Deleted; nothing changed."
                }
                if ($got) {
                    Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
                    Move-Item $next $stage
                    $was = if ($waiting -and $st.available) { " It replaces $($st.available), which was waiting." } else { '' }
                    Write-State @{ checked_at = (Now-Utc); installed = $have; latest = $rel.tag; source = $rel.via; state = 'waiting'; available = $rel.tag; staged = $rel.zipName; check_error = $null; retry_at = $null; message = $null }
                    Log "AIQ $($rel.tag) downloaded and checked. It installs when AIQ is idle.$was"
                } else {
                    Remove-Item -Recurse -Force $next -ErrorAction SilentlyContinue
                    if (-not ($Apply -and $waiting)) { exit 1 }
                }
            }
        }
    }
    if ($Apply) {
        $st = Read-State
        $zipPath = if ($st.staged) { Join-Path $stage $st.staged } else { '' }
        if ($st.state -ne 'waiting' -or -not $zipPath -or -not (Test-Path $zipPath)) {
            Start-Home
            exit $code
        }
        # AIQ-UPDATE-NOT-NEWER: a staged release that is not newer than what is installed (the person installed it
        # another way after the check) is dropped, never swapped in: a same-for-same swap stops AIQ for nothing
        # and overwrites the .old copies, so "Go back" is lost (laptop, 2026-11-04 04:17:03Z).
        $have = Installed-Version
        $stagedV = Ver "$($st.available)"
        $haveV = Ver $have
        if ($stagedV -and $haveV -and $stagedV -le $haveV) {
            Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
            Write-State @{ state = 'current'; installed = $have; available = $null; staged = $null; blocked = $null }
            Log "AIQ-UPDATE-NOT-NEWER: the waiting $($st.available) is not newer than AIQ $have. Dropped it; nothing changed."
            Start-Home
            exit 0
        }
        if ($WaitPid) { Wait-Process -Id $WaitPid -Timeout 20 -ErrorAction SilentlyContinue }
        $busy = Busy-Reason
        if ($busy) {
            Write-State @{ blocked = $busy }
            Log "AIQ $($st.available) is waiting: $busy."
            Start-Home
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
                Stamp-Now $to
            }
            # upgrade.ps1, and aiq.vsix (the VS Code extension): `aiq install --new` at the core's next start adds
            # it to VS Code if VS Code is a tool AIQ hasn't seen before; an extension already added stays as it is.
            foreach ($f in 'upgrade.ps1', 'aiq.vsix') {
                $extra = Join-Path $work $f  # not $from: that holds the version being left
                if (Test-Path $extra) { Copy-Item -Force $extra (Join-Path $bin $f); Stamp-Now (Join-Path $bin $f) }
            }
            # aiq-statusline.exe stays the same file across releases (U5n), so it is copied only when it differs, and a
            # copy that fails (Copilot running it this instant) keeps the one in place: never a reason to undo the update.
            $slFrom = Join-Path $work 'aiq-statusline.exe'
            $slTo = Join-Path $bin 'aiq-statusline.exe'
            if ((Test-Path $slFrom) -and (-not (Test-Path $slTo) -or (Get-FileHash $slFrom).Hash -ne (Get-FileHash $slTo).Hash)) {
                try { Copy-Item -Force $slFrom $slTo; Stamp-Now $slTo; Log 'AIQ-UPDATE-STATUSLINE: copied a new aiq-statusline.exe.' }
                catch { Log "AIQ-UPDATE-STATUSLINE: kept the aiq-statusline.exe in place ($($_.Exception.Message))." }
            }
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
        # Stop-Aiq stopped the core on both paths (updated, or put back after a failed swap).
        Start-Core
        Start-Home
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
