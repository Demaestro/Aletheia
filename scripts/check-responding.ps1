# check-responding.ps1
# Starts Vite + Aletheia binary, polls Responding for up to 45 s.
$root = "C:\Users\USER\OneDrive\Desktop\worship-production-interface"
$binary = "C:\Users\USER\cargo-targets\worship-production-interface\debug\aletheia-desktop.exe"
$vitePort = "5178"

Set-Location $root

# Kill stale instances
Get-Process aletheia-desktop,node -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1

# Start Vite
$viteLog = "$env:TEMP\vite-aletheia.log"
$nodeExe = (Get-Command node -ErrorAction SilentlyContinue).Source
if (-not $nodeExe) { Write-Host "ERROR: node not found in PATH"; exit 1 }

$viteArgs = @("$root\node_modules\vite\bin\vite.js", "--port", $vitePort)
$viteProc = Start-Process -FilePath $nodeExe -ArgumentList $viteArgs `
    -WorkingDirectory $root -RedirectStandardOutput $viteLog -NoNewWindow -PassThru

Write-Host "Vite PID=$($viteProc.Id) waiting for bundle..."

$viteReady = $false
for ($i = 0; $i -lt 30; $i++) {
    Start-Sleep -Seconds 1
    if (Test-Path $viteLog) {
        $content = Get-Content $viteLog -Raw -ErrorAction SilentlyContinue
        if ($content -match "ready in") { $viteReady = $true; break }
    }
}
if (-not $viteReady) { Write-Host "WARN: Vite ready timeout - launching anyway" }
else { Write-Host "Vite ready." }

# Launch binary
Write-Host "Launching $binary"
$appProc = Start-Process -FilePath $binary -WorkingDirectory $root -PassThru -NoNewWindow

# Poll Responding state
$maxWait = 45
$interval = 3
$elapsed = 0
$becameResponding = $false

while ($elapsed -le $maxWait) {
    Start-Sleep -Seconds $interval
    $elapsed += $interval

    $proc = Get-Process -Id $appProc.Id -ErrorAction SilentlyContinue
    if (-not $proc) {
        Write-Host "+${elapsed}s  process exited"
        break
    }
    $ws = [math]::Round($proc.WorkingSet / 1MB, 1)
    $r  = $proc.Responding
    Write-Host "+${elapsed}s  ws=${ws}MB  responding=$r"

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
Stop-Process -Id $appProc.Id -Force -ErrorAction SilentlyContinue
Stop-Process -Id $viteProc.Id -Force -ErrorAction SilentlyContinue
