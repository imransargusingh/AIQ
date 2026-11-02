# AIQ upgrade for Windows (private-alpha design A5). No admin rights.
#
#   powershell -ExecutionPolicy Bypass -File $env:USERPROFILE\.aiq\bin\upgrade.ps1            newest published release
#   powershell -ExecutionPolicy Bypass -File $env:USERPROFILE\.aiq\bin\upgrade.ps1 -Rollback  put the previous exes back
#   powershell -ExecutionPolicy Bypass -File .\upgrade.ps1 -Zip <path>                         from a zip you already have
#
# It asks GitHub for the newest PUBLISHED release (drafts are invisible), downloads its zip and SHA256SUMS, refuses
# the zip unless its SHA-256 matches, unzips it to a temp folder and runs that release's install.ps1 -NoLaunch,
# which stops AIQ, keeps the current exes as .old and copies the new ones in. Your data and settings stay; the new
# core migrates the store on its next start. A line per run goes to %USERPROFILE%\.aiq\upgrade-log.txt.
# The one network call: api.github.com and the release download, over HTTPS. Nothing about you is sent.
[CmdletBinding()]
param([switch]$Rollback, [string]$Zip, [string]$Repo = 'imransargusingh/AIQ', [switch]$AllowElevated)
$ErrorActionPreference = 'Stop'

$aiqHome = Join-Path $env:USERPROFILE '.aiq'
$bin = Join-Path $aiqHome 'bin'
$log = Join-Path $aiqHome 'upgrade-log.txt'
function Log([string]$m) {
    Write-Host $m
    Add-Content -Path $log -Value ("{0:u} {1}" -f (Get-Date).ToUniversalTime(), $m)
}
function Stop-Aiq {
    Get-Process -Name 'aiq', 'aiq-home' -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -and $_.Path.StartsWith($bin, [StringComparison]::OrdinalIgnoreCase) } |
        Stop-Process -Force -ErrorAction SilentlyContinue
}
function Installed-Version {
    try { ((& (Join-Path $bin 'aiq.exe') version) -replace '^aiq\s+', '').Trim() } catch { 'unknown' }
}

if ($Rollback) {
    $old = 'aiq.exe', 'aiq-home.exe' | Where-Object { Test-Path (Join-Path $bin "$_.old") }
    if (-not $old) { Log 'Nothing to roll back: no .old copies in the bin folder.'; exit 1 }
    Stop-Aiq
    foreach ($exe in $old) {
        $cur = Join-Path $bin $exe
        $tmp = "$cur.swap"
        if (Test-Path $cur) { Move-Item -Force $cur $tmp }
        Move-Item -Force "$cur.old" $cur
        if (Test-Path $tmp) { Move-Item -Force $tmp "$cur.old" }
    }
    Log "Rolled back to AIQ $(Installed-Version). Run -Rollback again to return to the newer one."
    exit 0
}

$work = Join-Path ([IO.Path]::GetTempPath()) ("aiq-upgrade-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force $work | Out-Null
try {
    if ($Zip) {
        $zipPath = (Resolve-Path $Zip).Path
        $sums = Join-Path (Split-Path $zipPath) 'SHA256SUMS'
        $tag = '(local zip)'
    } else {
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        $rel = Invoke-RestMethod -UseBasicParsing -Headers @{ 'User-Agent' = 'aiq-upgrade' } "https://api.github.com/repos/$Repo/releases/latest"
        $tag = $rel.tag_name
        $have = Installed-Version
        if ("v$have" -eq $tag) { Log "AIQ $have is the newest release. Nothing to do."; exit 0 }
        $za = $rel.assets | Where-Object { $_.name -like 'aiq-*-windows-x64.zip' } | Select-Object -First 1
        $sa = $rel.assets | Where-Object { $_.name -eq 'SHA256SUMS' } | Select-Object -First 1
        if (-not $za -or -not $sa) { Log "Release $tag has no Windows zip or SHA256SUMS. Nothing changed."; exit 1 }
        $zipPath = Join-Path $work $za.name
        $sums = Join-Path $work 'SHA256SUMS'
        Invoke-WebRequest -UseBasicParsing $za.browser_download_url -OutFile $zipPath
        Invoke-WebRequest -UseBasicParsing $sa.browser_download_url -OutFile $sums
    }

    # AIQ-UPGRADE-CHECKSUM: the zip must match the checksum published with it, or nothing changes.
    if (-not (Test-Path $sums)) { Log "AIQ-UPGRADE-CHECKSUM: no SHA256SUMS next to the zip. Nothing changed."; exit 1 }
    $name = Split-Path -Leaf $zipPath
    $want = (Get-Content $sums | Where-Object { $_ -match "\s\*?$([Regex]::Escape($name))$" } | Select-Object -First 1) -replace '\s.*$', ''
    $got = (Get-FileHash $zipPath -Algorithm SHA256).Hash.ToLower()
    if (-not $want -or $want.ToLower() -ne $got) {
        Log "AIQ-UPGRADE-CHECKSUM: $name does not match SHA256SUMS. Nothing changed."
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
    Log "Upgraded AIQ $before to $(Installed-Version) ($tag). Your data and settings are kept. To go back: upgrade.ps1 -Rollback"
} finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
