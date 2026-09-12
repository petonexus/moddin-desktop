#requires -version 5.1
$ErrorActionPreference = 'Stop'

function Has-Command([string]$Name) {
    return [bool](Get-Command $Name -ErrorAction SilentlyContinue)
}

Write-Host ''
Write-Host 'Moddin - Windows development setup' -ForegroundColor Cyan
Write-Host 'This installs/checks the native prerequisites required by Tauri.' -ForegroundColor Gray
Write-Host ''

if (-not (Has-Command 'winget')) {
    throw 'winget was not found. Install/update App Installer from Microsoft Store and run this script again.'
}

if (-not (Has-Command 'rustup')) {
    Write-Host '[1/3] Installing Rustup...' -ForegroundColor Yellow
    winget install --id Rustlang.Rustup --exact --accept-package-agreements --accept-source-agreements
    Write-Host ''
    Write-Host 'Rustup was installed. Close this terminal, open a new one, and run SETUP-WINDOWS.bat again.' -ForegroundColor Green
    exit 0
}

Write-Host '[1/3] Rustup found.' -ForegroundColor Green
& rustup default stable-msvc

if (-not (Has-Command 'cargo')) {
    Write-Host 'Cargo is not available in the current PATH yet.' -ForegroundColor Yellow
    Write-Host 'Close this terminal, open a new one, and run SETUP-WINDOWS.bat again.' -ForegroundColor Yellow
    exit 0
}

Write-Host '[2/3] Cargo found.' -ForegroundColor Green
& cargo --version

# Tauri requires the MSVC C++ toolchain. vswhere is a reliable signal that
# Visual Studio / Build Tools is installed. The script does not silently install
# several GB of Build Tools; it opens the official installer page when missing.
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path $vswhere)) {
    Write-Host ''
    Write-Host '[3/3] Microsoft C++ Build Tools were not detected.' -ForegroundColor Yellow
    Write-Host 'Install Visual Studio Build Tools 2022 and select:' -ForegroundColor White
    Write-Host '  Desktop development with C++' -ForegroundColor Cyan
    Write-Host 'Include the recommended MSVC toolset and Windows SDK.' -ForegroundColor Gray
    Write-Host ''
    Start-Process 'https://visualstudio.microsoft.com/visual-cpp-build-tools/'
    Write-Host 'After installation, restart the terminal and run SETUP-WINDOWS.bat again.' -ForegroundColor Yellow
    exit 0
}

Write-Host '[3/3] Visual Studio / C++ Build Tools detected.' -ForegroundColor Green
Write-Host ''
Write-Host 'Native prerequisites look ready.' -ForegroundColor Green
Write-Host 'Next:' -ForegroundColor White
Write-Host '  npm install'
Write-Host '  npm run tauri dev'
Write-Host ''
