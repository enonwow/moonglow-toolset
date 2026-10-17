# End-to-end proof through the actual GUI TLS listener and private WSL NWNX server.
param([switch]$SkipBuild, [switch]$Screens, [string]$Base, [string]$Out, [string]$Distro, [ValidateRange(1024,65535)][int]$RedisPort = 6380)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
if (-not $Out) { $Out = Join-Path $LiveRepo 'target/test-output/live-demo' }
if ($RedisPort -eq 6379) { throw 'The outage test must never use the system Redis port' }
$areaName = $null
$demo = $null
$evidence = [ordered]@{}
function Get-Live([string]$Path) {
    Invoke-RestMethod "$($script:demo.Uri)/$Path" -Headers @{Authorization="Bearer $($script:demo.Token)"} -SkipCertificateCheck -NoProxy -TimeoutSec 5
}
function Send-Live($Command) {
    $Command.area = $script:areaName
    Invoke-RestMethod "$($script:demo.Uri)/command" -Method Post -Body ($Command | ConvertTo-Json -Compress) `
        -ContentType 'application/json' -Headers @{Authorization="Bearer $($script:demo.Token)"} -SkipCertificateCheck -NoProxy -TimeoutSec 5
}
function Wait-Live([scriptblock]$Condition, [int]$Seconds = 25) {
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        $value = & $Condition
        if ($value) { return $value }
        Start-Sleep -Milliseconds 250
    }
    throw "Timed out: $Condition"
}
function Wait-Result([string]$Id, [switch]$Failure) {
    $result = Wait-Live { (Get-Live 'status').results | Where-Object { $_.result.id -eq $Id } | Select-Object -First 1 }
    if (-not $Failure -and -not $result.result.ok) { throw "$Id failed: $($result.result.err)" }
    return $result.result
}
function Save-Area([string]$Name) {
    $a = Get-Live "area/$script:areaName"
    $a | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $script:out "$Name.json") -Encoding utf8NoBOM
    return $a
}
try {
    $script:demo = & (Join-Path $PSScriptRoot 'Start-LiveDemo.ps1') -SkipBuild:$SkipBuild -Quiet -PassThru -Base $Base -Out $Out -Distro $LiveDistro -RedisPort $RedisPort -AllowChunks
    $script:areaName = ($demo.Areas -split ',')[0]
    $status = Wait-Live { $s = Get-Live 'status'; if ($s.connected -and $s.areas.Count -gt 0) { $s } }
    $evidence.firstBoot = $status.boot
    $a = Save-Area 'move-before'
    Start-Sleep -Seconds 2
    $b = Save-Area 'move-after'
    $moved = @($b.creatures | Where-Object {
        $current = $_; $previous = $a.creatures | Where-Object id -eq $current.id | Select-Object -First 1
        $previous -and ([Math]::Abs($current.x-$previous.x)+[Math]::Abs($current.y-$previous.y)) -gt 0.1
    })
    if (-not $moved.Count) {
        Start-Sleep -Seconds 5
        $b = Save-Area 'move-after'
        $moved = @($b.creatures | Where-Object { $current=$_; $previous=$a.creatures | Where-Object id -eq $current.id | Select-Object -First 1; $previous -and ([Math]::Abs($current.x-$previous.x)+[Math]::Abs($current.y-$previous.y)) -gt 0.1 })
    }
    if (-not $moved.Count) { throw 'No routine NPC movement was reported' }
    $evidence.moved = @($moved | Select-Object -First 4 id,tag,x,y)
    Write-Output "Movement: $($moved.Count) NPCs changed position; latest age $($b.ageMs) ms"
    $patrol = $b.creatures | Where-Object { -not $_.pc -and -not $_.dm } | Select-Object -First 1
    if (-not $patrol) { throw 'Demo has no non-player creature' }
    $spawn = @{ id='e2e-spawn'; op='spawn'; kind='creature'; r='nw_bartender'; x=$patrol.x+2; y=$patrol.y; z=$patrol.z; f=90 }
    $null = Send-Live $spawn
    $made = Wait-Result 'e2e-spawn'
    $madeId = $made.obj
    $own = Wait-Live { (Get-Live "area/$script:areaName").creatures | Where-Object { $_.id -eq $madeId -and $_.own } }
    $evidence.spawn = $made
    $null = Send-Live @{id='e2e-say'; op='npc_say'; target=$madeId; text='Moonglow live demo command accepted.'}
    $evidence.say = Wait-Result 'e2e-say'
    $null = Send-Live @{id='e2e-move'; op='npc_move'; target=$madeId; x=$own.x+4; y=$own.y; z=$own.z; run=$true}
    $evidence.move = Wait-Result 'e2e-move'
    $null = Wait-Live { (Get-Live "area/$script:areaName").creatures | Where-Object { $_.id -eq $madeId -and ([Math]::Abs($_.x-$own.x)+[Math]::Abs($_.y-$own.y)) -gt 0.5 } }
    $door = $b.doors | Select-Object -First 1
    if (-not $door) { throw 'Demo has no door' }
    $null = Send-Live @{id='e2e-door'; op='set_state'; target=$door.id; open=$true; lock=$false}
    $evidence.door = Wait-Result 'e2e-door'
    $null = Wait-Live { (Get-Live "area/$script:areaName").doors | Where-Object { $_.id -eq $door.id -and $_.open -and -not $_.lock } }
    $null = Save-Area 'spawn-door'
    Write-Output 'Spawn, own flag, say, move and door open state passed'

    $null = Send-Live @{id='e2e-chest'; op='spawn'; kind='placeable'; r='plc_chest1'; x=$own.x+3; y=$own.y+3; z=$own.z; f=0}
    $chest = Wait-Result 'e2e-chest'
    $null = Wait-Live { (Get-Live "area/$script:areaName").placeables | Where-Object id -eq $chest.obj }
    $null = Send-Live @{id='e2e-chest-state'; op='set_state'; target=$chest.obj; open=$true; lock=$true}
    $evidence.chestState = Wait-Result 'e2e-chest-state'
    $null = Wait-Live { (Get-Live "area/$script:areaName").placeables | Where-Object { $_.id -eq $chest.obj -and $_.open -and $_.lock } }
    $null = Send-Live @{id='e2e-item'; op='spawn'; kind='item'; r='nw_it_gold001'; x=$own.x+2; y=$own.y+3; z=$own.z; f=0}
    $item = Wait-Result 'e2e-item'
    $null = Wait-Live { (Get-Live "area/$script:areaName").items | Where-Object { $_.id -eq $item.obj -and $_.own } }
    foreach ($madeObject in @($chest,$item)) {
        $id = "e2e-remove-$($madeObject.id)"
        $null = Send-Live @{id=$id; op='destroy'; target=$madeObject.obj}
        $null = Wait-Result $id
    }
    $null = Send-Live @{id='e2e-victim'; op='spawn'; kind='creature'; r='nw_bartender'; x=$own.x+6; y=$own.y+1; z=$own.z; f=0}
    $victim = Wait-Result 'e2e-victim'
    $null = Wait-Live { (Get-Live "area/$script:areaName").creatures | Where-Object id -eq $victim.obj }
    $null = Send-Live @{id='e2e-attack'; op='npc_attack'; target=$madeId; victim=$victim.obj}
    $evidence.attack = Wait-Result 'e2e-attack'
    $null = Send-Live @{id='e2e-remove-victim'; op='destroy'; target=$victim.obj}
    $null = Wait-Result 'e2e-remove-victim'
    $evidence.item = $item
    $evidence.chest = $chest
    Write-Output 'Placeable/item spawn, placeable open/lock, attack dispatch and cleanup passed'

    # Both script operations are proved from returned state, never from server logs.
    $null = Send-Live @{id='e2e-script'; op='run_script'; target=$madeId; script='mglive_probe'}
    $evidence.script = Wait-Result 'e2e-script'
    $null = Wait-Live { (Get-Live "area/$script:areaName").creatures | Where-Object { $_.id -eq $madeId -and $_.name -eq 'Probe ran' } }
    $null = Send-Live @{id='e2e-chunk'; op='run_chunk'; target=$patrol.id; code='SetName(OBJECT_SELF, "Chunk ran");'}
    $evidence.chunk = Wait-Result 'e2e-chunk'
    $null = Wait-Live { (Get-Live "area/$script:areaName").creatures | Where-Object { $_.id -eq $patrol.id -and $_.name -eq 'Chunk ran' } }
    $null = Send-Live @{id='e2e-bad-chunk'; op='run_chunk'; target=$madeId; code='this is not nwscript'}
    $evidence.compileError = Wait-Result 'e2e-bad-chunk' -Failure
    if ($evidence.compileError.ok -or -not $evidence.compileError.err) { throw 'Invalid chunk did not return a compiler diagnostic' }
    $null = Send-Live @{id='e2e-unknown-script'; op='run_script'; target=$madeId; script='mglive_missing'}
    $evidence.unknownScript = Wait-Result 'e2e-unknown-script' -Failure
    if ($evidence.unknownScript.ok -or $evidence.unknownScript.err -ne 'unknown script') { throw 'Missing script was not rejected' }
    Write-Output 'Module script, chunk, compilation diagnostic and missing script passed'

    # Listener restart loses its in-memory retry ids; the game must still suppress a duplicate.
    & (Join-Path $PSScriptRoot 'Stop-LiveDemo.ps1') -GuiOnly -Out $Out -Distro $LiveDistro
    Start-Sleep -Seconds 10
    $script:demo = & (Join-Path $PSScriptRoot 'Start-LiveDemo.ps1') -SkipBuild -GuiOnly -Quiet -PassThru -Token $demo.Token -Out $Out -Distro $LiveDistro -RedisPort $RedisPort -AllowChunks
    $null = Wait-Live { $s=Get-Live 'status'; if ($s.connected -and $s.areas.Count -gt 0) { $s } }
    $null = Send-Live $spawn
    $null = Wait-Live { $s=Get-Live 'status'; if (@($s.queued | Where-Object id -eq 'e2e-spawn').Count -eq 0) { $true } }
    $ownAfter = @((Get-Live "area/$script:areaName").creatures | Where-Object own)
    if ($ownAfter.Count -ne 1 -or $ownAfter[0].id -ne $madeId) { throw 'Reconnect duplicated or lost the spawned object' }
    $evidence.reconnect = @{ outageSeconds=10; ownCount=$ownAfter.Count; sameObject=$true; duplicateAcknowledged=$true }
    $null = Send-Live @{id='e2e-destroy'; op='destroy'; target=$madeId}
    $evidence.destroy = Wait-Result 'e2e-destroy'
    $null = Wait-Live { if (@((Get-Live "area/$script:areaName").creatures | Where-Object id -eq $madeId).Count -eq 0) { $true } }
    Write-Output '10-second GUI outage, reconnect, duplicate suppression and destroy passed'

    # Owner decision 2026-10-12: Moonglow reconnects; this NWNX build needs a server restart.
    $beforeOutage = Get-Live 'status'
    if (-not $beforeOutage.redis.connected) { throw 'Moonglow Redis link is not connected before outage' }
    $null = Invoke-LiveHelper 'stop-redis' @("$RedisPort")
    $null = Wait-Live { if (-not (Get-Live 'status').redis.connected) { $true } }
    Start-Sleep -Seconds 10
    $stale = Get-Live 'status'
    if ($stale.ageMs -lt 8000) { throw 'Redis did not stop: report is not stale' }
    $null = Send-Live @{id='e2e-outage-say'; op='npc_say'; target=$patrol.id; text='Must expire after the required server restart.'}
    $null = Invoke-LiveHelper 'start-redis' @("$RedisPort")
    Test-LiveRedis -Port $RedisPort
    $reconnected = Wait-Live { $s=Get-Live 'status'; if ($s.redis.connected) { $s } }
    # Replay of a completed command must not create a second queue entry or result.
    # These are API dedup checks; stale area data is not presented as fresh game evidence.
    $null = Send-Live $spawn
    $dedup = Get-Live 'status'
    if (@($dedup.queued | Where-Object id -eq 'e2e-spawn').Count -ne 0 -or
        @($dedup.results | Where-Object { $_.result.id -eq 'e2e-spawn' }).Count -ne 1 -or
        @($dedup.queued | Where-Object id -eq 'e2e-outage-say').Count -ne 1) { throw 'Outage replay duplicated a command or lost the pending command' }
    $evidence.redisOutage = @{ seconds=10; staleAgeMs=$stale.ageMs; moonglowReconnected=$reconnected.redis.connected; duplicateQueued=$false; gameRestartRequired=$true }
    $null = Invoke-LiveHelper 'stop-server'
    $oldBoot = $beforeOutage.boot
    $null = Send-Live @{id='e2e-expire'; op='npc_say'; target=$patrol.id; text='Must expire on server restart.'}
    $server = Start-LiveServerProcess -Module (Join-Path $out 'DEMO-Live-01.mod') -Areas $areaName -Out $Out -RedisPort $RedisPort
    $restarted = Wait-Live { $s=Get-Live 'status'; if ($s.boot -ne $oldBoot -and $s.ageMs -lt 3000 -and $s.areas.Count -gt 0) { $s } }
    foreach ($id in @('e2e-expire','e2e-outage-say')) {
        $expired = $restarted.results | Where-Object { $_.result.id -eq $id } | Select-Object -First 1
        if ($expired.result.ok -or $expired.result.err -ne 'expired: the server restarted') { throw "Old command $id did not expire on new boot" }
        $evidence[$id] = $expired.result
    }
    $evidence.restart = @{ oldBoot=$oldBoot; newBoot=$restarted.boot; ageMs=$restarted.ageMs; oldCommandsExpired=$true }
    Write-Output 'Moonglow Redis reconnect, API deduplication, required server restart and old-command expiry passed'
    # This restart intentionally leaves the game-side chunk switch off.
    $targetAfterRestart = (Get-Live "area/$script:areaName").creatures | Where-Object { -not $_.pc -and -not $_.dm } | Select-Object -First 1
    $null = Send-Live @{id='e2e-server-gate'; op='run_chunk'; target=$targetAfterRestart.id; code='SetName(OBJECT_SELF, "Must not run");'}
    $evidence.serverChunkGate = Wait-Result 'e2e-server-gate' -Failure
    if ($evidence.serverChunkGate.ok -or $evidence.serverChunkGate.err -ne 'chunks are disabled on this server') { throw 'Server chunk gate failed' }
    & (Join-Path $PSScriptRoot 'Stop-LiveDemo.ps1') -GuiOnly -Out $Out -Distro $LiveDistro
    $script:demo = & (Join-Path $PSScriptRoot 'Start-LiveDemo.ps1') -SkipBuild -GuiOnly -Quiet -PassThru -Token $demo.Token -Out $Out -Distro $LiveDistro -RedisPort $RedisPort
    $null = Wait-Live { if ((Get-Live 'status').areas.Count -gt 0) { $true } }
    $blocked = $false
    try { $null = Send-Live @{id='e2e-options-gate'; op='run_chunk'; target=$targetAfterRestart.id; code='SetName(OBJECT_SELF, "Must not run");'} }
    catch { if ([int]$_.Exception.Response.StatusCode -ne 400) { throw }; $blocked = $true }
    if (-not $blocked) { throw 'Moonglow chunk gate did not return HTTP 400' }
    $evidence.optionsChunkGate = @{ status=400 }
    Write-Output 'Both independent chunk gates passed'
    $evidence.passed = $true
    $evidence | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $out 'e2e-redis.json') -Encoding utf8
    if ($Screens) {
        & (Join-Path $PSScriptRoot 'check.ps1') -Phase E -Picture live_demo_snapshot
        Write-Output 'GPU proof rendered from the real WSL reports'
    }
} catch {
    $evidence.passed = $false
    $evidence.error = $_.Exception.Message
    $evidence | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $out 'e2e-redis.json') -Encoding utf8
    throw
} finally {
    if ($demo) { & (Join-Path $PSScriptRoot 'Stop-LiveDemo.ps1') -Out $Out -Distro $LiveDistro }
    $null = Invoke-LiveHelper 'stop-redis' @("$RedisPort")
    $null = Invoke-LiveHelper 'stop-keeper'
}
