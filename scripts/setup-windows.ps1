#requires -version 5.1
<#
.SYNOPSIS
  Prepara automaticamente o Windows para desenvolver/rodar o Moddin com Tauri.

.DESCRIPTION
  O script:
    - valida winget;
    - instala Node.js LTS se necessario;
    - instala Rustup se necessario;
    - configura o toolchain stable-msvc;
    - instala Visual Studio Build Tools 2022 com Desktop development with C++;
    - atualiza o PATH da sessao atual;
    - executa npm install;
    - inicia npm run tauri dev.

  Pode ser executado varias vezes. As etapas ja instaladas sao ignoradas.

.PARAMETER SkipNpmInstall
  Nao executa npm install.

.PARAMETER SkipRun
  Faz apenas o setup e nao inicia o Tauri ao final.
#>

[CmdletBinding()]
param(
    [switch]$SkipNpmInstall,
    [switch]$SkipRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$RepoRoot = Split-Path -Parent $PSScriptRoot
$LogDir = Join-Path $RepoRoot 'logs'
New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
$LogFile = Join-Path $LogDir ("setup-windows-{0}.log" -f (Get-Date -Format 'yyyyMMdd-HHmmss'))

$script:TranscriptStarted = $false

function Start-SetupTranscript {
    try {
        Start-Transcript -Path $LogFile -Force | Out-Null
        $script:TranscriptStarted = $true
    } catch {
        Write-Host "[!] Nao consegui iniciar transcript: $($_.Exception.Message)" -ForegroundColor Yellow
    }
}

function Stop-SetupTranscript {
    if ($script:TranscriptStarted) {
        try { Stop-Transcript | Out-Null } catch {}
    }
}

function Write-Section([string]$Text) {
    Write-Host ''
    Write-Host ('=' * 72) -ForegroundColor DarkGray
    Write-Host $Text -ForegroundColor Cyan
    Write-Host ('=' * 72) -ForegroundColor DarkGray
}

function Write-Ok([string]$Text) {
    Write-Host "[OK] $Text" -ForegroundColor Green
}

function Write-Info([string]$Text) {
    Write-Host "[i]  $Text" -ForegroundColor Gray
}

function Write-Warn([string]$Text) {
    Write-Host "[!]  $Text" -ForegroundColor Yellow
}

function Has-Command([string]$Name) {
    return [bool](Get-Command $Name -ErrorAction SilentlyContinue)
}

function Refresh-ProcessPath {
    $machinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')

    $parts = @()
    if ($machinePath) { $parts += $machinePath }
    if ($userPath) { $parts += $userPath }

    $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
    if (Test-Path -LiteralPath $cargoBin) {
        $parts += $cargoBin
    }

    $nodePath = Join-Path $env:ProgramFiles 'nodejs'
    if (Test-Path -LiteralPath $nodePath) {
        $parts += $nodePath
    }

    $env:Path = ($parts -join ';')
}

function Assert-LastExitCode([string]$What) {
    if ($LASTEXITCODE -ne 0) {
        throw "$What falhou com codigo $LASTEXITCODE."
    }
}

function Invoke-WingetInstall {
    param(
        [Parameter(Mandatory)][string]$Id,
        [string]$Override
    )

    $args = @(
        'install',
        '--id', $Id,
        '--exact',
        '--accept-package-agreements',
        '--accept-source-agreements'
    )

    if ($Override) {
        $args += @('--override', $Override)
    }

    Write-Info "winget $($args -join ' ')"
    & winget @args

    if ($LASTEXITCODE -ne 0) {
        throw "winget nao conseguiu instalar '$Id' (codigo $LASTEXITCODE)."
    }
}

function Get-VsWherePath {
    $candidate = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path -LiteralPath $candidate) {
        return $candidate
    }
    return $null
}

function Test-VCTools {
    $vswhere = Get-VsWherePath
    if (-not $vswhere) {
        return $false
    }

    $installation = & $vswhere `
        -latest `
        -products '*' `
        -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
        -property installationPath 2>$null

    return -not [string]::IsNullOrWhiteSpace(($installation | Out-String).Trim())
}

function Show-Version([string]$Command, [string[]]$Arguments = @('--version')) {
    try {
        & $Command @Arguments
    } catch {
        Write-Warn "Nao consegui consultar versao de $Command."
    }
}

Start-SetupTranscript

try {
    Set-Location -LiteralPath $RepoRoot

    Write-Host ''
    Write-Host 'Moddin - Windows development bootstrap' -ForegroundColor Cyan
    Write-Host "Repositorio: $RepoRoot" -ForegroundColor DarkGray
    Write-Host "Log: $LogFile" -ForegroundColor DarkGray

    Write-Section '1/6 - Windows Package Manager'

    if (-not (Has-Command 'winget')) {
        throw @"
winget nao foi encontrado.

Instale/atualize o 'App Installer' da Microsoft Store e execute
SETUP-WINDOWS.bat novamente.
"@
    }

    Write-Ok 'winget encontrado.'

    Write-Section '2/6 - Node.js / npm'

    if (-not (Has-Command 'node') -or -not (Has-Command 'npm')) {
        Write-Info 'Node.js LTS nao encontrado. Instalando...'
        Invoke-WingetInstall -Id 'OpenJS.NodeJS.LTS'
        Refresh-ProcessPath
    }

    if (-not (Has-Command 'node') -or -not (Has-Command 'npm')) {
        throw 'Node/npm foram instalados, mas ainda nao apareceram no PATH desta sessao.'
    }

    Write-Ok 'Node.js e npm encontrados.'
    Show-Version 'node'
    Show-Version 'npm'

    Write-Section '3/6 - Rust / Cargo'

    if (-not (Has-Command 'rustup')) {
        Write-Info 'Rustup nao encontrado. Instalando...'
        Invoke-WingetInstall -Id 'Rustlang.Rustup'
        Refresh-ProcessPath
    }

    if (-not (Has-Command 'rustup')) {
        $rustupCandidate = Join-Path $env:USERPROFILE '.cargo\bin\rustup.exe'
        if (Test-Path -LiteralPath $rustupCandidate) {
            $env:Path = "$(Split-Path $rustupCandidate -Parent);$env:Path"
        }
    }

    if (-not (Has-Command 'rustup')) {
        throw 'Rustup foi instalado, mas o executavel ainda nao esta acessivel.'
    }

    Write-Info 'Configurando Rust stable-msvc...'
    & rustup default stable-msvc
    Assert-LastExitCode 'rustup default stable-msvc'

    Refresh-ProcessPath

    if (-not (Has-Command 'cargo')) {
        throw 'Cargo nao ficou disponivel depois da configuracao do Rust.'
    }

    Write-Ok 'Rust/Cargo prontos.'
    Show-Version 'rustc'
    Show-Version 'cargo'

    Write-Section '4/6 - Microsoft C++ Build Tools'

    if (Test-VCTools) {
        Write-Ok 'Visual Studio C++ Build Tools ja estao instalados.'
    } else {
        Write-Info 'C++ Build Tools nao encontrados.'
        Write-Info 'Instalando Visual Studio Build Tools 2022 + workload VCTools...'
        Write-Warn 'O instalador pode pedir permissao do Windows e baixar alguns GB.'

        $vsOverride = '--wait --passive --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended'
        Invoke-WingetInstall `
            -Id 'Microsoft.VisualStudio.2022.BuildTools' `
            -Override $vsOverride

        Refresh-ProcessPath

        if (-not (Test-VCTools)) {
            throw @"
O Visual Studio Build Tools terminou a instalacao, mas o workload C++ nao foi detectado.

Abra 'Visual Studio Installer', escolha 'Build Tools 2022' > Modificar
e confirme 'Desktop development with C++'. Depois execute este setup novamente.
"@
        }

        Write-Ok 'Visual Studio C++ Build Tools instalados.'
    }

    Write-Section '5/6 - Dependencias do Moddin'

    if (-not (Test-Path -LiteralPath (Join-Path $RepoRoot 'package.json'))) {
        throw "package.json nao encontrado em '$RepoRoot'. Rode este setup a partir da raiz do repositorio Moddin."
    }

    if (-not $SkipNpmInstall) {
        Write-Info 'Executando npm install...'
        & npm install
        Assert-LastExitCode 'npm install'
        Write-Ok 'Dependencias npm instaladas.'
    } else {
        Write-Info 'npm install ignorado por -SkipNpmInstall.'
    }

    Write-Section '6/6 - Validacao do Tauri'

    Write-Info 'Validando Cargo no projeto Tauri...'
    & cargo metadata --manifest-path (Join-Path $RepoRoot 'src-tauri\Cargo.toml') --no-deps --format-version 1 | Out-Null
    Assert-LastExitCode 'cargo metadata'
    Write-Ok 'Cargo/Tauri conseguem ler o projeto.'

    Write-Host ''
    Write-Host '==============================================================' -ForegroundColor Green
    Write-Host ' Ambiente do Moddin pronto.' -ForegroundColor Green
    Write-Host '==============================================================' -ForegroundColor Green
    Write-Host ''

    if (-not $SkipRun) {
        Write-Info 'Iniciando Moddin em modo desenvolvimento...'
        Write-Host ''
        & npm run tauri dev
        $tauriExit = $LASTEXITCODE

        if ($tauriExit -ne 0) {
            throw "npm run tauri dev terminou com codigo $tauriExit."
        }
    } else {
        Write-Info 'Execucao do app ignorada por -SkipRun.'
        Write-Host 'Para iniciar depois: npm run tauri dev' -ForegroundColor White
    }

    exit 0
}
catch {
    Write-Host ''
    Write-Host 'ERRO NO SETUP' -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Red
    Write-Host ''
    Write-Host "Log: $LogFile" -ForegroundColor Yellow
    exit 1
}
finally {
    Stop-SetupTranscript
}
