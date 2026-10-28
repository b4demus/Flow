@echo off
setlocal DisableDelayedExpansion
rem All file operations and elevation stay in PowerShell.
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1" %*
set "FLOW_EXIT=%ERRORLEVEL%"
if not "%FLOW_EXIT%"=="0" echo Installation did not complete. See the message above.
pause
exit /b %FLOW_EXIT%
