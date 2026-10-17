# Offline archive/migration proof. Never launches a server or demo.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'retired-assets.ps1')
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$out = Join-Path $repo 'target/test-output/live-accept6-astra2/retired'
$mg = Join-Path $repo 'target/release/mg.exe'
New-Item -ItemType Directory -Force (Join-Path $out 'module') | Out-Null
function New-TestInfo {
    '{"__data_type":"IFO ","Mod_HakList":{"type":"list","value":[{"__struct_id":8,"Mod_Hak":{"type":"cexostring","value":"campaign"}},{"__struct_id":8,"Mod_Hak":{"type":"cexostring","value":"mglive_colour"}},{"__struct_id":8,"Mod_Hak":{"type":"cexostring","value":"other"}}]}}' | ConvertFrom-Json
}
$info=New-TestInfo
$previous=@{owner='moonglow-live-demo';coloured=@{owner='mglive-coloured-v1'}}
$refused=$false
try { $null=Remove-LiveRetiredHak -Info $info -Previous $null } catch { $refused=$true }
if (-not $refused -or $info.Mod_HakList.value.Count -ne 3) { throw 'Unowned reference must be preserved/refused' }
$refused=$false
try { Assert-LiveDemoNoRetiredHak -Names @('mglive_colour') } catch { $refused=$true }
if (-not $refused) { throw 'Old cached module must be refused before startup' }
if (-not (Remove-LiveRetiredHak -Info $info -Previous $previous)) { throw 'Migration missed' }
if (($info.Mod_HakList.value.Mod_Hak.value -join ',') -ne 'campaign,other') { throw 'Original HAK order changed' }
if (Remove-LiveRetiredHak -Info $info -Previous $previous) { throw 'Migration must be idempotent' }
Assert-LiveDemoNoRetiredHak -Names @('campaign','other')
$json=Join-Path $out 'info.json'
[IO.File]::WriteAllText($json,($info | ConvertTo-Json -Depth 20))
& $mg --no-user-dir gff $json -o (Join-Path $out 'module/module.ifo')
if ($LASTEXITCODE) { throw 'GFF write failed' }
& $mg --no-user-dir pack (Join-Path $out 'module') (Join-Path $out 'retired.mod')
if ($LASTEXITCODE) { throw 'Archive write failed' }
$read=& $mg --json --no-user-dir haks (Join-Path $out 'retired.mod') | ConvertFrom-Json
if ($LASTEXITCODE -or ($read.haks.name -join ',') -ne 'campaign,other') { throw 'Packed module still lists retired HAK' }
$read | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $out 'result.json') -Encoding utf8
'PASS: owned detach, unowned refusal, idempotence, original order, startup refusal, actual packed module readback'
