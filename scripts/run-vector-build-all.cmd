@echo off
setlocal
cd /d C:\dev\aletheia
if not exist .logs mkdir .logs
set "PATH=C:\Program Files\nodejs;%PATH%"
"C:\Program Files\nodejs\npm.cmd" run vector:build -- --force --translations kjv web esv niv nkjv nlt amp bbe csb gnb tlb --max-entity-docs 4000 > .logs\vector-build-all.log 2>&1
endlocal
