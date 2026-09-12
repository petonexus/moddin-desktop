@echo off
setlocal
cd /d "%~dp0"
echo ===============================================================
echo  Moddin - Cheeky Foveated DLSS - DESINSTALAR
echo ===============================================================
echo.
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\CheekyFoveatedDLSS.ps1" -Action Uninstall
set "RC=%ERRORLEVEL%"
echo.
pause
exit /b %RC%
