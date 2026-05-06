@echo off
setlocal
cd /d C:\dev\aletheia
if not exist .logs mkdir .logs
wsl.exe bash -lc "cd /mnt/c/dev/aletheia && npm run vector:serve -- --kb-dir /mnt/c/Users/USER/AppData/Roaming/com.aletheia.production/vector-kb" > .logs\vector-serve.log 2>&1
endlocal
