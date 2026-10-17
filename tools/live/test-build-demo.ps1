# Packaging only, on our authored fixture. Never starts/builds the running demo.
# Run after prepare_private_live_build and mg CLI tests have built debug/mg.
param([string]$NwnxZip = '')
$ErrorActionPreference = 'Stop'
# The NWNX build's NWScript includes: NWNX_ROOT (tools/live/local.ps1) unless given.
if (-not $NwnxZip) {
    if (-not $env:NWNX_ROOT) { throw 'Pass -NwnxZip or set NWNX_ROOT (directory containing Binaries/)' }
    $NwnxZip = "$($env:NWNX_ROOT.TrimEnd('/'))/Binaries/NWScript.zip"
}
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$source = Join-Path $repo 'target/test-output/live-build/source.mod'
$out = Join-Path $repo 'target/test-output/live-build/demo-packaging'
if (-not (Test-Path -LiteralPath $source)) { throw 'Run prepare_private_live_build first' }
$before = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash
$oldRepair = $env:MGLIVE_BASE_REPAIR
try {
    $env:MGLIVE_BASE_REPAIR = ''
    $first = & (Join-Path $PSScriptRoot 'demo/build.ps1') -Base $source -Out $out -NwnxZip $NwnxZip -CaptureDialogue
    $firstHash = (Get-FileHash -LiteralPath $first -Algorithm SHA256).Hash
    $firstReport = Get-Content -LiteralPath (Join-Path $out 'live-build.json') -Raw | ConvertFrom-Json
    $second = & (Join-Path $PSScriptRoot 'demo/build.ps1') -Base $source -Out $out -NwnxZip $NwnxZip -CaptureDialogue
    $secondReport = Get-Content -LiteralPath (Join-Path $out 'live-build.json') -Raw | ConvertFrom-Json
    if ($first -ne $second -or !$firstReport.ok -or !$secondReport.ok -or $secondReport.original_load -ne 'lb_original' -or $secondReport.dialogues.Count -ne 1) { throw 'Packaging report mismatch' }
    if ((Get-FileHash -LiteralPath $second -Algorithm SHA256).Hash -ne $firstHash) { throw 'Repeated build changed bytes' }
    $null = & (Join-Path $PSScriptRoot 'demo/build.ps1') -Base $source -Out $out -NwnxZip $NwnxZip -CaptureDialogue:$false
    $disabled = Get-Content -LiteralPath (Join-Path $out 'live-build.json') -Raw | ConvertFrom-Json
    if (!$disabled.ok -or $disabled.dialogues.Count -ne 0) { throw 'Dialogue switch ignored' }
    if ((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash -ne $before) { throw 'Source changed' }
    if (!(Test-Path -LiteralPath $secondReport.settings) -or !(Test-Path -LiteralPath $secondReport.start_script)) { throw 'Missing sidecars' }
    [ordered]@{ passed=$true; sourceSha256=$before; output=$second; compiled=$secondReport.compiled; dialogues=$secondReport.dialogues.Count; originalLoad=$secondReport.original_load } |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path (Split-Path $out) 'demo-packaging-proof.json') -Encoding utf8
} finally { $env:MGLIVE_BASE_REPAIR = $oldRepair }
