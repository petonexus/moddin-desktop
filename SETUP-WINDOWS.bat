@echo off
setlocal
cd /d "%~dp0"

title Moddin - Windows Setup

echo ==============================================================
echo  Moddin - Setup completo do ambiente Windows
echo ==============================================================
echo.
echo Este instalador vai verificar/instalar:
echo   - Node.js LTS + npm
echo   - Rustup + Cargo (stable-msvc)
echo   - Visual Studio Build Tools / C++
echo   - dependencias npm
echo   - e depois iniciar o Moddin em modo dev
echo.
echo Pode aparecer uma janela do UAC durante a instalacao do Build Tools.
echo.

powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\setup-windows.ps1"

set "EXITCODE=%ERRORLEVEL%"
echo.
if not "%EXITCODE%"=="0" (
  echo ==============================================================
  echo  Setup terminou com erro ^(codigo %EXITCODE%^)
  echo  Veja o log na pasta logs\
  echo ==============================================================
) else (
  echo ==============================================================
  echo  Setup finalizado.
  echo ==============================================================
)

echo.
echo Pressione qualquer tecla para fechar.
pause >nul
exit /b %EXITCODE%
