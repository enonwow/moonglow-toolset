# Offline wiring proof: parse the launcher; execute ONLY the extracted helper with mocks.
$ErrorActionPreference = 'Stop'
$tokens = $null; $errors = $null
$common = [Management.Automation.Language.Parser]::ParseInput([IO.File]::ReadAllText((Join-Path $PSScriptRoot 'common.ps1')),  [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw $errors }
$fn = $common.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Start-LiveServerProcess' }, $true)
. ([scriptblock]::Create($fn.Extent.Text))
function ConvertTo-LiveWslPath([string]$Path) { $Path }
function Start-LiveWslProcess { param($Arguments, $Out, $Name) $script:Captured = $Arguments }
$LiveRepo = Split-Path (Split-Path $PSScriptRoot)
$LiveGameRoot = $null
$savedRoot = $env:NWNX_ROOT
$savedPlayers = $env:MGLIVE_ALLOW_PLAYER_OPS
try {
    $env:NWNX_ROOT = 'offline-mock'
    $env:MGLIVE_ALLOW_PLAYER_OPS = '1'
    Start-LiveServerProcess -Module 'fixture.mod' -Areas 'events' -Out 'offline'
    if ($Captured -notcontains 'MGLIVE_ALLOW_PLAYER_OPS=0') { throw 'Default must explicitly disable, even with ambient env=1' }
    Start-LiveServerProcess -Module 'fixture.mod' -Areas 'events' -Out 'offline' -AllowPlayerOps
    if ($Captured -notcontains 'MGLIVE_ALLOW_PLAYER_OPS=1') { throw 'Explicit switch was not forwarded' }
    $demo = [Management.Automation.Language.Parser]::ParseInput([IO.File]::ReadAllText((Join-Path $PSScriptRoot 'Start-LiveDemo.ps1')),  [ref]$tokens, [ref]$errors)
    if ($errors.Count) { throw $errors }
    if ('AllowPlayerOps' -notin $demo.ParamBlock.Parameters.Name.VariablePath.UserPath) { throw 'Missing public parameter' }
    $call = $demo.Find({ param($node) $node -is [Management.Automation.Language.CommandAst] -and $node.GetCommandName() -eq 'Start-LiveServerProcess' }, $true)
    if ($call.Extent.Text -notmatch '-AllowPlayerOps:\$AllowPlayerOps') { throw 'Demo does not forward its switch' }
    Write-Output 'Player switch: default off, explicit on, launcher wiring verified; no process started.'
} finally { $env:NWNX_ROOT = $savedRoot; $env:MGLIVE_ALLOW_PLAYER_OPS = $savedPlayers }
