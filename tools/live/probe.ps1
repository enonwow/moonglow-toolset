# Probe only the actual client's HTTPS API. No game reports or game server are created.
param([string]$Distro, [ValidateRange(1,65535)][int]$Port=5199, [ValidateRange(1,65535)][int]$RedisPort=6379)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
$exe = Join-Path $LiveRepo 'target/debug/moonglow.exe'
$scratch = Join-Path $LiveRepo 'target/test-output/live-redis/probe'
if (-not (Test-Path -LiteralPath $exe)) { throw 'Build first: cargo +1.98.0 build -p moonglow' }
New-Item -ItemType Directory -Force $scratch | Out-Null
$keeper = Start-LiveKeeper -Out $scratch
$token = [Convert]::ToHexString([Security.Cryptography.RandomNumberGenerator]::GetBytes(32))
$names = @('MOONGLOW_LIVE_TOKEN','MOONGLOW_LIVE_BIND','MOONGLOW_LIVE_REDIS','MOONGLOW_LIVE_PREFIX','MOONGLOW_LIVE_CHUNKS','APPDATA','NWN_HOME')
$saved = @{}
foreach ($name in $names) { $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
$app = $null
try {
    $env:MOONGLOW_LIVE_TOKEN = $token
    $env:MOONGLOW_LIVE_BIND = "127.0.0.1:$Port"
    $env:MOONGLOW_LIVE_REDIS = "127.0.0.1:$RedisPort"
    $env:MOONGLOW_LIVE_PREFIX = 'mglive-probe'
    $env:MOONGLOW_LIVE_CHUNKS = '0'
    $env:APPDATA = Join-Path $scratch 'gui-data'
    $env:NWN_HOME = Join-Path $scratch 'user'
    New-Item -ItemType Directory -Force $env:APPDATA, $env:NWN_HOME | Out-Null
    $app = Start-Process -FilePath $exe -ArgumentList '--no-plugins' -PassThru -WindowStyle Hidden `
        -RedirectStandardOutput (Join-Path $scratch 'gui-stdout.log') -RedirectStandardError (Join-Path $scratch 'gui-stderr.log')
    $headers = @{ Authorization = "Bearer $token" }
    $uri = "https://127.0.0.1:$Port/live/v1"
    $ready = $false
    for ($attempt=0; $attempt -lt 60; $attempt++) {
        if ($app.HasExited) { throw 'Probe GUI exited before API readiness' }
        try { $status=Invoke-RestMethod "$uri/status" -Headers $headers -SkipCertificateCheck -NoProxy -TimeoutSec 1; $ready=$true; break }
        catch { Start-Sleep -Milliseconds 500 }
    }
    if (-not $ready) { throw 'Probe API did not become ready' }
    if ($status.connected -or $status.areas.Count -ne 0) { throw 'Isolated probe unexpectedly received game state' }
    $cases=@(
        @{path='status'; method='GET'; token=$token; code=200},
        @{path='status'; method='GET'; token='wrong'; code=401},
        @{path='sync'; method='POST'; token=$token; body='{}'; code=404},
        @{path='area/missing'; method='GET'; token=$token; code=404},
        @{path='command'; method='POST'; token=$token; body='{"area":"missing","op":"run_chunk","code":"void main() {}"}'; code=400}
    )
    $results=@(foreach ($case in $cases) {
        $args=@{ Uri="$uri/$($case.path)"; Method=$case.method; Headers=@{Authorization="Bearer $($case.token)"}; SkipCertificateCheck=$true; NoProxy=$true; TimeoutSec=5; SkipHttpErrorCheck=$true }
        if ($case.body) { $args.Body=$case.body; $args.ContentType='application/json' }
        $response=Invoke-WebRequest @args
        if ($response.StatusCode -ne $case.code) { throw "$($case.path): expected $($case.code), got $($response.StatusCode)" }
        @{path=$case.path; status=[int]$response.StatusCode}
    })
    @{passed=$true; results=$results} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $scratch 'probe.json') -Encoding utf8NoBOM
    Write-Output 'Client API probe passed: status, authentication, removed sync route, missing area and chunk refusal.'
} finally {
    if ($app -and -not $app.HasExited) { Stop-Process -Id $app.Id; $null=$app.WaitForExit(10000) }
    if ($keeper) { $null=Invoke-LiveHelper 'stop-keeper' }
    foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name,$saved[$name],'Process') }
}
