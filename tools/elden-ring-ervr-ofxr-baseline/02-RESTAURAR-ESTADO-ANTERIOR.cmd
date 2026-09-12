@echo off
setlocal
cd /d "%~dp0"
set "GAME_ARGS="
if not "%~1"=="" set GAME_ARGS=-GamePath "%~1"
echo ===============================================================
echo  Moddin - Elden Ring ERVR + VDXR + OFXR - RESTAURAR ESTADO
echo ===============================================================
echo.
echo O jogo precisa estar fechado.
echo Arquivos da quarentena serao devolvidos somente se o destino original estiver livre.
echo.
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\ERVR-OFXR-Baseline.ps1" -Mode Restore %GAME_ARGS%
set "RC=%ERRORLEVEL%"
echo.
pause
exit /b %RC%
