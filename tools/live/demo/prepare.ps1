# Demo-only dependency staging; generic module preparation is mg live-build.
param([string]$Base, [string]$Distro, [string]$Out, [string]$HakSource, [string]$NwnxZip)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../common.ps1')
. (Join-Path $PSScriptRoot 'retired-assets.ps1')
if (-not $Base) { $Base = $env:MGLIVE_DEMO_BASE }
if (-not $Base) { throw 'Provide -Base or MGLIVE_DEMO_BASE (read-only source module)' }
if (-not $HakSource) { $HakSource = $env:MGLIVE_HAK_SOURCE }
if (-not $NwnxZip) {
    if (-not $env:NWNX_ROOT) { throw 'Set NWNX_ROOT or provide -NwnxZip' }
    $NwnxZip = "$env:NWNX_ROOT/Binaries/NWScript.zip"
}
if (-not $Out) { $Out = Join-Path $LiveRepo 'target/test-output/live-demo' }
$Out = [IO.Path]::GetFullPath($Out)
$user = Join-Path $Out 'user'
New-Item -ItemType Directory -Force $Out, $user, (Join-Path $user 'hak') | Out-Null
$manifestPath = Join-Path $Out 'inputs.json'
$baseHash = (Get-FileHash -LiteralPath $Base -Algorithm SHA256).Hash
if (Test-Path -LiteralPath $manifestPath) {
    $previous = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    if ($previous.owner -ne 'moonglow-live-demo' -or $previous.baseSha256 -ne $baseHash) {
        throw 'Different or unowned base: choose a separate -Out instead of reusing stale inputs'
    }
}
$info = Invoke-LiveMg -UserDirectory $user -Command @('haks', $Base)
$names = @($info.haks | ForEach-Object { $_.name })
Assert-LiveDemoNoRetiredHak -Names $names
$haks = @(foreach ($name in $names) {
    if ($name -notmatch '^[A-Za-z0-9_-]+$') { throw "Unexpected HAK name: $name" }
    $source = Join-Path $HakSource "$name.hak"
    $dest = Join-Path $user "hak/$name.hak"
    $hash = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash
    if (-not (Test-Path -LiteralPath $dest) -or (Get-FileHash -LiteralPath $dest -Algorithm SHA256).Hash -ne $hash) { Copy-Item -LiteralPath $source -Destination $dest }
    @{ source=$source; path=$dest; sha256=$hash; bytes=(Get-Item -LiteralPath $dest).Length }
})
# Windows cannot follow an absolute Linux symlink exposed through the WSL UNC provider.
# Resolve the read-only include archive in WSL before passing its directory to Rust.
$realZip = ((Invoke-LiveWsl @('realpath', '--', $NwnxZip)) -join '').Trim()
$nwnxLinux = $realZip -replace '/Binaries/NWScript.zip$', ''
$nwnxWindows = ((Invoke-LiveWsl @('wslpath', '-w', $nwnxLinux)) -join '').Trim()
$module = [IO.Path]::GetFullPath($Base)
# Only the owner's optional, hash-bound demo repair needs a loose working copy.
if ($env:MGLIVE_BASE_REPAIR) {
    $module = Join-Path $Out 'live-build-input'
    if (-not (Test-Path -LiteralPath $module)) { $null = Invoke-LiveMg -UserDirectory $user -Command @('unpack', $Base, $module) }
    elseif (-not $previous) { throw 'Unowned repair input folder' }
}
$manifest = @{ owner='moonglow-live-demo'; base=[IO.Path]::GetFullPath($Base); baseSha256=$baseHash; haks=$haks; nwNx=@{source=$realZip}; builder='mg live-build' }
$manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifestPath -Encoding utf8
[pscustomobject]@{ Out=$Out; Module=$module; User=$user; Inputs=$manifest; Nwnx=$nwnxWindows }
