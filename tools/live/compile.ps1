# Compatibility entry point: all generic preparation/compilation is mg live-build.
param([switch]$CaptureDialogue, [string]$Base, [string]$Distro, [string]$Out)
& (Join-Path $PSScriptRoot 'demo/build.ps1') -Base $Base -Distro $Distro -Out $Out -CaptureDialogue:$CaptureDialogue
