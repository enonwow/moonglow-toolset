#requires -Version 7.0
param(
    [Parameter(Mandatory)][string]$Replay,
    [Parameter(Mandatory)][string]$Module,
    [ValidateRange(5,7200)][int]$Seconds=1800,
    [ValidatePattern('^[a-z0-9-]+$')][string]$Tag='baseline',
    [switch]$Restore,
    [string]$Drafts,
    [switch]$RejectDrafts,
    [switch]$SkipBuild
)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
$out=Join-Path $LiveRepo 'target/test-output/live-gui-stress'
$replayPath=(Resolve-Path -LiteralPath $Replay).ProviderPath
$modulePath=(Resolve-Path -LiteralPath $Module).ProviderPath
if (!(Test-Path -LiteralPath (Join-Path $out 'owned-files.json'))) { throw 'Create the task ownership ledger before running a soak.' }
if (!$SkipBuild) {
    & (Join-Path $PSScriptRoot 'check.ps1') -Phase 8 -Next -Step gui-stress -MeasureGui -MeasureProfile Release -Jobs 1 -NoUiDebug
}
$exe=Join-Path $LiveRepo 'target/release/examples/live_measure.exe'
$hash=(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
$receipt=Get-Content -Raw -LiteralPath (Join-Path $LiveRepo 'target/test-output/live-measure/gui-build-release.json') | ConvertFrom-Json
if ($receipt.profile -ne 'Release' -or $receipt.heap -or $receipt.sha256 -ne $hash) { throw 'Release measurement binary/receipt mismatch.' }
$names=@('NWN_ROOT','NWN_HOME','APPDATA','MGLIVE_MEASURE_OUT','MGLIVE_MEASURE_PROFILE','MGLIVE_MEASURE_SECONDS','MGLIVE_STRESS_REPLAY','MGLIVE_STRESS_MODULE','MGLIVE_STRESS_DATA','MGLIVE_STRESS_RESTORE','MGLIVE_STRESS_DRAFTS','MGLIVE_STRESS_REJECT_ONLY') + @(Get-ChildItem Env: | Where-Object Name -Like 'MOONGLOW_LIVE_*' | ForEach-Object Name)
$saved=@{}
foreach($name in $names){$saved[$name]=[Environment]::GetEnvironmentVariable($name,'Process')}
try {
    foreach($name in $names | Where-Object {$_ -like 'MOONGLOW_LIVE_*'}){[Environment]::SetEnvironmentVariable($name,$null,'Process')}
    $env:NWN_ROOT=$LiveGameRoot
    $env:NWN_HOME=Join-Path $out 'user'
    $env:APPDATA=Join-Path $out 'appdata'
    $env:MGLIVE_STRESS_DATA=Join-Path $out 'data'
    New-Item -ItemType Directory -Force $env:NWN_HOME,$env:APPDATA,$env:MGLIVE_STRESS_DATA | Out-Null
    $env:MGLIVE_STRESS_DRAFTS=if($Drafts){(Resolve-Path -LiteralPath $Drafts).ProviderPath}else{$null}
    $env:MGLIVE_STRESS_REJECT_ONLY=if($RejectDrafts){'1'}else{$null}
    $env:MGLIVE_STRESS_REPLAY=$replayPath
    $env:MGLIVE_STRESS_MODULE=$modulePath
    $env:MGLIVE_STRESS_RESTORE=if($Restore){'1'}else{$null}
    $env:MGLIVE_MEASURE_PROFILE='Release'
    $env:MGLIVE_MEASURE_SECONDS=[string]$Seconds
    $env:MGLIVE_MEASURE_OUT=Join-Path $out "$Tag.json"
    if (Get-Process cargo,rustc,link,lld-link -ErrorAction SilentlyContinue) { throw 'A compiler is active; defer the timed run.' }
    $replayHash=(Get-FileHash -LiteralPath $replayPath).Hash
    $moduleHash=(Get-FileHash -LiteralPath $modulePath).Hash
    $draftsHash=if($env:MGLIVE_STRESS_DRAFTS){(Get-FileHash -LiteralPath $env:MGLIVE_STRESS_DRAFTS).Hash}else{$null}
    $began=[DateTime]::UtcNow
    $process=Start-Process -FilePath $exe -WorkingDirectory $LiveRepo -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $out "$Tag-stdout.log") -RedirectStandardError (Join-Path $out "$Tag-stderr.log")
    [ordered]@{pid=$process.Id;process_start_utc=$process.StartTime.ToUniversalTime().ToString('o');started_utc=$began.ToString('o');tag=$Tag;binary=$exe;binary_sha256=$hash;replay=$replayPath;replay_sha256=$replayHash;module=$modulePath;module_sha256=$moduleHash;drafts=$env:MGLIVE_STRESS_DRAFTS;drafts_sha256=$draftsHash;git_head=(& git -C $LiveRepo rev-parse HEAD)} | ConvertTo-Json | Set-Content -Encoding utf8 -LiteralPath (Join-Path $out "$Tag-owner.json")
    $helperStart=$process.StartTime
    function Stop-OwnedMeasurement {
        if (!$process.HasExited) {
            $current=Get-Process -Id $process.Id -ErrorAction Stop
            if ($current.StartTime -ne $helperStart -or $current.Path -ne $exe) {throw 'Diagnostic process identity changed; no process stopped.'}
            $process.Kill()
            $process.WaitForExit()
        }
    }
    $deadline=$began.AddSeconds($Seconds+240)
    while (!$process.WaitForExit(10000)) {
        if ([DateTime]::UtcNow -gt $deadline) { Stop-OwnedMeasurement; throw "Owned helper $($process.Id) exceeded its deadline; inspect its identity/outputs. No other process stopped." }
        $logs=@("$Tag-stdout.log","$Tag-stderr.log") | ForEach-Object {Get-Item -LiteralPath (Join-Path $out $_)}
        if (($logs | Measure-Object Length -Sum).Sum -gt 4MB) { Stop-OwnedMeasurement; throw 'Diagnostic log budget exceeded; inspect owned helper.' }
    }
    $ownerPath=Join-Path $out "$Tag-owner.json"
    $ownerReceipt=Get-Content -Raw -LiteralPath $ownerPath | ConvertFrom-Json
    $ownerReceipt | Add-Member exit_code $process.ExitCode
    $ownerReceipt | Add-Member finished_utc ([DateTime]::UtcNow.ToString('o'))
    $ownerReceipt | ConvertTo-Json | Set-Content -Encoding utf8 -LiteralPath $ownerPath
    if ($process.ExitCode -ne 0) { throw "Soak failed (exit $($process.ExitCode)); inspect $Tag-stderr.log and $Tag.driver.json" }
    $file=Get-Item -LiteralPath $env:MGLIVE_MEASURE_OUT
    if($file.LastWriteTimeUtc -lt $began){throw 'Stale soak result'}
    $result=Get-Content -Raw -LiteralPath $file.FullName | ConvertFrom-Json
    if($result.error -or $result.metrics.counts.minimized_frames -or $result.last_sample.minimized_os){throw 'Invalid soak: replay error or external minimization'}
    if($result.last_sample.core.areas -lt 4 -or $result.last_sample.core.objects -lt 900 -or $result.last_sample.ui.focus_entries -lt 6){throw 'Stress population or watch layout incomplete'}
    if(@($result.last_sample.ui.areas | Where-Object {$_.report_reads -eq 0 -or $_.looks.extras -eq 0}).Count){throw 'A Live area was never rendered'}
    if(!@($result.last_sample.ui.areas | Where-Object {$_.selected -eq 50}).Count){throw 'Stress selection must contain50objects'}
    if($Seconds -ge 60){foreach($window in @('chat','focus','log','panel','selection')){if(!$result.metrics.counts.('calls:window.'+$window)){throw "Stress window not drawn: $window"}}}
    if($result.duration_s -lt $Seconds-1){throw 'Soak duration was too short'}
    if((Get-FileHash -LiteralPath $replayPath).Hash -ne $replayHash -or (Get-FileHash -LiteralPath $modulePath).Hash -ne $moduleHash){throw 'Replay input changed'}
    if ($draftsHash -and (Get-FileHash -LiteralPath $env:MGLIVE_STRESS_DRAFTS).Hash -ne $draftsHash) {throw 'Draft input changed'}
    $result | Add-Member drafts_sha256 $draftsHash
    $result | Add-Member binary_sha256 $hash
    $result | Add-Member replay_sha256 $replayHash
    $result | Add-Member module_sha256 $moduleHash
    $result | Add-Member git_head (& git -C $LiveRepo rev-parse HEAD)
    $result | ConvertTo-Json -Depth 30 | Set-Content -Encoding utf8 -LiteralPath $file.FullName
    $result | Select-Object duration_s,cpu_one_core_percent,error
} finally {
    foreach($name in $names){[Environment]::SetEnvironmentVariable($name,$saved[$name],'Process')}
}
