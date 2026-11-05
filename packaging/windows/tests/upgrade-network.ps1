# Test of upgrade.ps1 -Check's network path against a local stand-in for github.com and api.github.com.
# Run on Windows: powershell -ExecutionPolicy Bypass -File packaging\windows\tests\upgrade-network.ps1 <path to upgrade.ps1>
# It runs upgrade.ps1 with USERPROFILE pointed at a temp folder, so an installed AIQ is never touched.
#   1. The releases/latest redirect names the tag: the zip and SHA256SUMS come from releases/download; no API call.
#   2. No tag from the redirect, the API answers: the API's asset URLs are used.
#   3. No tag from the redirect, the API refuses for its rate limit: check_error plus retry_at = the reset time.
param([Parameter(Mandatory = $true)][string]$Upgrade)
$ErrorActionPreference = 'Stop'

$root = Join-Path ([IO.Path]::GetTempPath()) ("aiq-upnet-" + [Guid]::NewGuid().ToString('N'))
$files = Join-Path $root 'files'
New-Item -ItemType Directory -Force $files | Out-Null
$zipName = 'aiq-99.0.0-windows-x64.zip'
Set-Content -Path (Join-Path $root 'aiq.exe') -Value 'stand-in'
Compress-Archive -Path (Join-Path $root 'aiq.exe') -DestinationPath (Join-Path $files $zipName)
$h = (Get-FileHash (Join-Path $files $zipName) -Algorithm SHA256).Hash.ToLower()
"$h  $zipName" | Set-Content -Encoding ascii (Join-Path $files 'SHA256SUMS')

$l = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
$l.Start(); $port = $l.LocalEndpoint.Port; $l.Stop()
$base = "http://localhost:$port"
$hits = Join-Path $root 'hits.txt'
$reset = 1793760000   # 2026-11-04T02:40:00Z

$server = Start-Job -ArgumentList $base, $files, $hits, $reset -ScriptBlock {
    param($base, $files, $hits, $reset)
    $http = [Net.HttpListener]::new()
    $http.Prefixes.Add("$base/")
    $http.Start()
    while ($true) {
        $c = $http.GetContext()
        $p = $c.Request.Url.AbsolutePath
        Add-Content -Path $hits -Value "$($c.Request.HttpMethod) $p"
        $r = $c.Response
        $body = $null
        switch -Regex ($p) {
            '^/stop$' { $r.StatusCode = 200; $r.Close(); $http.Stop(); return }
            '^/o/redirect/releases/latest$' { $r.StatusCode = 302; $r.RedirectLocation = "$base/o/redirect/releases/tag/v99.0.0" }
            '^/o/(api|limited)/releases/latest$' { $r.StatusCode = 302; $r.RedirectLocation = "$base/o/$($Matches[1])/releases" }
            '^/o/\w+/releases/download/v99\.0\.0/(.+)$' { $body = [IO.File]::ReadAllBytes((Join-Path $files $Matches[1])) }
            '^/api/repos/o/api/releases/latest$' {
                $body = [Text.Encoding]::UTF8.GetBytes((@{ tag_name = 'v99.0.0'; assets = @(
                    @{ name = 'aiq-99.0.0-windows-x64.zip'; browser_download_url = "$base/o/api/releases/download/v99.0.0/aiq-99.0.0-windows-x64.zip" },
                    @{ name = 'SHA256SUMS'; browser_download_url = "$base/o/api/releases/download/v99.0.0/SHA256SUMS" }) } | ConvertTo-Json -Depth 4))
            }
            '^/api/repos/o/limited/releases/latest$' {
                $r.StatusCode = 403
                $r.Headers.Add('X-RateLimit-Limit', '60'); $r.Headers.Add('X-RateLimit-Remaining', '0'); $r.Headers.Add('X-RateLimit-Reset', "$reset")
            }
            default { $r.StatusCode = 404 }
        }
        if ($body) { $r.OutputStream.Write($body, 0, $body.Length) }
        $r.Close()
    }
}

$failed = $false
function Fail([string]$m) { Write-Host "FAIL: $m"; $script:failed = $true }
function Run([string]$repo) {
    $profileDir = Join-Path $root $repo
    New-Item -ItemType Directory -Force $profileDir | Out-Null
    Remove-Item -Force $hits -ErrorAction SilentlyContinue
    $saved = $env:USERPROFILE
    $env:USERPROFILE = $profileDir
    try { powershell -NoProfile -ExecutionPolicy Bypass -File $Upgrade -Check -Repo "o/$repo" -Site $base -Api "$base/api" | Out-Host }
    finally { $env:USERPROFILE = $saved }
    $aiq = Join-Path $profileDir '.aiq'
    @{
        code = $LASTEXITCODE
        state = (Get-Content -Raw (Join-Path $aiq 'update.json') | ConvertFrom-Json)
        hits = @(if (Test-Path $hits) { Get-Content $hits })
        log = (Get-Content -Raw (Join-Path $aiq 'upgrade-log.txt'))
        staged = (Test-Path (Join-Path $aiq "update\$zipName"))
    }
}

try {
    # Wait for the listener.
    $up = $false
    foreach ($i in 1..50) {
        try { Invoke-WebRequest -UseBasicParsing "$base/ping" | Out-Null } catch { if ($_.Exception.Response) { $up = $true; break } }
        Start-Sleep -Milliseconds 200
    }
    if (-not $up) { throw "stand-in server did not start: $(Receive-Job $server 2>&1)" }

    $a = Run 'redirect'
    if ($a.code -ne 0) { Fail "redirect: exit $($a.code)" }
    if ($a.state.state -ne 'waiting' -or $a.state.available -ne 'v99.0.0' -or -not $a.staged) { Fail "redirect: $($a.state | ConvertTo-Json -Compress)" }
    if ($a.hits -match '/api/') { Fail "redirect: asked the API anyway: $($a.hits -join '; ')" }
    if (-not ($a.hits -contains 'HEAD /o/redirect/releases/latest')) { Fail "redirect: no HEAD of releases/latest: $($a.hits -join '; ')" }
    if ($a.log -notmatch 'AIQ-UPDATE-SOURCE: the redirect named v99\.0\.0') { Fail "redirect: log does not name the source" }
    if ($a.state.source -ne 'redirect') { Fail "redirect: update.json source is '$($a.state.source)', want redirect" }

    $b = Run 'api'
    if ($b.code -ne 0) { Fail "api: exit $($b.code)" }
    if ($b.state.state -ne 'waiting' -or $b.state.available -ne 'v99.0.0' -or -not $b.staged) { Fail "api: $($b.state | ConvertTo-Json -Compress)" }
    if (-not ($b.hits -contains 'GET /api/repos/o/api/releases/latest')) { Fail "api: fallback not asked: $($b.hits -join '; ')" }
    if ($b.log -notmatch 'AIQ-UPDATE-REDIRECT') { Fail 'api: log does not say why it fell back' }
    if ($b.state.source -ne 'api') { Fail "api: update.json source is '$($b.state.source)', want api" }

    $c = Run 'limited'
    if ($c.code -ne 1) { Fail "limited: exit $($c.code), want 1" }
    if (-not $c.state.check_error) { Fail "limited: no check_error: $($c.state | ConvertTo-Json -Compress)" }
    if ($c.state.retry_at -ne '2026-11-04T02:40:00Z') { Fail "limited: retry_at '$($c.state.retry_at)', want 2026-11-04T02:40:00Z" }
    if ($c.staged) { Fail 'limited: something was staged' }
} finally {
    try { Invoke-WebRequest -UseBasicParsing "$base/stop" | Out-Null } catch { }
    Stop-Job $server -ErrorAction SilentlyContinue
    Remove-Job -Force $server -ErrorAction SilentlyContinue
    Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
}
if ($failed) { exit 1 }
Write-Host 'upgrade.ps1 network path: redirect, API fallback and rate-limit reset all as expected.'
