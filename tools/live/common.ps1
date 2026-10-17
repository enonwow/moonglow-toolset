$LiveRepo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
if (-not $global:MoonglowLiveLocalLoaded) {
    $local = Join-Path $PSScriptRoot 'local.ps1'
    if (Test-Path -LiteralPath $local) { . $local }
    $global:MoonglowLiveLocalLoaded = $true
}
$LiveTarget = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $LiveRepo 'target' }
# The newer of the development and the demo's optimized CLI (either may be absent after cargo clean).
$LiveMg = @('debug', 'release') | ForEach-Object { Join-Path $LiveTarget "$_/mg.exe" } | Where-Object { Test-Path -LiteralPath $_ } |
    Sort-Object { (Get-Item -LiteralPath $_).LastWriteTimeUtc } | Select-Object -Last 1
if (-not $LiveMg) { $LiveMg = Join-Path $LiveTarget 'debug/mg.exe' }
$LiveGameRoot = $env:NWN_ROOT
$LiveDistro = if ($Distro) { $Distro } else { $env:MGLIVE_DISTRO }

function Get-LiveWslArgs {
    if ($LiveDistro) { '-d'; $LiveDistro }
    '-e'
}
function Invoke-LiveWsl {
    param([string[]]$Command)
    $arguments = @(Get-LiveWslArgs) + $Command
    & wsl @arguments
    if ($LASTEXITCODE -ne 0) { throw "WSL command failed ($LASTEXITCODE): $($Command[0])" }
}
# 'open' (the API answers), 'gated' (403: up, LLM access off) or $null (not listening yet).
function Test-LiveListener {
    param([string]$Uri, [string]$Token)
    try {
        $null = Invoke-RestMethod $Uri -Headers @{Authorization="Bearer $Token"} -SkipCertificateCheck -NoProxy -TimeoutSec 1
        return 'open'
    } catch {
        if ($_.Exception.Response -and [int]$_.Exception.Response.StatusCode -eq 403) { return 'gated' }
        return $null
    }
}
# With the API closed: Moonglow is subscribed to the game's reports and the game publishes them.
function Test-LiveReportsByRedis {
    param([int]$RedisPort, [string]$Prefix)
    $channel = "$Prefix.state"
    $subscribers = Invoke-LiveWsl @('redis-cli', '-p', "$RedisPort", '--raw', 'PUBSUB', 'NUMSUB', $channel)
    if ([int](@($subscribers)[-1]) -lt 1) { return $false }
    $sample = Invoke-LiveWsl @('bash', '-c', "timeout 3 redis-cli -p $RedisPort --raw SUBSCRIBE '$channel'; true")
    return (($sample -join "`n").Contains('"seq"'))
}
function Invoke-LiveMg {
    param([string[]]$Command, [string]$UserDirectory)
    if (-not (Test-Path -LiteralPath $LiveMg)) { throw 'Build the CLI first: cargo +1.98.0 build -p mg' }
    $arguments = @('--json', '--user-dir', $UserDirectory)
    if ($LiveGameRoot) { $arguments += @('--root', $LiveGameRoot) }
    $output = & $LiveMg @arguments @Command
    if ($LASTEXITCODE -ne 0) { throw "mg failed: $($Command -join ' ')`n$($output -join "`n")" }
    $value = ($output -join "`n") | ConvertFrom-Json
    if ($value.error) { throw $value.error }
    return $value
}
function ConvertTo-LiveWslPath {
    param([string]$Path)
    $result = Invoke-LiveWsl @('wslpath', '-a', '-u', [IO.Path]::GetFullPath($Path))
    return ($result -join '').Trim()
}
function ConvertTo-LiveArgument([string]$Value) {
    # Start-Process joins ArgumentList: quote each Windows argv element explicitly.
    '"' + [regex]::Replace([regex]::Replace($Value, '(\\*)"', '$1$1\"'), '(\\+)$', '$1$1') + '"'
}
function Start-LiveWslProcess {
    param([string[]]$Command, [string]$Out, [string]$Name)
    $arguments = '-e ' + (($Command | ForEach-Object { ConvertTo-LiveArgument $_ }) -join ' ')
    if ($LiveDistro) { $arguments = '-d ' + (ConvertTo-LiveArgument $LiveDistro) + ' ' + $arguments }
    Start-Process -FilePath 'wsl.exe' -ArgumentList $arguments -PassThru -WindowStyle Hidden `
        -RedirectStandardOutput (Join-Path $Out "$Name-stdout.log") -RedirectStandardError (Join-Path $Out "$Name-stderr.log")
}
function Invoke-LiveHelper {
    param([string]$Name, [string[]]$Arguments = @())
    $helper = ConvertTo-LiveWslPath (Join-Path $LiveRepo "tools/live/wsl/$Name.sh")
    Invoke-LiveWsl (@('bash', $helper) + $Arguments)
}
function Start-LiveKeeper {
    param([string]$Out)
    $running = Invoke-LiveHelper 'keep-wsl' @('--status')
    if ($running) { return $null }
    $helper = ConvertTo-LiveWslPath (Join-Path $LiveRepo 'tools/live/wsl/keep-wsl.sh')
    # Only one holder per scratch directory; flock makes repeated starts harmless.
    $process = Start-LiveWslProcess @('bash', $helper) -Out $Out -Name 'keeper'
    Start-Sleep -Milliseconds 300
    if ($process.HasExited -and $process.ExitCode -ne 0) { throw 'WSL keepalive failed; inspect keeper-stderr.log' }
    return $process
}
function Test-LiveRedis {
    param([int]$Port)
    $client = [Net.Sockets.TcpClient]::new()
    try {
        if (-not $client.ConnectAsync('127.0.0.1', $Port).Wait(3000)) { throw 'connect timeout' }
        $stream = $client.GetStream()
        $stream.ReadTimeout = 3000
        $stream.WriteTimeout = 3000
        $bytes = [Text.Encoding]::ASCII.GetBytes("*1`r`n`$4`r`nPING`r`n")
        $stream.Write($bytes)
        $reader = [IO.StreamReader]::new($stream, [Text.Encoding]::ASCII)
        if ($reader.ReadLine() -ne '+PONG') { throw 'unexpected Redis reply' }
    } catch { throw "Redis unreachable — is WSL running? $($_.Exception.Message)" }
    finally { $client.Dispose() }
}
function Start-LiveServerProcess {
    param([string]$Module, [string]$Areas, [string]$Out, [int]$RedisPort = 6379,
          [string]$Prefix = 'mglive', [switch]$AllowChunks, [switch]$AllowPlayerOps, [switch]$Passwords)
    if (-not $env:NWNX_ROOT) { throw 'NWNX_ROOT is required (directory containing Binaries/)' }
    $helper = ConvertTo-LiveWslPath (Join-Path $LiveRepo 'tools/live/wsl/start-server.sh')
    $moduleWsl = ConvertTo-LiveWslPath $Module
    $environment = @("NWNX_ROOT=$env:NWNX_ROOT", "MGLIVE_REDIS_PORT=$RedisPort", "MGLIVE_PREFIX=$Prefix", "MGLIVE_ALLOW_CHUNK=$([int][bool]$AllowChunks)", "MGLIVE_ALLOW_PLAYER_OPS=$([int][bool]$AllowPlayerOps)", "MGLIVE_PASSWORDS=$([int][bool]$Passwords)")
    if ($env:NWN_SERVER) { $environment += "NWN_SERVER=$env:NWN_SERVER" }
    if ($env:MGLIVE_TEXT_CODEPAGE) {
        if ($env:MGLIVE_TEXT_CODEPAGE -notin @('cp1250','cp1251','cp1252')) { throw 'MGLIVE_TEXT_CODEPAGE must be cp1250, cp1251 or cp1252' }
        $environment += "MGLIVE_TEXT_CODEPAGE=$env:MGLIVE_TEXT_CODEPAGE"
    }
    if ($LiveGameRoot) { $environment += "NWN_ROOT=$(ConvertTo-LiveWslPath $LiveGameRoot)" }
    Start-LiveWslProcess (@('env') + $environment + @('bash', $helper, $moduleWsl, $Areas)) -Out $Out -Name 'server'
}

# An explicit local/environment choice wins. Otherwise prefer the separately
# built reconnect plugin, without embedding a workstation's Linux username.
if (-not $env:NWNX_ROOT) {
    $candidate = Invoke-LiveWsl @('python3', '-c', 'from pathlib import Path; p=Path.home()/"nwnx-redis-reconnect"; print(p if (p/"Binaries/NWNX_Redis.so").is_file() else "")')
    if ($candidate) { $env:NWNX_ROOT = ($candidate -join '').Trim() }
}
