# check-window-visible.ps1
# Launches via dev.mjs (Vite warmup + binary) and checks for a visible window.

Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public class Win32 {
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

$root    = "C:\Users\USER\OneDrive\Desktop\worship-production-interface"
$nodeExe = (Get-Command node -ErrorAction SilentlyContinue).Source

# Kill stale instances
Get-Process aletheia-desktop,node -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1

# Launch via dev.mjs (handles Vite + warmup + binary)
Write-Host "Starting dev.mjs launcher..."
$launcher = Start-Process -FilePath $nodeExe -ArgumentList @("scripts/dev.mjs") `
    -WorkingDirectory $root -NoNewWindow -PassThru

# Poll for visible titled window for up to 90 s
$found = $false
for ($i = 0; $i -lt 30; $i++) {
    Start-Sleep -Seconds 3
    $elapsed = ($i + 1) * 3

    $title = [Win32]::FindVisibleWindow("Aletheia")
    $proc = Get-Process aletheia-desktop -ErrorAction SilentlyContinue
    $ws = if ($proc) { "$([math]::Round($proc.WorkingSet/1MB,1))MB" } else { "not running" }

    if ($title) {
        Write-Host "+${elapsed}s  WINDOW VISIBLE: '$title'  ($ws)"
        $found = $true
        break
    } else {
        Write-Host "+${elapsed}s  hidden ($ws)"
    }
}

if ($found) { Write-Host "RESULT: OK - window appeared already loaded" }
else         { Write-Host "RESULT: FAIL - window never appeared in 90s" }

Get-Process aletheia-desktop,node -ErrorAction SilentlyContinue | Stop-Process -Force
Stop-Process -Id $launcher.Id -Force -ErrorAction SilentlyContinue
