#requires -Version 7.0
param([switch]$SkipBuild, [ValidateSet('1','1b','2')][string]$Phase = '1')
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
$proof = Join-Path $LiveRepo 'target/test-output/live-next'
$demoOut = Join-Path $LiveRepo 'target/test-output/live-demo'
# Never replace the owner's current two-monitor session or borrow its keeper.
$exe = Join-Path $LiveRepo 'target/debug/moonglow.exe'
if (Get-Process moonglow -ErrorAction SilentlyContinue | Where-Object Path -EQ $exe) {
    throw 'Existing Moonglow demo must be closed by its owner before the Phase 1 engine test.'
}
$occupied = Invoke-LiveWsl @('ss', '-H', '-lun', 'sport = :5121')
if ($occupied) { throw 'UDP 5121 is in use; no process was stopped.' }
if (Invoke-LiveHelper 'keep-wsl' @('--status')) {
    throw 'An existing demo keeper is owned by another run; resolve it before this test.'
}
New-Item -ItemType Directory -Force $proof | Out-Null
if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot 'check.ps1') -Phase $Phase -Next -Build -Jobs 1 -NoUiDebug
    $module = Join-Path $demoOut 'DEMO-Live-01.mod'
    $recovery = Join-Path $proof 'accepted-before.mod'
    if ((Test-Path -LiteralPath $module) -and -not (Test-Path -LiteralPath $recovery)) {
        Copy-Item -LiteralPath $module -Destination $recovery
        @{ purpose='Last accepted module until Phase 1 owner acceptance';
           sha256=(Get-FileHash -LiteralPath $recovery).Hash;
           bytes=(Get-Item -LiteralPath $recovery).Length } |
            ConvertTo-Json | Set-Content -LiteralPath (Join-Path $proof 'recovery.json') -Encoding utf8
    }
    $null = & (Join-Path $PSScriptRoot 'demo/build.ps1')
}
$demo = $null
$savedToken = $env:MGLIVE_TEST_TOKEN
$savedUri = $env:MGLIVE_TEST_URI
try {
    $demo = & (Join-Path $PSScriptRoot 'Start-LiveDemo.ps1') -SkipBuild -Quiet -PassThru -AllowChunks -Prefix "mglive-p$Phase"
    $env:MGLIVE_TEST_TOKEN = $demo.Token
    $env:MGLIVE_TEST_URI = $demo.Uri
    & python (Join-Path $PSScriptRoot 'test-next.py') $demoOut $proof $Phase
    if ($LASTEXITCODE -ne 0) { throw "Phase $Phase engine assertions failed; see live-next/engine*.json." }
} finally {
    $env:MGLIVE_TEST_TOKEN = $savedToken
    $env:MGLIVE_TEST_URI = $savedUri
    if ($demo) { & (Join-Path $PSScriptRoot 'Stop-LiveDemo.ps1') -Out $demoOut }
}
