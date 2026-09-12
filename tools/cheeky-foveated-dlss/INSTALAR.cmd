@echo off
setlocal
cd /d "%~dp0"
echo ===============================================================
echo  Moddin - Cheeky Foveated DLSS - INSTALAR
echo ===============================================================
echo.
echo Este launcher permite candidatos EXPERIMENTAIS somente quando todos
echo os pre-requisitos detectaveis passaram. Backups sao feitos antes.
echo.
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\CheekyFoveatedDLSS.ps1" -Action Install -IncludeExperimental
set "RC=%ERRORLEVEL%"
echo.
pause
exit /b %RC%
