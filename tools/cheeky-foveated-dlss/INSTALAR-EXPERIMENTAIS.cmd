@echo off
setlocal
cd /d "%~dp0"
echo =====================================================================
echo  Moddin - Cheeky Foveated DLSS - INSTALAR EXPERIMENTAIS
echo =====================================================================
echo.
echo Este modo tambem instala alvos EXPERIMENTAL_READY quando todos os
 echo pre-requisitos detectaveis passaram. Use primeiro VERIFICAR.cmd.
echo Backups sao feitos antes de qualquer alteracao.
echo.
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\CheekyFoveatedDLSS.ps1" -Action Install -IncludeExperimental
set "RC=%ERRORLEVEL%"
echo.
pause
exit /b %RC%
