# Offline migration only. The removed feature cannot be enabled or generated.
function Remove-LiveRetiredHak {
    param($Info, $Previous)
    $entries = @($Info.Mod_HakList.value)
    if (-not ($entries | Where-Object { $_.Mod_Hak.value -eq 'mglive_colour' })) { return $false }
    if ($Previous.owner -ne 'moonglow-live-demo' -or $Previous.coloured.owner -ne 'mglive-coloured-v1') {
        throw 'Retired generated HAK has no ownership provenance; refusing to edit this module.'
    }
    $Info.Mod_HakList.value = @($entries | Where-Object { $_.Mod_Hak.value -ne 'mglive_colour' })
    return $true
}

function Assert-LiveDemoNoRetiredHak {
    param([string[]]$Names)
    if ($Names -contains 'mglive_colour') {
        throw 'Cached demo still lists a retired generated HAK. Prepare/build the owned scratch module offline before -SkipBuild. Never rebuild a running demo.'
    }
}
