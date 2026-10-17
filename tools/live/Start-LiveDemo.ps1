param(
    [string]$Base, [string]$Out, [string]$Distro,
    [ValidateRange(1,65535)][int]$Port = 5199,
    [ValidateRange(1,65535)][int]$RedisPort = 6379,
    [string]$Prefix = 'mglive', [string]$Areas, [string]$Token,
    [switch]$CaptureDialogue = $true,
    [switch]$AllowChunks, [switch]$AllowPlayerOps, [switch]$SkipBuild, [switch]$GuiOnly,
    [switch]$Quiet, [switch]$PassThru, [switch]$Passwords
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
. (Join-Path $PSScriptRoot 'demo/retired-assets.ps1')
if (-not $Base) { $Base = $env:MGLIVE_DEMO_BASE }
if (-not $Out) { $Out = Join-Path $LiveRepo 'target/test-output/live-demo' }
$Out = [IO.Path]::GetFullPath($Out)
if ($Areas -and $Areas -notmatch '^[A-Za-z0-9_-]{1,16}(,[A-Za-z0-9_-]{1,16})*$') { throw 'Areas must be comma-separated resrefs' }
if ($Prefix -notmatch '^[A-Za-z0-9_:.-]{1,64}$') { throw 'Invalid Redis prefix' }
New-Item -ItemType Directory -Force $Out | Out-Null
# The demo uses the measured optimized profile; ordinary developer builds stay in debug.
$exe = Join-Path $LiveRepo 'target/release/moonglow.exe'
if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot 'check.ps1') -Phase 8 -Next -Step demo-gui -GuiBuild -Jobs 1 -NoUiDebug | Out-Host
}
if (-not (Test-Path -LiteralPath $exe)) {
    throw 'Missing optimized GUI. Run tools/live/check.ps1 -Phase 8 -Next -GuiBuild -Jobs 1 -NoUiDebug before using -SkipBuild.'
}
# Refuse stale -SkipBuild inputs before starting any keeper, Redis or GUI.
if ($SkipBuild) {
    $cached = Join-Path $Out 'DEMO-Live-01.mod'
    if (-not (Test-Path -LiteralPath $cached)) { throw "Missing demo module: $cached" }
    $info = Invoke-LiveMg -UserDirectory (Join-Path $Out 'user') -Command @('haks', $cached)
    Assert-LiveDemoNoRetiredHak -Names @($info.haks | ForEach-Object { $_.name })
}
$keeper = Start-LiveKeeper -Out $Out
$names = @('MOONGLOW_LIVE_TOKEN','MOONGLOW_LIVE_BIND','MOONGLOW_LIVE_REDIS','MOONGLOW_LIVE_PREFIX','MOONGLOW_LIVE_CHUNKS','NWN_ROOT','NWN_HOME','APPDATA')
$saved = @{}
foreach ($name in $names) { $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
$gui = $null
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
    $env:MOONGLOW_LIVE_TOKEN = $Token
    $env:MOONGLOW_LIVE_BIND = "${address}:$Port"
    $env:MOONGLOW_LIVE_REDIS = "127.0.0.1:$RedisPort"
    $env:MOONGLOW_LIVE_PREFIX = $Prefix
    $env:MOONGLOW_LIVE_CHUNKS = [string][int][bool]$AllowChunks
    $env:NWN_ROOT = $LiveGameRoot
    $env:NWN_HOME = Join-Path $Out 'user'
    $env:APPDATA = Join-Path $Out 'gui-data'
    New-Item -ItemType Directory -Force $env:APPDATA | Out-Null
    $gui = Start-Process -FilePath $exe -ArgumentList "--no-plugins $(ConvertTo-LiveArgument $module)" -PassThru -WindowStyle Hidden `
        -RedirectStandardOutput (Join-Path $Out 'gui-stdout.log') -RedirectStandardError (Join-Path $Out 'gui-stderr.log')
    $uri = "https://${address}:$Port/live/v1"
    $ready = $false
    for ($attempt = 0; $attempt -lt 60; $attempt++) {
        if ($gui.HasExited) { throw 'Moonglow exited before its listener was ready' }
        # 403: the listener is up and LLM access is off (Live > Allow LLM); the API stays closed.
        $listener = Test-LiveListener "$uri/status" $Token
        if ($listener) { $ready = $true; break }
        Start-Sleep -Milliseconds 500
    }
    if (-not $ready) { throw 'Live listener did not become ready; inspect gui-stderr.log' }
    if (-not $GuiOnly) {
        $serverProcess = Start-LiveServerProcess -Module $module -Areas $Areas -Out $Out -RedisPort $RedisPort -Prefix $Prefix -AllowChunks:$AllowChunks -AllowPlayerOps:$AllowPlayerOps -Passwords:$Passwords
        $reported = $false
        for ($attempt = 0; $attempt -lt 60; $attempt++) {
            if ($serverProcess.HasExited) { throw 'WSL server exited; inspect server-stderr.log and server-stdout.log' }
            if ($listener -eq 'open') {
                $status = Invoke-RestMethod "$uri/status" -Headers @{Authorization="Bearer $Token"} -SkipCertificateCheck -NoProxy -TimeoutSec 2
                if ($status.connected -and $status.areas.Count -gt 0) { $reported = $true; break }
            } elseif (Test-LiveReportsByRedis -RedisPort $RedisPort -Prefix $Prefix) { $reported = $true; break }
            Start-Sleep -Milliseconds 500
        }
        if (-not $reported) { throw 'No area report arrived; inspect NWNX Redis configuration and MGLIVE_AREAS' }
    }
    [ordered]@{ owner='moonglow-live-demo'; guiProfile='release'; guiPid=$gui.Id; guiStarted=$gui.StartTime.ToUniversalTime().ToString('o'); address=$address; port=$Port; out=$Out; distro=$LiveDistro; redisPort=$RedisPort; prefix=$Prefix; areas=$Areas } |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $Out 'session.json') -Encoding utf8
    if (-not $Quiet) {
        Write-Host "Live: API on $uri; Redis 127.0.0.1:$RedisPort (private WSL server, UDP 5121)"
        Write-Host "Token: $Token"
        Write-Host "curl.exe -k -H `"Authorization: Bearer $Token`" $uri/status"
        Write-Host 'Stop with tools/live/Stop-LiveDemo.ps1; it also releases the WSL keepalive.'
    }
    if ($PassThru) { [pscustomobject]@{ Gui=$gui; Server=$serverProcess; Token=$Token; Uri=$uri; Out=$Out; Address=$address; Port=$Port; RedisPort=$RedisPort; Prefix=$Prefix; Areas=$Areas } }
} catch {
    if ($serverProcess -and -not $serverProcess.HasExited) { $null = Invoke-LiveHelper 'stop-server' }
    if ($gui -and -not $gui.HasExited) { Stop-Process -Id $gui.Id; $null = $gui.WaitForExit(10000) }
    if (-not $GuiOnly) { $null = Invoke-LiveHelper 'stop-keeper' }
    throw
} finally {
    foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process') }
}
