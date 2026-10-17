param([string]$Out, [string]$Distro, [switch]$GuiOnly)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
if (-not $Out) { $Out = Join-Path $LiveRepo 'target/test-output/live-demo' }
$sessionPath = Join-Path $Out 'session.json'
if (Test-Path -LiteralPath $sessionPath) {
    $session = Get-Content -LiteralPath $sessionPath -Raw | ConvertFrom-Json
    if (-not $Distro -and $session.distro) { $LiveDistro = $session.distro }
    if ($session.owner -ne 'moonglow-live-demo') { throw 'Unrecognized session owner' }
    # Old sessions predate profile recording and always used debug. Never trust an arbitrary path.
    $guiBuildProfile = if ($session.guiProfile) { [string]$session.guiProfile } else { 'debug' }
    if ($guiBuildProfile -notin @('debug', 'release')) { throw 'Unrecognized GUI profile; session retained' }
    $expectedGui = Join-Path $LiveRepo "target/$guiBuildProfile/moonglow.exe"
    $gui = Get-Process -Id $session.guiPid -ErrorAction SilentlyContinue
    if ($gui -and $gui.Path -eq $expectedGui -and
        $gui.StartTime.ToUniversalTime().Ticks -eq ([DateTime]$session.guiStarted).ToUniversalTime().Ticks) {
        Stop-Process -Id $gui.Id
        if (-not $gui.WaitForExit(10000)) { throw 'Owned Moonglow process did not exit' }
    }
    # Forced termination cannot run Rust's descriptor Drop. The dedicated demo
    # directory and its verified session record authorize removing this one
    # stale credential after its recorded process has exited.
    if (-not (Get-Process -Id $session.guiPid -ErrorAction SilentlyContinue)) {
        $descriptorPath = Join-Path $Out "gui-data/Moonglow/live/sessions/$($session.guiPid).json"
        if (Test-Path -LiteralPath $descriptorPath) {
            $descriptor = Get-Content -LiteralPath $descriptorPath -Raw | ConvertFrom-Json
            if ($descriptor.protocol -ne 'moonglow-agent/1' -or $descriptor.pid -ne $session.guiPid -or
                $descriptor.url -ne "https://$($session.address):$($session.port)") { throw 'Unexpected demo descriptor; retained' }
            Remove-Item -LiteralPath $descriptorPath
        }
    }
}
if (-not $GuiOnly) {
    $null = Invoke-LiveHelper 'stop-server'
    $null = Invoke-LiveHelper 'stop-keeper'
}
