#requires -Version 7.0
param(
    [Parameter(Mandatory)][int]$ProcessId,
    [Parameter(Mandatory)][string]$ExpectedPath,
    [Parameter(Mandatory)][string]$Out,
    [ValidateRange(1,300)][int]$Seconds = 30,
    [switch]$GpuMemory
)
# Read-only process CPU/private/working-set and committed virtual-address categories.
$ErrorActionPreference = 'Stop'
$process = Get-Process -Id $ProcessId
if ($process.Path -ne [IO.Path]::GetFullPath($ExpectedPath)) { throw 'Process identity does not match' }
$started = $process.StartTime
Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class GuiMemoryMap {
    [StructLayout(LayoutKind.Sequential)] struct Region {
        public IntPtr Base, AllocationBase; public uint AllocationProtect;
        public ushort PartitionId; public UIntPtr Size; public uint State, Protect, Type;
    }
    [DllImport("kernel32.dll")] static extern IntPtr OpenProcess(uint rights, bool inherit, int id);
    [DllImport("kernel32.dll")] static extern UIntPtr VirtualQueryEx(IntPtr process, IntPtr address, out Region info, UIntPtr size);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    public static Dictionary<string, ulong> Read(int id) {
        var result = new Dictionary<string, ulong>();
        var process = OpenProcess(0x0400, false, id);
        if (process == IntPtr.Zero) throw new Exception("OpenProcess query denied");
        try {
            ulong address = 0;
            while (VirtualQueryEx(process, new IntPtr((long)address), out var r, (UIntPtr)Marshal.SizeOf<Region>()) != UIntPtr.Zero) {
                ulong size = r.Size.ToUInt64();
                string state = r.State == 0x1000 ? "committed" : r.State == 0x2000 ? "reserved" : "free";
                string kind = r.Type == 0x20000 ? "private" : r.Type == 0x40000 ? "mapped" : r.Type == 0x1000000 ? "image" : "none";
                if (state != "free") {
                    string key = state + "/" + kind + "/protect_" + r.Protect.ToString("x");
                    result[key] = result.TryGetValue(key, out var n) ? n + size : size;
                }
                ulong next = (ulong)r.Base.ToInt64() + size;
                if (next <= address || next > long.MaxValue) break;
                address = next;
            }
        } finally { CloseHandle(process); }
        return result;
    }
}
'@
$initial = $process.TotalProcessorTime.TotalSeconds
$watch = [Diagnostics.Stopwatch]::StartNew()
$samples = @()
for ($n = 0; $n -lt $Seconds; $n++) {
    Start-Sleep -Seconds 1
    $process.Refresh()
    if ($process.HasExited -or $process.StartTime -ne $started) { throw 'Process exited or identity changed' }
    $samples += [ordered]@{ elapsed_s=$watch.Elapsed.TotalSeconds; cpu_s=$process.TotalProcessorTime.TotalSeconds-$initial; working_set_bytes=$process.WorkingSet64; private_bytes=$process.PrivateMemorySize64 }
}
$map = [GuiMemoryMap]::Read($ProcessId)
$report = [ordered]@{
    pid=$ProcessId; path=$process.Path; start=$started.ToUniversalTime().ToString('o')
    method='100 * process CPU seconds / elapsed seconds; 100% is one logical core. VirtualQueryEx classifies committed virtual pages, not ownership or physical RAM.'
    logical_processors=[Environment]::ProcessorCount
    cpu_one_core_percent=100*$samples[-1].cpu_s/$samples[-1].elapsed_s
    samples=$samples; virtual_bytes=$map
}
if ($GpuMemory) {
    $report.gpu_memory = @(Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPUProcessMemory |
        Where-Object Name -Like "pid_$($ProcessId)_*" | Select-Object Name,DedicatedUsage,SharedUsage,TotalCommitted)
}
$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $Out -Encoding utf8
[pscustomobject]$report | Select-Object pid,cpu_one_core_percent
