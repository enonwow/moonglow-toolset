#requires -Version 7.0
param(
    [Parameter(Mandatory)][ValidateSet('1','1b','2','3','4','4b','5','6','7','8','A','B','C','D','E','G','F')][string]$Phase,
    [ValidateSet('mg-live','mg-preview','mg-module','mg-render','mg','mg-ui','moonglow-mcp')][string]$Package,
    [string]$Filter,
    [switch]$LibraryOnly,
    [ValidateSet('live_ops_proof')][string]$ProofExample,
    [switch]$NoCapture,
    [ValidateSet('live_prepare_snapshot','live_session_factions_snapshot','live_vfx_transform_snapshot','live_appearance_restore_snapshot','live_inventory_grid_snapshot','live_skill_roll_snapshot','hexxar_followup','live_options','live_demo_snapshot','live_morning_snapshot','live_next_phase1','live_next_phase1b','live_labels_snapshot','live_panel_snapshot','live_combat_snapshot','live_factions_snapshot','live_motion_snapshot','live_grid_snapshot','live_effects_snapshot','live_full_effects_snapshot','live_event_effects_snapshot','live_speech_snapshot','live_scene_snapshot','live_review_legend_snapshot','live_review_group_snapshot','live_review_props_snapshot','live_review_lightshaft_snapshot','live_review_profile_snapshot','live_review_spells_snapshot','live_review_patrol_snapshot','live_review_tiles_snapshot','live_accept2_baselines','live_accept2_layout_snapshot','live_accept2_actions_snapshot','live_accept2_appearance_snapshot')][string]$Picture,
    [ValidateSet('live_portrait','live_watch_areas','live_prepare','live_audit7','live_accept6_focus','live_accept6_broken_looks','live_accept6','live_viewer','live_targeting','live_player_look','live_spawning','live_labels','live_panel','live_motion','live_effects','live_events_view','live_speech','live_scene','live_chat','live_focus','live_review', 'live_accept5','live_accept5_portraits','live_accept5_effects','live_accept5_scene','live_accept5_teleport','live_accept5_factions', 'live_accept4', 'live_accept4_colours', 'live_accept4_windows', 'live_accept4_dialogs', 'live_accept4_placeable', 'live_review_accept3_animation','an_area_s_picture')][string]$UiTest,
    [switch]$Lint,
    [switch]$CheckFmt,
    [switch]$Build,
    [switch]$DemoBuild,
    [switch]$MeasureGui,
    [ValidateSet('Debug','DevOpt','Release')][string]$MeasureProfile = 'Debug',
    [switch]$MeasureHeap,
    [switch]$MeasureTest,
    [switch]$Next,
    [ValidatePattern('^[a-z0-9-]+$')][string]$Step,
    [ValidateRange(1,2)][int]$Jobs = 2,
    [switch]$NoUiDebug,
    [switch]$UiUnoptimized
)
$ErrorActionPreference = 'Stop'
if ($MeasureTest -and !$MeasureGui) { throw 'MeasureTest requires MeasureGui.' }
if ($LibraryOnly -and !$Package) { throw 'LibraryOnly requires -Package.' }
if ($Build -and $DemoBuild) { throw 'Choose either -Build or -DemoBuild, not both.' }
if ($DemoBuild -and $UiUnoptimized) { throw 'DemoBuild uses Release; UiUnoptimized is for development tests.' }
if ($DemoBuild) { $Jobs = 1 }
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$out = Join-Path $repo $(if ($Phase -match '^[A-Z]$') { 'target/test-output/live-redis' } else { 'target/test-output/live-mode' })
if ($Next) { $out = Join-Path $repo 'target/test-output/live-next' }
New-Item -ItemType Directory -Force $out | Out-Null
Push-Location $repo
$previousRoot = $env:NWN_ROOT
$previousJobs = $env:CARGO_BUILD_JOBS
try {
    # Linking this workspace's many test binaries at once can exhaust RAM.
    $env:CARGO_BUILD_JOBS = [string]$Jobs
    # This Windows check must not launch nwserver.exe. Real engine proof is
    # performed by the private WSL demo; GPU view checks use the real install.
    $env:NWN_ROOT = Join-Path $out 'no-native-game'
    $checks = @(
        @{ Name = 'fmt'; Args = @('fmt') },
        @{ Name = 'clippy'; Args = @('clippy', '--workspace', '--all-targets', '--', '-D', 'warnings') },
        @{ Name = 'live'; Args = @('test', '-p', 'mg-live', '--features', 'build') },
        @{ Name = 'workspace'; Args = @('test', '--workspace', '--no-fail-fast') },
        @{ Name = 'fmt-check'; Args = @('fmt', '--check') }
    )
    # Shared-tree checks must not reformat another worker's files.
    if ($CheckFmt) { $checks = $checks | Where-Object { $_.Name -ne 'fmt' } }
    if ($Package) {
        $testArgs = @('test', '-p', $Package)
        # Preparation fixture tests require the optional module builder.
        if ($Package -eq 'mg-live') { $testArgs += @('--features', 'build') }
        if ($LibraryOnly) { $testArgs += '--lib' }
        if ($Filter) { $testArgs += $Filter }
        if ($NoCapture) { $testArgs += @('--', '--nocapture') }
        $checks = @(@{ Name = 'focused'; Args = $testArgs })
    }
    if ($ProofExample) {
        # Explicit mg-live proof helper only; never rebuild mg or moonglow apps.
        $checks = @(@{ Name = 'proof-helper'; Args = @('build', '-p', 'mg-live', '--example', $ProofExample) })
    }
    if ($Lint) {
        # Metadata checks only: safe while the owner uses the built apps.
        if ($Package) {
            $checks = @(@{ Name = 'clippy'; Args = @('clippy', '-p', $Package, '--all-targets', '--', '-D', 'warnings') })
        } else {
            $checks = $checks | Where-Object { $_.Name -in @('fmt', 'clippy', 'fmt-check') }
        }
    }
    if ($Picture) {
        # This explicit binary/filter cannot invoke native engine tests.
        $env:NWN_ROOT = $previousRoot
        . (Join-Path $PSScriptRoot 'common.ps1')
        $env:NWN_ROOT = $script:LiveGameRoot
        $checks = @(@{ Name = 'picture'; Args = @('test', '-p', 'mg-ui', '--test', 'screens', $Picture, '--', '--ignored') })
    }
    if ($UiTest) {
        # Only these viewer tests may read the install; never a native engine test.
        $env:NWN_ROOT = $previousRoot
        . (Join-Path $PSScriptRoot 'common.ps1')
        $env:NWN_ROOT = $script:LiveGameRoot
        $checks = @(@{ Name = $UiTest; Args = @('test', '-p', 'mg-ui', '--test', 'app', $UiTest, '--', '--nocapture') })
    }
    if ($Build) {
        $checks = @(@{ Name = 'build'; Args = @('build', '-p', 'mg', '-p', 'moonglow', '-p', 'moonglow-mcp') })
    }
    if ($DemoBuild) {
        # The demo gateway and the CLI its launcher drives, optimized in a separate artifact path; no lifecycle action.
        $checks = @(@{ Name = 'gateway-build-release'; Args = @('build', '--release', '-p', 'mg-gateway-dev', '--bin', 'mg-gateway-dev') },
            @{ Name = 'mg-build-release'; Args = @('build', '--release', '-p', 'mg') })
    }
    if ($MeasureGui) {
        if ($Build -or $DemoBuild -or $Package -or $Picture -or $UiTest -or $UiUnoptimized) {
            throw 'MeasureGui builds only its diagnostic example, with its explicit profile.'
        }
        $features = if ($MeasureHeap) { 'measure-alloc' } else { 'measure' }
        $measureCommand = if ($Lint) { 'clippy' } elseif ($MeasureTest) { 'test' } else { 'build' }
        $measureArgs = @($measureCommand, '--example', 'live_measure', '-p', 'mg-ui', '--features', $features)
        if ($MeasureProfile -eq 'Release') { $measureArgs += '--release' }
        if ($MeasureProfile -eq 'DevOpt') { $measureArgs += @('--config','profile.dev.opt-level=2') }
        if ($Lint) { $measureArgs += @('--','-D','warnings') }
        $checks = @(@{ Name = "measure-$($MeasureProfile.ToLower())"; Args = $measureArgs })
    }
    foreach ($check in $checks) {
        $prefix = if ($Step) { "phase$Phase-$Step" } else { "phase$Phase" }
        $log = Join-Path $out "$prefix-$($check.Name).log"
        Write-Output "Phase $Phase / $($check.Name)"
        $cargoArgs = $check.Args
        if ($NoUiDebug -and $cargoArgs[0] -in @('test', 'build', 'clippy')) {
            # Leave dependency profiles alone; mg-ui's debug info can exhaust
            # Windows commit capacity while the owner's applications are open.
            $cargoArgs = @($cargoArgs[0], '--config', 'profile.test.package.mg-ui.debug=0',
                '--config', 'profile.dev.package.mg-ui.debug=0') + $cargoArgs[1..($cargoArgs.Count - 1)]
        }
        if ($UiUnoptimized -and $cargoArgs[0] -in @('test', 'build', 'clippy')) {
            # The large app test can still exhaust LLVM memory under contention.
            # This opt-in functional-test profile changes only mg-ui.
            $cargoArgs = @($cargoArgs[0], '--config', 'profile.test.package.mg-ui.opt-level=0',
                '--config', 'profile.dev.package.mg-ui.opt-level=0') + $cargoArgs[1..($cargoArgs.Count - 1)]
        }
        & cargo +1.98.0 @cargoArgs *> $log
        if ($LASTEXITCODE -ne 0) {
            Get-Content -LiteralPath $log -Tail 65
            throw "Check failed: $($check.Name) (see $log)"
        }
        Get-Content -LiteralPath $log -Tail 3
    }
    if ($MeasureGui -and -not $Lint -and -not $MeasureTest) {
        $measurementDir = Join-Path $repo 'target/test-output/live-measure'
        New-Item -ItemType Directory -Force $measurementDir | Out-Null
        $targetProfile = if ($MeasureProfile -eq 'Release') { 'release' } else { 'debug' }
        $measurementExe = Join-Path $repo "target/$targetProfile/examples/live_measure.exe"
        [ordered]@{ profile=$MeasureProfile; heap=[bool]$MeasureHeap; binary=$measurementExe;
            sha256=(Get-FileHash -LiteralPath $measurementExe -Algorithm SHA256).Hash;
            time=[DateTime]::UtcNow.ToString('o') } | ConvertTo-Json |
            Set-Content -LiteralPath (Join-Path $measurementDir "gui-build-$targetProfile.json") -Encoding utf8
    }
} finally {
    $env:NWN_ROOT = $previousRoot
    $env:CARGO_BUILD_JOBS = $previousJobs
    Pop-Location
}
