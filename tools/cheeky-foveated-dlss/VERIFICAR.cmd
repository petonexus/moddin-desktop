@echo off
setlocal
cd /d "%~dp0"
echo ===============================================================
echo  Moddin - Cheeky Foveated DLSS - VERIFICAR
echo ===============================================================
echo.
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\CheekyFoveatedDLSS.ps1" -Action Scan
set "RC=%ERRORLEVEL%"
echo.
pause
exit /b %RC%
