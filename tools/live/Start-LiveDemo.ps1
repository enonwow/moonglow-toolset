param(
    [string]$Base, [string]$Out, [string]$Distro,
    [ValidateRange(1,65535)][int]$Port = 5199,
    [ValidateRange(1,65535)][int]$RedisPort = 6379,
    [string]$Prefix = 'mglive', [string]$Areas, [string]$Token,
    [switch]$CaptureDialogue = $true,
    [switch]$AllowChunks, [switch]$AllowPlayerOps, [switch]$SkipBuild, [switch]$GatewayOnly,
    [switch]$Quiet, [switch]$PassThru, [switch]$Passwords
)
# Starts the loopback development gateway (apps/mg-gateway-dev) next to the private WSL game
# server. Moonglow connects to the gateway as to any Live gateway (Options > Live).
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
. (Join-Path $PSScriptRoot 'demo/retired-assets.ps1')
if (-not $Base) { $Base = $env:MGLIVE_DEMO_BASE }
if (-not $Out) { $Out = Join-Path $LiveRepo 'target/test-output/live-demo' }
$Out = [IO.Path]::GetFullPath($Out)
if ($Areas -and $Areas -notmatch '^[A-Za-z0-9_-]{1,16}(,[A-Za-z0-9_-]{1,16})*$') { throw 'Areas must be comma-separated resrefs' }
if ($Prefix -notmatch '^[A-Za-z0-9_:.-]{1,64}$') { throw 'Invalid Redis prefix' }
New-Item -ItemType Directory -Force $Out | Out-Null
# The demo uses the optimized profile; ordinary developer builds stay in debug.
$exe = Join-Path $LiveRepo 'target/release/mg-gateway-dev.exe'
if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot 'check.ps1') -Phase 8 -Next -Step demo-gateway -DemoBuild -Jobs 1 -NoUiDebug | Out-Host
}
if (-not (Test-Path -LiteralPath $exe)) {
    throw 'Missing optimized gateway. Run tools/live/check.ps1 -Phase 8 -Next -DemoBuild -Jobs 1 -NoUiDebug before using -SkipBuild.'
}
# Refuse stale -SkipBuild inputs before starting any keeper, Redis or gateway.
if ($SkipBuild) {
    $cached = Join-Path $Out 'DEMO-Live-01.mod'
    if (-not (Test-Path -LiteralPath $cached)) { throw "Missing demo module: $cached" }
    $info = Invoke-LiveMg -UserDirectory (Join-Path $Out 'user') -Command @('haks', $cached)
    Assert-LiveDemoNoRetiredHak -Names @($info.haks | ForEach-Object { $_.name })
}
$keeper = Start-LiveKeeper -Out $Out
$names = @('MOONGLOW_GATEWAY_TOKEN','MOONGLOW_LIVE_REDIS','MOONGLOW_LIVE_PREFIX','MOONGLOW_LIVE_CHUNKS','MOONGLOW_LIVE_PLAYERS')
$saved = @{}
foreach ($name in $names) { $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
$gateway = $null
$serverProcess = $null
try {
    $null = Invoke-LiveHelper 'start-redis' @("$RedisPort")
    Test-LiveRedis -Port $RedisPort
    if (-not $SkipBuild) { $null = & (Join-Path $PSScriptRoot 'demo/build.ps1') -Base $Base -Out $Out -Distro $LiveDistro -CaptureDialogue:$CaptureDialogue }
    $module = Join-Path $Out 'DEMO-Live-01.mod'
    if (-not (Test-Path -LiteralPath $module)) { throw "Missing demo module: $module" }
    $address = '127.0.0.1'
    if (-not $Token) { $Token = [Convert]::ToHexString([Security.Cryptography.RandomNumberGenerator]::GetBytes(32)) }
    if ($Token -notmatch '^[A-Za-z0-9_-]{32,256}$') { throw 'Use a token with 32-256 ASCII letters, digits, hyphens or underscores' }
    # The token travels in the environment, never in the configuration file or arguments.
    $env:MOONGLOW_GATEWAY_TOKEN = $Token
    $env:MOONGLOW_LIVE_REDIS = "127.0.0.1:$RedisPort"
    $env:MOONGLOW_LIVE_PREFIX = $Prefix
    $env:MOONGLOW_LIVE_CHUNKS = [string][int][bool]$AllowChunks
    $env:MOONGLOW_LIVE_PLAYERS = [string][int][bool]$AllowPlayerOps
    # The data directory is required state across gateway restarts, not a cache.
    $config = Join-Path $Out 'gateway.json'
    [ordered]@{ port = $Port; data_dir = (Join-Path $Out 'gateway-data') } | ConvertTo-Json | Set-Content -LiteralPath $config -Encoding utf8
    $gateway = Start-Process -FilePath $exe -ArgumentList "--config $(ConvertTo-LiveArgument $config)" -PassThru -WindowStyle Hidden `
        -RedirectStandardOutput (Join-Path $Out 'gateway-stdout.log') -RedirectStandardError (Join-Path $Out 'gateway-stderr.log')
    $uri = "http://${address}:$Port"
    $ready = $false
    for ($attempt = 0; $attempt -lt 60; $attempt++) {
        if ($gateway.HasExited) { throw 'The gateway exited before it was ready; inspect gateway-stdout.log' }
        try { $null = Invoke-RestMethod "$uri/health" -NoProxy -TimeoutSec 1; $ready = $true; break }
        catch { Start-Sleep -Milliseconds 500 }
    }
    if (-not $ready) { throw 'The gateway did not become ready; inspect gateway-stdout.log' }
    if (-not $GatewayOnly) {
        $serverProcess = Start-LiveServerProcess -Module $module -Areas $Areas -Out $Out -RedisPort $RedisPort -Prefix $Prefix -AllowChunks:$AllowChunks -AllowPlayerOps:$AllowPlayerOps -Passwords:$Passwords
        $reported = $false
        for ($attempt = 0; $attempt -lt 60; $attempt++) {
            if ($serverProcess.HasExited) { throw 'WSL server exited; inspect server-stderr.log and server-stdout.log' }
            if ($gateway.HasExited) { throw 'The gateway exited; inspect gateway-stdout.log' }
            if (Test-LiveReportsByRedis -RedisPort $RedisPort -Prefix $Prefix) { $reported = $true; break }
            Start-Sleep -Milliseconds 500
        }
        if (-not $reported) { throw 'No area report arrived; inspect NWNX Redis configuration and MGLIVE_AREAS' }
    }
    [ordered]@{ owner='moonglow-live-demo'; gatewayProfile='release'; gatewayPid=$gateway.Id; gatewayStarted=$gateway.StartTime.ToUniversalTime().ToString('o'); address=$address; port=$Port; out=$Out; distro=$LiveDistro; redisPort=$RedisPort; prefix=$Prefix; areas=$Areas } |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $Out 'session.json') -Encoding utf8
    if (-not $Quiet) {
        Write-Host "Live: gateway on $uri; Redis 127.0.0.1:$RedisPort (private WSL server, UDP 5121)"
        Write-Host "Token: $Token"
        Write-Host "Add $uri with this token in Moonglow's Options > Live; $uri/health answers without it."
        Write-Host 'Stop with tools/live/Stop-LiveDemo.ps1; it also releases the WSL keepalive.'
    }
    if ($PassThru) { [pscustomobject]@{ Gateway=$gateway; Server=$serverProcess; Token=$Token; Uri=$uri; Out=$Out; Address=$address; Port=$Port; RedisPort=$RedisPort; Prefix=$Prefix; Areas=$Areas } }
} catch {
    if ($serverProcess -and -not $serverProcess.HasExited) { $null = Invoke-LiveHelper 'stop-server' }
    if ($gateway -and -not $gateway.HasExited) { Stop-Process -Id $gateway.Id; $null = $gateway.WaitForExit(10000) }
    if (-not $GatewayOnly) { $null = Invoke-LiveHelper 'stop-keeper' }
    throw
} finally {
    foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process') }
}
