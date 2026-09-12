@echo off
setlocal
cd /d "%~dp0"

echo ==============================================================
echo  Moddin - Windows development setup
echo ==============================================================
echo.

powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\setup-windows.ps1"

set "EXITCODE=%ERRORLEVEL%"
echo.
if not "%EXITCODE%"=="0" (
  echo Setup terminou com erro ^(codigo %EXITCODE%^).
) else (
  echo Setup finalizado.
)
echo.
echo Pressione qualquer tecla para fechar.
pause >nul
exit /b %EXITCODE%
