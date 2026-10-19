param([string]$Out, [string]$Distro, [switch]$GatewayOnly)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
if (-not $Out) { $Out = Join-Path $LiveRepo 'target/test-output/live-demo' }
$sessionPath = Join-Path $Out 'session.json'
if (Test-Path -LiteralPath $sessionPath) {
    $session = Get-Content -LiteralPath $sessionPath -Raw | ConvertFrom-Json
    if (-not $Distro -and $session.distro) { $LiveDistro = $session.distro }
    if ($session.owner -ne 'moonglow-live-demo') { throw 'Unrecognized session owner' }
    # Never trust an arbitrary path: only the recorded profile of this repository's gateway.
    if ($session.gatewayPid) {
        if ($session.gatewayProfile -notin @('debug', 'release')) { throw 'Unrecognized gateway profile; session retained' }
        $expected = Join-Path $LiveRepo "target/$($session.gatewayProfile)/mg-gateway-dev.exe"
        $gateway = Get-Process -Id $session.gatewayPid -ErrorAction SilentlyContinue
        if ($gateway -and $gateway.Path -eq $expected -and
            $gateway.StartTime.ToUniversalTime().Ticks -eq ([DateTime]$session.gatewayStarted).ToUniversalTime().Ticks) {
            Stop-Process -Id $gateway.Id
            if (-not $gateway.WaitForExit(10000)) { throw 'Owned gateway process did not exit' }
        }
    }
}
if (-not $GatewayOnly) {
    $null = Invoke-LiveHelper 'stop-server'
    $null = Invoke-LiveHelper 'stop-keeper'
}
