@echo off
setlocal
cd /d "%~dp0"

title Moddin Desktop - Dev

echo ==============================================================
echo  Moddin - iniciando modo desenvolvimento
echo ==============================================================
echo.

if not exist "node_modules" (
  echo node_modules nao encontrado. Executando npm install...
  call npm install
  if errorlevel 1 goto :error
)

call npm run tauri dev
if errorlevel 1 goto :error

exit /b 0

:error
echo.
echo Falha ao iniciar o Moddin.
echo Se for a primeira execucao, rode SETUP-WINDOWS.bat.
echo.
pause
exit /b 1
