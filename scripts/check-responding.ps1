# check-responding.ps1
# Starts the production launcher and verifies that a visible Aletheia window appears.
$root = "C:\dev\aletheia"
$maxWait = 45
$interval = 3

Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public class Win32RespondingProbe {
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr hWnd, StringBuilder sb, int n);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc cb, IntPtr lp);
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lp);

    public static string FindVisibleWindow(string titlePart) {
        string found = null;
        EnumWindows(delegate(IntPtr hWnd, IntPtr lp) {
            if (!IsWindowVisible(hWnd)) return true;
            var sb = new StringBuilder(256);
            GetWindowText(hWnd, sb, 256);
            if (sb.ToString().IndexOf(titlePart, StringComparison.OrdinalIgnoreCase) >= 0) {
                found = sb.ToString();
                return false;
            }
            return true;
        }, IntPtr.Zero);
        return found;
    }
}
"@

Set-Location $root

# Kill stale instances
Get-Process aletheia-desktop,node -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1

$nodeExe = (Get-Command node.exe -ErrorAction SilentlyContinue).Source
if (-not $nodeExe -and (Test-Path "C:\Program Files\nodejs\node.exe")) {
    $nodeExe = "C:\Program Files\nodejs\node.exe"
}
if (-not $nodeExe) { Write-Host "ERROR: node not found in PATH"; exit 1 }

Write-Host "Launching Aletheia via scripts/dev.mjs"
$launcher = Start-Process -FilePath $nodeExe -ArgumentList @("scripts/dev.mjs") `
    -WorkingDirectory $root -PassThru -NoNewWindow

# Poll Responding state
$elapsed = 0
$becameResponding = $false

while ($elapsed -le $maxWait) {
    Start-Sleep -Seconds $interval
    $elapsed += $interval

    $proc = Get-Process aletheia-desktop -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $proc) {
        Write-Host "+${elapsed}s  process not running"
        continue
    }
    $ws = [math]::Round($proc.WorkingSet / 1MB, 1)
    $title = [Win32RespondingProbe]::FindVisibleWindow("Aletheia")
    $r  = [bool]$title
    Write-Host "+${elapsed}s  ws=${ws}MB  windowVisible=$r"

    if ($r -and -not $becameResponding) {
        $becameResponding = $true
        Write-Host ">>> APP IS RESPONDING <<<"
    }
}

if ($becameResponding) {
    Write-Host "RESULT: OK"
} else {
    Write-Host "RESULT: FAIL - never became responsive in ${maxWait}s"
}

# Cleanup
Get-Process aletheia-desktop,node -ErrorAction SilentlyContinue | Stop-Process -Force
if ($launcher) { Stop-Process -Id $launcher.Id -Force -ErrorAction SilentlyContinue }
