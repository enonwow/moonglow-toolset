# Reproduce the Morning acceptance with curl and a real MCP SDK client.
param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
$out = Join-Path $LiveRepo 'target/test-output/live-demo'
$morning = Join-Path $out 'morning'
New-Item -ItemType Directory -Force $morning | Out-Null
$demo = $null
$evidence = [ordered]@{ passed=$false; transport='curl'; startedUtc=[DateTime]::UtcNow.ToString('o'); commands=@() }
function Save-Morning([string]$Name, $Value) {
    $Value | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath (Join-Path $morning "$Name.json") -Encoding utf8NoBOM
}
function Invoke-Curl([string]$Path, $Body = $null) {
    $arguments = @('--silent','--show-error','--fail-with-body','--noproxy','*','-k',
        '--max-time','35','-H',"Authorization: Bearer $($script:demo.Token)")
    if ($null -ne $Body) {
        $arguments += @('-H','Content-Type: application/json','--data-binary','@-')
        $raw = ($Body | ConvertTo-Json -Depth 12 -Compress) | & curl.exe @arguments "$($script:demo.Uri)/$Path"
    } else { $raw = & curl.exe @arguments "$($script:demo.Uri)/$Path" }
    if ($LASTEXITCODE -ne 0) { throw "curl failed for $Path ($LASTEXITCODE): $raw" }
    return ($raw -join "`n") | ConvertFrom-Json
}
function Wait-Morning([scriptblock]$Condition) {
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    while ([DateTime]::UtcNow -lt $deadline) {
        $value = & $Condition
        if ($value) { return $value }
        Start-Sleep -Milliseconds 200
    }
    throw "Morning acceptance timed out: $Condition"
}
try {
    if (-not $env:MGLIVE_MCP_CLIENT) { throw 'Set MGLIVE_MCP_CLIENT to an installed @modelcontextprotocol/client package in local.ps1' }
    $sourceHash = (Get-FileHash -LiteralPath $env:MGLIVE_DEMO_BASE -Algorithm SHA256).Hash
    $evidence.sourceModuleSha256 = $sourceHash
    $evidence.binaries = @('mg','moonglow','moonglow-mcp') | ForEach-Object {
        @{name=$_; sha256=(Get-FileHash -LiteralPath (Join-Path $LiveRepo "target/debug/$_.exe") -Algorithm SHA256).Hash}
    }
    # This owns Redis 6380 only, and stops its server/GUI/keeper before returning.
    & (Join-Path $PSScriptRoot 'test-demo.ps1') -SkipBuild:$SkipBuild -Out $out
    $e2e = Get-Content -LiteralPath (Join-Path $out 'e2e-redis.json') -Raw | ConvertFrom-Json
    if (-not $e2e.passed) { throw 'Redis end-to-end acceptance did not pass' }
    Save-Morning 'e2e-redis' $e2e
    $evidence.demoModuleSha256 = (Get-FileHash -LiteralPath (Join-Path $out 'DEMO-Live-01.mod') -Algorithm SHA256).Hash
    $script:demo = & (Join-Path $PSScriptRoot 'Start-LiveDemo.ps1') -SkipBuild -AllowChunks -Quiet -PassThru -Out $out
    $area = ($demo.Areas -split ',')[0]
    # Owner-selected acceptance actor: a temporary spawn, without the demo's
    # competing city follower routine. Server shutdown discards this runtime object.
    $setup = Invoke-Curl 'command' @{area=$area;id='morning-curl-setup';op='spawn';kind='creature';r='nw_bartender';x=25;y=25;z=0;f=0}
    $made = Wait-Morning { (Invoke-Curl 'status').results | Where-Object { $_.result.id -eq $setup.queued } | Select-Object -First 1 }
    if (-not $made.result.ok -or -not $made.result.obj) { throw 'Morning bartender spawn failed' }
    $evidence.setup = $made.result
    $before = Wait-Morning {
        $state = Invoke-Curl "area/$area"
        if ($state.creatures | Where-Object id -EQ $made.result.obj) { $state }
    }
    $npc = $before.creatures | Where-Object id -EQ $made.result.obj | Select-Object -First 1
    if (-not $npc.own -or $npc.pc -or $npc.dm -or $npc.possessed) { throw 'Spawned acceptance actor is not an owned NPC' }
    Save-Morning 'curl-before' $before
    $evidence.target = $npc.id
    $evidence.area = $area
    $commands = @(
        @{op='npc_move'; target=$npc.id; x=20; y=20; z=0; run=$true},
        @{op='npc_say'; target=$npc.id; text='Good morning!'},
        @{op='run_script'; target=$npc.id; script='mglive_probe'},
        @{op='run_chunk'; target=$npc.id; code='SetName(OBJECT_SELF, "Chunk ran"); AssignCommand(OBJECT_SELF, ActionJumpToLocation(Location(GetArea(OBJECT_SELF), Vector(25.0, 25.0, 0.0), 0.0))); SpeakString("Sent from outside");'},
        @{op='run_chunk'; code='this is not nwscript'}
    )
    for ($i=0; $i -lt $commands.Count; $i++) {
        $command = $commands[$i]
        $command.area = $area
        $command.id = "morning-curl-$i"
        $queued = Invoke-Curl 'command' $command
        $completed = Wait-Morning { (Invoke-Curl 'status').results | Where-Object { $_.result.id -eq $queued.queued } | Select-Object -First 1 }
        $result = $completed.result
        $expected = $i -ne ($commands.Count-1)
        if ($result.ok -ne $expected -or (-not $expected -and -not $result.err)) {
            throw "Unexpected $($command.op) result: $($result | ConvertTo-Json -Compress)"
        }
        $evidence.commands += @{op=$command.op; result=$result}
        if ($command.op -eq 'npc_move') {
            $arrived = Wait-Morning {
                (Invoke-Curl "area/$area").creatures | Where-Object {
                    $_.id -eq $npc.id -and [Math]::Sqrt([Math]::Pow($_.x-20,2)+[Math]::Pow($_.y-20,2)) -lt 1
                }
            }
            $evidence.walk = @{from=@{x=$npc.x;y=$npc.y};to=@{x=$arrived.x;y=$arrived.y};arrived=$true}
        }
        if ($command.op -eq 'run_script') {
            $null = Wait-Morning { (Invoke-Curl "area/$area").creatures | Where-Object { $_.id -eq $npc.id -and $_.name -eq 'Probe ran' } }
        }
    }
    $null = Wait-Morning {
        (Invoke-Curl "area/$area").creatures | Where-Object {
            $_.id -eq $npc.id -and $_.name -eq 'Chunk ran' -and [Math]::Sqrt([Math]::Pow($_.x-25,2)+[Math]::Pow($_.y-25,2)) -lt 1
        }
    }
    Save-Morning 'curl-after' (Invoke-Curl "area/$area")
    Save-Morning 'status-curl' (Invoke-Curl 'status')
    $teardown = Invoke-Curl 'command' @{area=$area;id='morning-curl-teardown';op='destroy';target=$npc.id}
    $removed = Wait-Morning { (Invoke-Curl 'status').results | Where-Object { $_.result.id -eq $teardown.queued } | Select-Object -First 1 }
    if (-not $removed.result.ok) { throw 'Could not remove the owned curl acceptance actor' }
    $null = Wait-Morning { -not ((Invoke-Curl "area/$area").creatures | Where-Object id -EQ $npc.id) }
    $evidence.teardown = $removed.result
    $evidence.passed = $true
    Save-Morning 'curl-morning' $evidence
    $descriptor = Join-Path $out "gui-data/Moonglow/live/sessions/$($demo.Gui.Id).json"
    & node (Join-Path $PSScriptRoot 'mcp-client.mjs') (Join-Path $LiveRepo 'target/debug/moonglow-mcp.exe') $LiveMg $morning $descriptor morning $area
    if ($LASTEXITCODE -ne 0) { throw 'MCP Morning acceptance failed; see mcp-morning.json' }
    Save-Morning 'status' (Invoke-Curl 'status')
    if ((Get-FileHash -LiteralPath $env:MGLIVE_DEMO_BASE -Algorithm SHA256).Hash -ne $sourceHash) { throw 'The read-only source module changed during acceptance' }
    $evidence.sourceUnchanged = $true
    Write-Output 'Morning curl and MCP acceptance passed; owner window observation and client join remain manual.'
} catch {
    $failure = $_
    $evidence.passed = $false
    $evidence.error = $failure.Exception.Message
    if ($demo) {
        try {
            Save-Morning 'status-failed' (Invoke-Curl 'status')
            if ($area) { Save-Morning 'curl-failed-area' (Invoke-Curl "area/$area") }
        } catch { $evidence.diagnosticError = $_.Exception.Message }
    }
    throw $failure
} finally {
    Save-Morning 'curl-morning' $evidence
    if ($demo) { & (Join-Path $PSScriptRoot 'Stop-LiveDemo.ps1') -Out $out }
}
