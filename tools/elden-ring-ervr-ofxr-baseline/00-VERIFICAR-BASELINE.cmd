@echo off
setlocal
cd /d "%~dp0"
set "GAME_ARGS="
if not "%~1"=="" set GAME_ARGS=-GamePath "%~1"
echo ===============================================================
echo  Moddin - Elden Ring ERVR + VDXR + OFXR - VERIFICAR BASELINE
echo ===============================================================
echo.
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\ERVR-OFXR-Baseline.ps1" -Mode Verify %GAME_ARGS%
set "RC=%ERRORLEVEL%"
echo.
pause
exit /b %RC%
