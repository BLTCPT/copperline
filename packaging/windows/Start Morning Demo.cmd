@echo off
cd /d "%~dp0"
if not exist "data" mkdir "data"
if errorlevel 1 goto failed
if exist "data\morning-demo" goto open
copperline-cli.exe restore morning-demo.clbak data\morning-demo
if errorlevel 1 goto failed
:open
start "" copperline-desktop.exe --data data\morning-demo --view bookings
goto :eof
:failed
echo The demo could not be restored. Keep any partial folder and restore guard.
echo See docs\backup.md. Do not delete evidence to force the workspace open.
pause
exit /b 1
