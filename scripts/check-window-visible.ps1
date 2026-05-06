# check-window-visible.ps1
# Launches via the supported Tauri dev path and checks for a visible,
# responding Aletheia window.

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

$root    = "C:\dev\aletheia"
$nodeExe = (Get-Command node.exe -ErrorAction SilentlyContinue).Source
if (-not $nodeExe -and (Test-Path "C:\Program Files\nodejs\node.exe")) {
    $nodeExe = "C:\Program Files\nodejs\node.exe"
}
if (-not $nodeExe) {
    Write-Host "RESULT: FAIL - node.exe not found"
    exit 1
}
$nodeDir = Split-Path -Parent $nodeExe
if ($nodeDir -and -not ($env:Path -like "*$nodeDir*")) {
    $env:Path = "$nodeDir;$env:Path"
}
$systemNodeDir = "C:\Program Files\nodejs"
if ((Test-Path $systemNodeDir) -and -not ($env:Path -like "*$systemNodeDir*")) {
    $env:Path = "$systemNodeDir;$env:Path"
}

# Kill stale instances
Get-Process aletheia-desktop,node -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1

# Launch via npm script so this check matches the operator command.
Write-Host "Starting Tauri dev launcher..."
$launcher = Start-Process -FilePath $nodeExe -ArgumentList @(
        ".\node_modules\@tauri-apps\cli\tauri.js",
        "dev",
        "--config",
        "src-tauri/tauri.conf.dev.json"
    ) `
    -WorkingDirectory $root -NoNewWindow -PassThru

# Poll for visible titled window for up to 90 s
$found = $false
for ($i = 0; $i -lt 30; $i++) {
    Start-Sleep -Seconds 3
    $elapsed = ($i + 1) * 3

    $title = [Win32]::FindVisibleWindow("Aletheia")
    $proc = Get-Process aletheia-desktop -ErrorAction SilentlyContinue | Select-Object -First 1
    $ws = if ($proc) { "$([math]::Round($proc.WorkingSet/1MB,1))MB" } else { "not running" }
    $responding = if ($proc) { $proc.Responding } else { $false }

    if ($title -and $responding) {
        Write-Host "+${elapsed}s  WINDOW VISIBLE + RESPONDING: '$title'  ($ws)"
        $found = $true
        break
    } elseif ($title) {
        Write-Host "+${elapsed}s  visible but not responding: '$title'  ($ws)"
    } else {
        Write-Host "+${elapsed}s  hidden ($ws)"
    }
}

if ($found) { Write-Host "RESULT: OK - window is visible and responding" }
else         { Write-Host "RESULT: FAIL - window did not become visible/responding in 90s" }

Get-Process aletheia-desktop,node -ErrorAction SilentlyContinue | Stop-Process -Force
if ($launcher) { Stop-Process -Id $launcher.Id -Force -ErrorAction SilentlyContinue }
