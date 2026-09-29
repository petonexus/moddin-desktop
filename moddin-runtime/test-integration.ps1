# Integration smoke test: validates the runtime layout Moddin ships.
#
# 1. Confirms node.exe + moddin-agent/src/mcp-server.mjs exist in the
#    runtime directory.
# 2. Runs the moddin-agent smoke-test with the bundled node.
# 3. Drives a JSON-RPC initialize + list_supported_games against the
#    bundled node+mcp-server pair the same way Cursor would.

$ErrorActionPreference = 'Stop'
$here       = $PSScriptRoot
$node       = Join-Path $here 'node.exe'
$script     = Join-Path $here 'moddin-agent\src\mcp-server.mjs'
$smoke      = Join-Path $here 'moddin-agent\scripts\smoke-test.mjs'
$env:MODDIN_PROJECT_ROOT          = 'C:\mods\moddin\moddin'
$env:MODDIN_LOCAL_CAPABILITIES_DIR = ''

function Step($n, $msg) { Write-Host "[$n] $msg" -ForegroundColor Cyan }
function Pass($msg)     { Write-Host "    OK  $msg" -ForegroundColor Green }
function Fail($msg)     { Write-Host "    FAIL $msg" -ForegroundColor Red; exit 1 }

Step '1/4' "Bundled node.exe"
if (-not (Test-Path $node)) { Fail "missing node.exe at $node" }
Pass $node

Step '2/4' "Bundled MCP script"
if (-not (Test-Path $script)) { Fail "missing mcp-server.mjs at $script" }
Pass $script

Step '3/4' "Smoke test (schema, validate, preview)"
# Use cmd /c so PowerShell stops mis-interpreting the MCP server's
# startup banner on stderr as an error.
$out = cmd /c "`"$node`" `"$smoke`" 2>NUL"
if ($LASTEXITCODE -ne 0) { Fail "smoke test exited $LASTEXITCODE`: $out" }
Pass "smoke test exited 0"

Step '4/4' "JSON-RPC over stdio"
$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $node
$psi.Arguments = '"' + $script + '"'
$psi.UseShellExecute = $false
$psi.RedirectStandardInput  = $true
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError  = $true
$psi.WorkingDirectory = $here
$psi.EnvironmentVariables['MODDIN_PROJECT_ROOT']           = 'C:\mods\moddin\moddin'
$psi.EnvironmentVariables['MODDIN_LOCAL_CAPABILITIES_DIR'] = ''

$proc = [System.Diagnostics.Process]::Start($psi)
$proc.StandardInput.WriteLine('{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"integration","version":"1.0.0"}}}')
$proc.StandardInput.WriteLine('{"jsonrpc":"2.0","method":"notifications/initialized"}')
$proc.StandardInput.WriteLine('{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_supported_games","arguments":{}}}')
$proc.StandardInput.Close()

# Read everything the server wrote (with a hard timeout via
# WaitForExit). MCP servers speak JSON-RPC line-delimited.
$stdoutTask = $proc.StandardOutput.ReadToEndAsync()
$stderrTask = $proc.StandardError.ReadToEndAsync()
$exitedIn   = $proc.WaitForExit(8000)
if (-not $exitedIn) { $proc.Kill() }
$stdout = $stdoutTask.Result
$stderr = $stderrTask.Result

$lines = ($stdout -split "`r?`n") | Where-Object { $_ -match '^\{' }
# The server emits a reply for `tools/call list_supported_games` (id=2)
# after the initialize handshake. Looking for any non-empty JSON-RPC
# line is enough proof of end-to-end life.
$reply = $null
foreach ($line in $lines) {
    try {
        $obj = $line | ConvertFrom-Json
        if ($obj.id -eq 2 -and $obj.result.content[0].text) { $reply = $obj; break }
    } catch { }
}
if (-not $reply) {
    Fail "no JSON-RPC reply for list_supported_games captured.`nstdout: $stdout`nstderr: $stderr"
}
$games = ($reply.result.content[0].text | ConvertFrom-Json).games
if ($games.Count -lt 1) { Fail "no games returned" }
Pass "list_supported_games returned $($games.Count) games (first: $($games[0].id))"

Write-Host ""
Write-Host "All integration checks passed." -ForegroundColor Green
