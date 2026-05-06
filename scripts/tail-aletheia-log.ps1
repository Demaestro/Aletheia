$log = Join-Path $env:LOCALAPPDATA "com.aletheia.production\logs\Aletheia.log"

if (-not (Test-Path $log)) {
    Write-Host "Aletheia log not found: $log"
    exit 1
}

Get-Content -Path $log -Tail 80
