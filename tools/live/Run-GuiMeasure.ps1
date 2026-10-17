#requires -Version 7.0
param(
    [Alias('Profile')][ValidateSet('Debug','DevOpt','Release')][string]$BuildProfile='Debug',
    [string[]]$Cases=@('static','reports-only','quiet','reports','idle','moving','effects-markers','effects-full','hidden','minimized','panel','log','chat-loaded','log-loaded','cues','speech','focus-loaded','areas-loaded','legend','macros'),
    [ValidateRange(5,300)][int]$Seconds=12,
    [ValidatePattern('^[a-z0-9-]+$')][string]$Tag,
    [switch]$Heap,
    [switch]$SkipBuild
)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
$out=Join-Path $LiveRepo 'target/test-output/live-measure'
New-Item -ItemType Directory -Force $out | Out-Null
if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot 'check.ps1') -Phase 8 -Next -Step measure -MeasureGui -MeasureProfile $BuildProfile -MeasureHeap:$Heap -Jobs 1 -NoUiDebug
}
$kind=if($BuildProfile -eq 'Release') {'release'} else {'debug'}
$exe=Join-Path $LiveRepo "target/$kind/examples/live_measure.exe"
$hash=(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
$fixturePath=Join-Path $LiveRepo 'target/test-output/live-next/review-profile-fixture.json'
$modulePath=Join-Path $LiveRepo 'target/test-output/live-demo/DEMO-Live-01.mod'
$fixtureHash=(Get-FileHash -LiteralPath $fixturePath -Algorithm SHA256).Hash
$moduleHash=(Get-FileHash -LiteralPath $modulePath -Algorithm SHA256).Hash
$receipt=Get-Content -LiteralPath (Join-Path $out "gui-build-$kind.json") -Raw | ConvertFrom-Json
if ($receipt.profile -ne $BuildProfile -or $receipt.heap -ne [bool]$Heap -or $receipt.sha256 -ne $hash) {
    throw 'Build receipt does not match this profile/heap/binary; run without -SkipBuild.'
}
$names=@('NWN_ROOT','NWN_HOME','APPDATA','MGLIVE_MEASURE_OUT','MGLIVE_MEASURE_CASE','MGLIVE_MEASURE_PROFILE','MGLIVE_MEASURE_SECONDS') + @(Get-ChildItem Env: | Where-Object Name -Like 'MOONGLOW_LIVE_*' | ForEach-Object Name)
$saved=@{}
foreach($name in $names) {$saved[$name]=[Environment]::GetEnvironmentVariable($name,'Process')}
try {
    foreach($name in $names | Where-Object {$_ -like 'MOONGLOW_LIVE_*'}) {[Environment]::SetEnvironmentVariable($name,$null,'Process')}
    $env:NWN_ROOT=$LiveGameRoot
    $env:NWN_HOME=Join-Path $out 'user'
    $env:APPDATA=Join-Path $out 'appdata'
    New-Item -ItemType Directory -Force $env:NWN_HOME,$env:APPDATA | Out-Null
    $env:MGLIVE_MEASURE_PROFILE=$BuildProfile
    $env:MGLIVE_MEASURE_SECONDS=[string]$Seconds
    foreach($case in $Cases) {
        if($case -notin @('static','reports-only','quiet','reports','idle','moving','effects-markers','effects-full','hidden','minimized','panel','log','chat-loaded','log-loaded','cues','speech','focus-loaded','areas-loaded','legend','macros')) {throw "Unknown case $case"}
        $suffix=if($Heap){'-heap'}else{''}
        $stem="$($BuildProfile.ToLower())-$case$suffix"
        if($Tag) { $stem="$Tag-$stem" }
        $env:MGLIVE_MEASURE_OUT=Join-Path $out "$stem.json"
        $env:MGLIVE_MEASURE_CASE=$case
        # Other workers share Cargo's target. Never sample over their compilation.
        $idleDeadline=[DateTime]::UtcNow.AddMinutes(10)
        while (Get-Process cargo,rustc,link,lld-link -ErrorAction SilentlyContinue) {
            if ([DateTime]::UtcNow -gt $idleDeadline) { throw 'Compiler still active; defer the timed replay.' }
            Start-Sleep -Seconds 2
        }
        $began=[DateTime]::UtcNow
        $process=Start-Process -FilePath $exe -WorkingDirectory $LiveRepo -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $out "$stem-stdout.log") -RedirectStandardError (Join-Path $out "$stem-stderr.log")
        if(-not $process.WaitForExit(($Seconds+120)*1000)) {throw "Owned helper $($process.Id) did not exit; no unrelated process stopped"}
        if($process.ExitCode -ne 0) {throw "Measurement $stem failed, inspect its stderr"}
        $file=Get-Item -LiteralPath $env:MGLIVE_MEASURE_OUT
        if($file.LastWriteTimeUtc -lt $began) {throw 'Stale measurement output'}
        $data=Get-Content -LiteralPath $file.FullName -Raw | ConvertFrom-Json
        if ($case -in @('hidden','minimized','static') -and $data.area_pictures -ne 0) { throw "Control $case unexpectedly rendered the area" }
        if ($case -ne 'minimized' -and ($data.minimized_os -or $data.minimized_observed)) { throw "Replay $case was minimized externally; discard and repeat the sample." }
        if ($case -eq 'minimized' -and -not $data.minimized_os) { throw 'OS did not confirm minimization' }
        if ($case -eq 'effects-full' -and $data.census.looks.effect_models -ne 16) { throw 'Full replay must attach all 16 effect models' }
        if ($case -like '*-loaded' -and $data.census.event_query_count -ne 500) { throw 'Populated log control must return 500 rows from its 1000-event ring' }
        if ($case -notin @('focus-loaded','areas-loaded') -and $data.census.focus_entries -ne 0) { throw 'Replay inherited a watch from a prior case' }
        if ($case -in @('focus-loaded','areas-loaded') -and $data.census.focus_rows -ne 500) { throw 'Focus must contain its bounded 500-row history' }
        if ($case -eq 'areas-loaded' -and $data.census.area_views -ne 3) { throw 'Multiple-map control must open three areas' }
        if ($case -eq 'macros' -and $data.census.macro_steps -ne 20) { throw 'Macro control must have 20 steps' }
        if ($case -eq 'cues' -and $data.transient_max -lt 1) { throw 'Cue replay did not display a cue' }
        if ($case -eq 'speech' -and $data.speech_max -lt 1) { throw 'Speech replay did not display a bubble' }
        $data | Add-Member binary_sha256 $hash
        if ((Get-FileHash -LiteralPath $fixturePath).Hash -ne $fixtureHash -or (Get-FileHash -LiteralPath $modulePath).Hash -ne $moduleHash) { throw 'Read-only replay modified an input' }
        $data | Add-Member fixture_sha256 $fixtureHash
        $data | Add-Member module_sha256 $moduleHash
        if ($data.heap_accounting -ne [bool]$Heap) { throw 'Heap flag does not match the built helper' }
        $data | Add-Member git_head (& git -C $LiveRepo rev-parse HEAD)
        $data | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $file.FullName -Encoding utf8
        $data | Select-Object profile,case,cpu_one_core_percent,fps,area_picture_hz
    }
} finally {foreach($name in $names){[Environment]::SetEnvironmentVariable($name,$saved[$name],'Process')}}
