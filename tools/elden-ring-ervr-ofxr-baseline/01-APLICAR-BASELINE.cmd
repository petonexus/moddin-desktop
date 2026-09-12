@echo off
setlocal
cd /d "%~dp0"
set "GAME_ARGS="
if not "%~1"=="" set GAME_ARGS=-GamePath "%~1"
echo ===============================================================
echo  Moddin - Elden Ring ERVR + VDXR + OFXR - APLICAR BASELINE
echo ===============================================================
echo.
echo O jogo precisa estar fechado.
echo OFXR sera apenas verificado; esta operacao nao o atualiza nem modifica.
echo Extras identificados serao movidos para a quarentena com backup reversivel.
echo.
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\ERVR-OFXR-Baseline.ps1" -Mode Apply %GAME_ARGS%
set "RC=%ERRORLEVEL%"
echo.
pause
exit /b %RC%
