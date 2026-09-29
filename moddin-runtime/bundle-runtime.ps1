# Re-creates the moddin-runtime/ folder from scratch.
#
# This is the script CI / a developer runs before `npm run tauri build`.
# It does not require Node.js to be installed on the machine doing the
# build — the script downloads Node itself.

$ErrorActionPreference = 'Stop'
$here        = $PSScriptRoot
$runtimeRoot = (Resolve-Path (Join-Path $here '..')).Path
$outDir      = Join-Path $runtimeRoot 'moddin-runtime'
$nodeUrl     = 'https://nodejs.org/dist/v20.19.5/node-v20.19.5-win-x64.zip'
$zipPath     = Join-Path $outDir 'node.zip'
$tmp         = Join-Path $outDir 'tmp'

Write-Host '== Moddin runtime bundle ==' -ForegroundColor Cyan

# 1) Download portable Node if not already present.
if (-not (Test-Path (Join-Path $outDir 'node.exe'))) {
    Write-Host '[1/3] Downloading portable Node...' -ForegroundColor Cyan
    New-Item -ItemType Directory -Path $outDir -Force | Out-Null
    Invoke-WebRequest -Uri $nodeUrl -OutFile $zipPath | Out-Null
    New-Item -ItemType Directory -Path $tmp -Force | Out-Null
    Expand-Archive -Path $zipPath -DestinationPath $tmp -Force
    $bin = Get-ChildItem -Path $tmp -Filter 'node.exe' -Recurse | Select-Object -First 1
    Move-Item -Force -Path $bin.FullName -Destination (Join-Path $outDir 'node.exe')
    # Best-effort cleanup of staging. The hard-safety policy may keep
    # the original zip around; CI scripts run in a clean dir each time.
    if (Test-Path $zipPath) { Remove-Item -Force $zipPath }
} else {
    Write-Host '[1/3] node.exe already present, skip download.' -ForegroundColor DarkCyan
}

# 2) Stage moddin-agent (prod-only) under moddin-runtime/moddin-agent.
$agentSrc = Join-Path $runtimeRoot 'moddin-agent'
$agentDst = Join-Path $outDir 'moddin-agent'
Write-Host '[2/3] Staging prod-only moddin-agent...' -ForegroundColor Cyan
if (Test-Path $agentDst) {
    # Keep the layout but nuke the previous runtime package.json so we
    # override devDependencies and lockfile.
    Copy-Item -Path (Join-Path $agentSrc '*') -Destination $agentDst -Recurse -Force
} else {
    New-Item -ItemType Directory -Path $agentDst -Force | Out-Null
    Copy-Item -Path (Join-Path $agentSrc '*') -Destination $agentDst -Recurse -Force
}

# 3) Install production deps (no @yao-pkg/pkg etc.), smoke-test the
#    bundled runtime, then write a tiny marker file.
Write-Host '[3/3] Installing prod deps + smoke test...' -ForegroundColor Cyan
$prodPkg = @{
    name        = 'moddin-agent-runtime'
    version     = '0.1.0'
    private     = $true
    description = 'Production-only copy of moddin-agent shipped inside Moddin Desktop.'
    type        = 'module'
    main        = 'src/mcp-server.mjs'
    engines     = @{ node = '>=20' }
    dependencies = @{
        '@modelcontextprotocol/sdk' = '^1.0.4'
        'ajv'          = '^8.17.1'
        'ajv-formats'  = '^3.0.1'
        'yaml'         = '^2.6.1'
    }
} | ConvertTo-Json -Depth 6
Set-Content -Path (Join-Path $agentDst 'package.json') -Value $prodPkg -Encoding UTF8

Push-Location $agentDst
try {
    if (Test-Path 'node_modules.old') {
        # A previous run may have left its staging copy behind.
        Remove-Item -Recurse -Force 'node_modules.old'
    }
    if (Test-Path 'node_modules') {
        # Move stale node_modules aside so npm install does not pile up.
        Rename-Item -Force -Path 'node_modules' -NewName 'node_modules.old'
    }
    npm install --omit=dev --no-audit --no-fund | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'npm install failed' }
} finally { Pop-Location }

# Smoke test the staged runtime. MODDIN_PROJECT_ROOT must point at a
# project root (repo root) so the server resolves src/catalog/games and
# src-tauri/capabilities — the staged moddin-runtime/ folder itself is
# not one. Same layout the standalone CI smoke step relies on.
$env:MODDIN_PROJECT_ROOT = $runtimeRoot
& (Join-Path $outDir 'node.exe') (Join-Path $agentDst 'scripts\smoke-test.mjs') | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'smoke test failed; refusing to mark runtime OK' }
Remove-Item Env:MODDIN_PROJECT_ROOT

Set-Content -Path (Join-Path $outDir 'BUNDLE_OK') -Value "built $(Get-Date -Format o)" -Encoding UTF8
Write-Host '== runtime OK ==' -ForegroundColor Green
Write-Host "Bundled to $outDir" -ForegroundColor Green
