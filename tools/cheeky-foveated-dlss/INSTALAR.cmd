@echo off
setlocal
cd /d "%~dp0"
echo ===============================================================
echo  Moddin - Cheeky Foveated DLSS - INSTALAR (CONSERVADOR)
echo ===============================================================
echo.
echo Instala apenas alvos classificados como READY.
echo Casos experimentais ficam para INSTALAR-EXPERIMENTAIS.cmd.
echo Backups sao feitos antes de qualquer alteracao.
echo.
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\CheekyFoveatedDLSS.ps1" -Action Install
set "RC=%ERRORLEVEL%"
echo.
pause
exit /b %RC%
