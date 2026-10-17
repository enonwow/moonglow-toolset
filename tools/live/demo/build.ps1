param(
    [switch]$CaptureDialogue = $true,
    [string]$Base,
    [string]$Distro, [string]$Out, [string]$NwnxZip
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../common.ps1')
$work = & (Join-Path $PSScriptRoot 'prepare.ps1') -Base $Base -Out $Out -Distro $LiveDistro -NwnxZip $NwnxZip
# Optional, hash-bound repair supplied by the owner for this read-only base.
# It changes only the scratch module and only an exact obsolete script reference.
if ($env:MGLIVE_BASE_REPAIR) {
    $repair = $env:MGLIVE_BASE_REPAIR | ConvertFrom-Json
    if ($work.Inputs.baseSha256 -eq $repair.sha256) {
        if ($repair.resource -notmatch '^[A-Za-z0-9_-]{1,16}\.git$' -or
            $repair.field -notmatch '^Script[A-Za-z]+$' -or $repair.list -ne 'Creature List') { throw 'Invalid base repair configuration' }
        $resource = Join-Path $work.Module $repair.resource
        $git = (& $LiveMg gff $resource | Out-String | ConvertFrom-Json)
        if ($LASTEXITCODE -ne 0) { throw 'Could not inspect the configured scratch repair resource' }
        for ($i = 0; $i -lt $git.($repair.list).value.Count; $i++) {
            if ($git.($repair.list).value[$i].($repair.field).value -eq $repair.value) {
                $null = Invoke-LiveMg -UserDirectory $work.User -Command @('set', $work.Module, $repair.resource, "/$($repair.list)[$i]/$($repair.field)=")
            }
        }
    }
}
# Generic Live preparation, compilation and verification live in Rust.
# Keep only the demo's hash-bound base repair and dependency staging above.
$stage = Join-Path $work.Out 'live-build-stage'
$marker = Join-Path $stage '.moonglow-live-build-stage'
if (Test-Path -LiteralPath $stage) {
    if ((Get-Item -LiteralPath $stage).LinkType -or !(Test-Path -LiteralPath $marker) -or (Get-Content -LiteralPath $marker -Raw).Trim() -ne 'moonglow-live-demo') { throw 'Unowned build staging folder' }
} else {
    New-Item -ItemType Directory $stage | Out-Null
    'moonglow-live-demo' | Set-Content -LiteralPath $marker
}
$candidate = Join-Path $stage 'DEMO-Live-01.mod'
$receipt = Join-Path $work.Out 'live-build.json'
$args = @('live-build', $work.Module, '--nwnx', $work.Nwnx, '--out', $candidate)
if ($CaptureDialogue) { $args += '--capture-dialogue' }
$report = Invoke-LiveMg -UserDirectory $work.User -Command $args
if (-not $report.ok) { throw ($report.errors -join "`n") }
$previousBuild = if (Test-Path -LiteralPath $receipt) { Get-Content -LiteralPath $receipt -Raw | ConvertFrom-Json } else { $null }
# The module is the demo's established output; sidecars must be from our receipt.
foreach ($field in @('output','settings','start_script')) {
    $source = $report.$field
    $destination = Join-Path $work.Out ([IO.Path]::GetFileName($source))
    if ((Test-Path -LiteralPath $destination) -and ((Get-Item -LiteralPath $destination).LinkType -or ($field -ne 'output' -and $previousBuild.$field -ne $destination))) { throw "Unowned output: $destination" }
}
# Keep the previous usable result until the Rust build has succeeded.
foreach ($field in @('settings','start_script','output')) {
    $source = $report.$field
    $destination = Join-Path $work.Out ([IO.Path]::GetFileName($source))
    Move-Item -LiteralPath $source -Destination $destination -Force
    $report.$field = $destination
}
$report | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $receipt -Encoding utf8
Write-Host "Verified demo: $($report.output) ($($report.warnings.Count) warnings)"
$report.output
