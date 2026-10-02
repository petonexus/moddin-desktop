# Builds moddin-runtime/, the folder tauri.conf.json lists under
# `bundle.resources` and Tauri therefore ships inside the installer: a
# digest-verified portable Node plus a production-only copy of
# moddin-agent.
#
# It REFRESHES that folder in place. It does not recreate it from
# scratch: a node.exe that is already present is kept, and the previous
# staging is overwritten rather than deleted first. (This header used to
# claim otherwise, which is how a reader stopped checking what a
# "rebuilt" runtime folder actually contained.)
#
# Node.js IS required on the machine running it: step 3 shells out to
# `npm ci` to stage the agent's production dependencies. The script
# downloads a *portable* Node for the bundle itself, so that copy is
# pinned and verified rather than whatever the build machine happens to
# have installed -- but it does not bootstrap a toolchain.

$ErrorActionPreference = 'Stop'
$here        = $PSScriptRoot
$runtimeRoot = (Resolve-Path (Join-Path $here '..')).Path
$outDir      = Join-Path $runtimeRoot 'moddin-runtime'
$nodeVersion = 'v20.19.5'
$nodeFile    = "node-$nodeVersion-win-x64.zip"
$nodeUrl     = "https://nodejs.org/dist/$nodeVersion/$nodeFile"
$shasumsUrl  = "https://nodejs.org/dist/$nodeVersion/SHASUMS256.txt"
$zipPath     = Join-Path $outDir 'node.zip'
$tmp         = Join-Path $outDir 'tmp'
$agentSrc    = Join-Path $runtimeRoot 'moddin-agent'
$agentDst    = Join-Path $outDir 'moddin-agent'

# The production dependency set, in manifest order. Versions are NOT
# written here: they are read out of the agent's committed lockfile so
# that the only thing deciding what ships is a tracked file.
$prodDeps = @('@modelcontextprotocol/sdk', 'ajv', 'ajv-formats', 'yaml')

# Everything this run can leave behind. node.zip is 28.5 MB, tmp/ is the
# ~79 MB extracted archive and node_modules.old is a 56 MB dev install
# from a previous version of this script. None of the three is read at
# runtime and, because tauri.conf.json bundles the whole moddin-agent
# directory, node_modules.old would ship. Removal used to be
# "best-effort", with a comment normalising the case where it did not
# happen, which is how the archive ended up in every build tree.
function Remove-BuildScratch {
    param([string[]]$Path)
    foreach ($p in $Path) {
        $resolved = [System.IO.Path]::GetFullPath($p)
        if (-not $resolved.StartsWith($outDir + [System.IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw "Build scratch path is outside the runtime: $resolved" }
        if (Test-Path -LiteralPath $p) {
            Remove-Item -LiteralPath $p -Recurse -Force -ErrorAction SilentlyContinue
        }
        if (Test-Path -LiteralPath $p) {
            # Loud, because the quiet version of this is a release that
            # ships dead weight and reports success.
            throw "Could not remove build scratch '$p'. Close whatever is holding it and re-run: leaving it in place ships it."
        }
    }
}

Write-Host '== Moddin runtime bundle ==' -ForegroundColor Cyan
New-Item -ItemType Directory -Path $outDir -Force | Out-Null
$bundleOk = $false

try {
    # 1) Download portable Node if not already present, verifying its digest
    #    against the SHASUMS256.txt Node publishes next to the archive.
    #
    #    This archive is downloaded on every release build and then ships
    #    inside the installer, so an unauthenticated download here is the
    #    weakest link in a product whose whole pitch is that it verifies
    #    what it installs. The digests come from the same host as the
    #    archive, which is not a full supply-chain guarantee -- it is what
    #    nodejs.org offers without a signature, and it catches a corrupted
    #    or substituted download rather than a compromised host.
    if (-not (Test-Path (Join-Path $outDir 'node.exe'))) {
        Write-Host '[1/3] Downloading portable Node...' -ForegroundColor Cyan
        Invoke-WebRequest -Uri $shasumsUrl -OutFile (Join-Path $outDir 'SHASUMS256.txt')
        $expected = (Get-Content (Join-Path $outDir 'SHASUMS256.txt') |
            Where-Object { $_ -match "\s$([regex]::Escape($nodeFile))\s*$" } |
            ForEach-Object { ($_ -split '\s+')[0] } |
            Select-Object -First 1)
        if (-not $expected) {
            throw "No published SHA-256 for $nodeFile; refusing to install an unverified runtime."
        }

        Invoke-WebRequest -Uri $nodeUrl -OutFile $zipPath
        $actual = (Get-FileHash -Path $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $expected.ToLowerInvariant()) {
            throw "SHA-256 mismatch for $nodeFile. Expected $expected, got $actual. Refusing to install."
        }
        Write-Host "      digest OK: $actual" -ForegroundColor DarkGreen

        New-Item -ItemType Directory -Path $tmp -Force | Out-Null
        Expand-Archive -Path $zipPath -DestinationPath $tmp -Force
        $bin = Get-ChildItem -Path $tmp -Filter 'node.exe' -Recurse | Select-Object -First 1
        if (-not $bin) { throw "Extracted archive contained no node.exe; refusing to bundle." }
        Move-Item -Force -Path $bin.FullName -Destination (Join-Path $outDir 'node.exe')
    } else {
        Write-Host '[1/3] node.exe already present, skip download.' -ForegroundColor DarkCyan
    }

    # 2) Stage moddin-agent under moddin-runtime/moddin-agent.
    #
    #    Everything except node_modules. The source tree usually has one
    #    (ci.yml runs `npm ci` there before bundling), it is the full dev
    #    install, and copying it put 56 MB of development dependencies --
    #    @yao-pkg/pkg above all -- alongside the 18 MB of production ones.
    #    The previous script renamed that copy to node_modules.old rather
    #    than skipping it, so the installer carried both.
    Write-Host '[2/3] Staging moddin-agent sources...' -ForegroundColor Cyan
    New-Item -ItemType Directory -Path $agentDst -Force | Out-Null
    Get-ChildItem -LiteralPath $agentSrc -Force |
        Where-Object { $_.Name -ne 'node_modules' } |
        Copy-Item -Destination $agentDst -Recurse -Force

    # Ship the catalog so the installed MCP needs no development checkout.
    $dataRoot = Join-Path $agentDst 'data'
    Remove-BuildScratch @($dataRoot)
    $capData = Join-Path $dataRoot 'src-tauri/capabilities'
    $gameData = Join-Path $dataRoot 'src/catalog/games'
    New-Item -ItemType Directory -Path $capData, $gameData -Force | Out-Null
    Copy-Item -Path (Join-Path $runtimeRoot 'src-tauri/capabilities/*.yaml') -Destination $capData -Force
    Copy-Item -Path (Join-Path $runtimeRoot 'src/catalog/games/*.yaml') -Destination $gameData -Force

    # 3) Generate the production manifest, then install with `npm ci`.
    #
    #    Both generated files are pure functions of
    #    moddin-agent/package-lock.json, which is tracked. That is what
    #    closes ROADMAP O-06: the blocker was that the generated manifest
    #    lives under a gitignored path, so no lockfile could be committed
    #    next to it. The lockfile that matters already exists -- it sits
    #    next to moddin-agent/package.json, the manifest the staged one is
    #    derived from -- so the answer is to copy it in and install from
    #    it, not to invent a second tracked file.
    #
    #    The staged manifest pins EXACT versions rather than the carets
    #    this script used to write. With a lockfile those carets would be
    #    inert, but a manifest that declares a range is a manifest that a
    #    future edit -- or a human reading it -- can take at face value,
    #    and the install would then quietly stop matching the lockfile.
    Write-Host '[3/3] Installing pinned prod deps + smoke test...' -ForegroundColor Cyan
    $lockSrc = Join-Path $agentSrc 'package-lock.json'
    if (-not (Test-Path -LiteralPath $lockSrc)) {
        throw "No lockfile at '$lockSrc'. Refusing to resolve dependency ranges from the registry; run 'npm install' in moddin-agent and commit the result."
    }

    # Read the resolved versions out of the lockfile with the bundled Node.
    # Not with ConvertFrom-Json: a lockfile has a `packages[""]` entry (the
    # root project), and Windows PowerShell 5.1 throws on an empty JSON key.
    # CI runs pwsh 7, where that would have gone unnoticed until a
    # maintainer on 5.1 ran the bundler.
    $readLock = @'
const fs = require('fs');
const lock = JSON.parse(fs.readFileSync(process.argv[1], 'utf8'));
const out = {};
for (const dep of process.argv.slice(2)) {
  const entry = lock.packages['node_modules/' + dep];
  if (!entry || !entry.version) {
    process.stderr.write('unresolved: ' + dep + '\n');
    process.exit(2);
  }
  out[dep] = entry.version;
}
process.stdout.write(JSON.stringify(out));
'@
    $lockJson = & (Join-Path $outDir 'node.exe') -e $readLock $lockSrc @prodDeps
    if ($LASTEXITCODE -ne 0) {
        throw "moddin-agent/package-lock.json does not resolve every production dependency. Re-run 'npm install' in moddin-agent and commit the lockfile."
    }
    $pinned = [ordered]@{}
    foreach ($prop in ($lockJson | ConvertFrom-Json).PSObject.Properties) {
        $pinned[$prop.Name] = $prop.Value
    }
    Write-Host "      pinned: $(($pinned.GetEnumerator() | ForEach-Object { "$($_.Key)@$($_.Value)" }) -join ', ')" -ForegroundColor DarkGreen

    $prodPkg = [ordered]@{
        name         = 'moddin-agent-runtime'
        version      = '0.1.0'
        private      = $true
        description  = 'Production-only copy of moddin-agent shipped inside Moddin Desktop.'
        type         = 'module'
        main         = 'src/mcp-server.mjs'
        engines      = [ordered]@{ node = '>=20' }
        dependencies = $pinned
    } | ConvertTo-Json -Depth 6
    Set-Content -Path (Join-Path $agentDst 'package.json') -Value $prodPkg -Encoding UTF8

    Push-Location $agentDst
    try {
        # `npm ci` removes node_modules itself and installs the locked
        # tree exactly, so the install is a function of the lockfile and
        # not of whatever the registry would have resolved this morning.
        # The lockfile is not rewritten.
        npm ci --omit=dev --no-audit --no-fund | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'npm ci failed' }
    } finally { Pop-Location }

    # Smoke test the staged runtime. MODDIN_PROJECT_ROOT must point at a
    # project root (repo root) so the server resolves src/catalog/games and
    # src-tauri/capabilities -- the staged moddin-runtime/ folder itself is
    # not one. Same layout the standalone CI smoke step relies on.
    $previousProjectRoot = $env:MODDIN_PROJECT_ROOT
    try {
        $env:MODDIN_PROJECT_ROOT = $dataRoot
        & (Join-Path $outDir 'node.exe') (Join-Path $agentDst 'scripts\smoke-test.mjs') | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'smoke test failed; refusing to mark runtime OK' }
    } finally { $env:MODDIN_PROJECT_ROOT = $previousProjectRoot }

    # Nothing this run created may survive it -- including a run that
    # failed, which is when the old script left the most behind.
    Remove-BuildScratch @(
        $zipPath,
        $tmp,
        (Join-Path $agentDst 'node_modules.old')
    )

    $stagedPkg = Get-Content -LiteralPath (Join-Path $agentDst 'package.json') -Raw | ConvertFrom-Json
    foreach ($prop in $stagedPkg.dependencies.PSObject.Properties) {
        if ($prop.Value -notmatch '^\d+\.\d+\.\d+([-+].*)?$') {
            throw "Staged manifest pins '$($prop.Name)' as '$($prop.Value)', which is a range. The bundled runtime must be exact."
        }
    }
    $required = @(
        (Join-Path $outDir 'node.exe'),
        (Join-Path $agentDst 'package.json'),
        (Join-Path $agentDst 'package-lock.json'),
        (Join-Path $agentDst 'node_modules')
    )
    $missing = $required | Where-Object { -not (Test-Path -LiteralPath $_) }
    if ($missing) {
        throw "Runtime bundle is incomplete; missing: $($missing -join ', ')"
    }

    Set-Content -Path (Join-Path $outDir 'BUNDLE_OK') -Value "built $(Get-Date -Format o)" -Encoding UTF8
    $bundleOk = $true
} finally {
    if (-not $bundleOk) {
        # The run is already failing, so clean up quietly rather than
        # replacing the real error with a cleanup one -- a digest mismatch
        # or a broken smoke test is not something "could not delete
        # node.zip" should hide. The next successful run sweeps the same
        # three paths, before it writes BUNDLE_OK.
        foreach ($p in @($zipPath, $tmp, (Join-Path $agentDst 'node_modules.old'))) {
            if (Test-Path -LiteralPath $p) {
                Remove-Item -LiteralPath $p -Recurse -Force -ErrorAction SilentlyContinue
            }
            if (Test-Path -LiteralPath $p) {
                Write-Warning "Left '$p' behind; the next successful run sweeps it."
            }
        }
    }
}

Write-Host '== runtime OK ==' -ForegroundColor Green
Write-Host "Bundled to $outDir" -ForegroundColor Green
