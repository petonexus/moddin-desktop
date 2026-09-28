# registers-cursor.ps1
# Automates: npm install + smoke test + merge moddin into Cursor's mcp.json.
# Re-runnable. Safe to call again.

$ErrorActionPreference = 'Stop'

$here       = Split-Path -Parent $PSScriptRoot
$mcpServer  = Join-Path $here 'src\mcp-server.mjs'
$cursorCfg  = Join-Path $env:USERPROFILE '.cursor\mcp.json'

Write-Host '[1/4] Checking Node...' -ForegroundColor Cyan
$node = (node -v) 2>$null
if (-not $node -or $node -notmatch 'v(\d+)') {
    throw 'Node.js is not on PATH. Install Node 20+ first (winget install OpenJS.NodeJS.LTS).'
}
$major = [int]$Matches[1]
if ($major -lt 20) { throw "Node $node is too old. Need 20+." }
Write-Host "      Node $node OK"

Write-Host '[2/4] npm install...' -ForegroundColor Cyan
Push-Location $here
try {
    npm install --no-audit --no-fund | Out-Null
    Write-Host '      deps OK'
} finally { Pop-Location }

Write-Host '[3/4] smoke test...' -ForegroundColor Cyan
node "$here\scripts\smoke-test.mjs" | Out-Null
Write-Host '      MCP server boots, validate + preview OK'

Write-Host '[4/4] merging moddin into mcp.json...' -ForegroundColor Cyan
if (Test-Path $cursorCfg) {
    try {
        $existing = Get-Content $cursorCfg -Raw | ConvertFrom-Json -ErrorAction Stop
    } catch {
        throw "Cursor mcp.json is not valid JSON. Fix it manually first: $cursorCfg"
    }
} else {
    $existing = [pscustomobject]@{ mcpServers = [pscustomobject]@{} }
}

if (-not $existing.mcpServers) { $existing | Add-Member -NotePropertyName mcpServers -NotePropertyValue ([pscustomobject]@{}) }

if ($existing.mcpServers.PSObject.Properties.Name -contains 'moddin') {
    Write-Host '      moddin already registered, skipping' -ForegroundColor Yellow
} else {
    $moddin = [pscustomobject]@{
        command = 'node'
        args    = @($mcpServer)
        env     = [pscustomobject]@{
            MODDIN_PROJECT_ROOT          = 'C:/mods/moddin/moddin'
            MODDIN_LOCAL_CAPABILITIES_DIR = ''
        }
    }
    $existing.mcpServers | Add-Member -NotePropertyName 'moddin' -NotePropertyValue $moddin -Force
    ($existing | ConvertTo-Json -Depth 10) | Set-Content -Path $cursorCfg -Encoding UTF8
    Write-Host "      wrote $cursorCfg"
}

Write-Host ''
Write-Host 'Done. Restart Cursor, then in any chat ask:' -ForegroundColor Green
Write-Host '   "Chame list_supported_games do MCP moddin e me devolva a lista."'
