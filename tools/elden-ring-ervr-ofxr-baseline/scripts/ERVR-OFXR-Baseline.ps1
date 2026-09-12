[CmdletBinding()]
param(
    [ValidateSet('Apply', 'Verify', 'Restore')]
    [string]$Mode = 'Verify',
    [string]$GamePath
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

$scriptDirectory = Split-Path -Parent $MyInvocation.MyCommand.Path
$packageDirectory = Split-Path -Parent $scriptDirectory
$scriptPath = $MyInvocation.MyCommand.Path
$runId = Get-Date -Format 'yyyyMMdd-HHmmss'
$baselineValues = @(
    @{ Section = 'VR';          Key = 'StereoMode';   Value = 'full'  }
    @{ Section = 'VR';          Key = 'GameRes';      Value = 'auto'  }
    @{ Section = 'VR';          Key = 'GameResScale'; Value = '1.0'   }
    @{ Section = 'VR';          Key = 'RenderScale';  Value = '1.0'   }
    @{ Section = 'Patches';     Key = 'FpsTarget';    Value = '60'    }
    @{ Section = 'FirstPerson'; Key = 'CameraBob';    Value = '0'     }
    @{ Section = 'HUD';         Key = 'Mode';         Value = 'quad'  }
    @{ Section = 'HUD';         Key = 'Lock';         Value = 'body'  }
    @{ Section = 'General';     Key = 'LogLevel';     Value = 'debug' }
)

function Get-FullPathSafe {
    param([string]$Path)

    if ([string]::IsNullOrWhiteSpace($Path)) {
        return $null
    }

    try {
        return [IO.Path]::GetFullPath($Path.Trim('"'))
    } catch {
        return $Path.Trim('"')
    }
}

function Write-Info {
    param([string]$Message)
    Write-Host $Message
}

function Write-Ok {
    param([string]$Message)
    Write-Host $Message -ForegroundColor Green
}

function Write-Warn {
    param([string]$Message)
    Write-Host $Message -ForegroundColor Yellow
}

function Write-Err {
    param([string]$Message)
    Write-Host $Message -ForegroundColor Red
}

function Test-IsAdministrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Ensure-Administrator {
    param([string]$RequestedMode)

    if (Test-IsAdministrator) {
        return
    }

    $quote = [char]34
    $arguments = "-NoProfile -ExecutionPolicy Bypass -File $quote$($script:scriptPath)$quote -Mode $RequestedMode"
    if (-not [string]::IsNullOrWhiteSpace($GamePath)) {
        $arguments += " -GamePath $quote$GamePath$quote"
    }

    Write-Warn 'Uma entrada Cheeky OpenXR em HKLM precisa de permissao de administrador.'
    $process = Start-Process -FilePath 'powershell.exe' -Verb RunAs -ArgumentList $arguments -Wait -PassThru
    exit $process.ExitCode
}

function Get-SteamRoots {
    $roots = New-Object System.Collections.ArrayList
    $steamRoot = $null

    foreach ($registryPath in @(
        'HKCU:\Software\Valve\Steam',
        'HKLM:\SOFTWARE\WOW6432Node\Valve\Steam'
    )) {
        try {
            $properties = Get-ItemProperty -Path $registryPath -ErrorAction Stop
            foreach ($propertyName in @('SteamPath', 'InstallPath')) {
                if ($properties.PSObject.Properties.Name -contains $propertyName) {
                    $candidate = Get-FullPathSafe ([string]$properties.$propertyName)
                    if ($candidate -and (Test-Path -LiteralPath $candidate)) {
                        [void]$roots.Add($candidate)
                        if (-not $steamRoot) {
                            $steamRoot = $candidate
                        }
                    }
                }
            }
        } catch {
            # A missing registry branch is normal on non-standard Steam setups.
        }
    }

    if (-not $steamRoot) {
        $programFilesX86 = [Environment]::GetEnvironmentVariable('ProgramFiles(x86)')
        if ($programFilesX86) {
            $candidate = Join-Path $programFilesX86 'Steam'
            if (Test-Path -LiteralPath $candidate) {
                $steamRoot = $candidate
                [void]$roots.Add($candidate)
            }
        }
    }

    foreach ($root in @($roots | Select-Object -Unique)) {
        $libraryFile = Join-Path $root 'steamapps\libraryfolders.vdf'
        if (-not (Test-Path -LiteralPath $libraryFile -PathType Leaf)) {
            continue
        }

        try {
            $contents = Get-Content -LiteralPath $libraryFile -Raw
            foreach ($match in [regex]::Matches($contents, '"path"\s+"([^"]+)"')) {
                $library = $match.Groups[1].Value -replace '\\\\', '\'
                $library = Get-FullPathSafe $library
                if ($library -and (Test-Path -LiteralPath $library)) {
                    [void]$roots.Add($library)
                }
            }
        } catch {
            Write-Warn ("Nao consegui ler a biblioteca Steam: {0}" -f $libraryFile)
        }
    }

    return @($roots | Select-Object -Unique)
}

function Find-GameDirectory {
    param([string]$Override)

    if (-not [string]::IsNullOrWhiteSpace($Override)) {
        $path = Get-FullPathSafe $Override

        if (Test-Path -LiteralPath $path -PathType Leaf) {
            if ([IO.Path]::GetFileName($path) -ieq 'eldenring.exe') {
                return (Split-Path -Parent $path)
            }
        }

        if (Test-Path -LiteralPath $path -PathType Container) {
            if (Test-Path -LiteralPath (Join-Path $path 'eldenring.exe')) {
                return $path
            }
            if (Test-Path -LiteralPath (Join-Path $path 'Game\eldenring.exe')) {
                return (Join-Path $path 'Game')
            }
        }
    }

    foreach ($library in (Get-SteamRoots)) {
        $steamApps = Join-Path $library 'steamapps'
        $manifest = Join-Path $steamApps 'appmanifest_1245620.acf'
        if (-not (Test-Path -LiteralPath $manifest -PathType Leaf)) {
            continue
        }

        try {
            $contents = Get-Content -LiteralPath $manifest -Raw
            $match = [regex]::Match($contents, '(?m)^\s*"installdir"\s+"([^"]+)"')
            if (-not $match.Success) {
                continue
            }

            $installRoot = Join-Path (Join-Path $steamApps 'common') $match.Groups[1].Value
            $gameDirectory = Join-Path $installRoot 'Game'
            if (Test-Path -LiteralPath (Join-Path $gameDirectory 'eldenring.exe')) {
                return (Get-FullPathSafe $gameDirectory)
            }
            if (Test-Path -LiteralPath (Join-Path $installRoot 'eldenring.exe')) {
                return (Get-FullPathSafe $installRoot)
            }
        } catch {
            Write-Warn ("Nao consegui ler o manifesto Steam: {0}" -f $manifest)
        }
    }

    return $null
}

function Get-GameRoot {
    param([string]$GameDirectory)

    if ((Split-Path -Leaf $GameDirectory) -ieq 'Game') {
        return (Split-Path -Parent $GameDirectory)
    }
    return $GameDirectory
}

function Assert-GameClosed {
    $running = @(Get-Process -Name 'eldenring' -ErrorAction SilentlyContinue)
    if ($running.Count -gt 0) {
        throw 'eldenring.exe esta aberto. Feche o jogo antes de aplicar ou restaurar o baseline.'
    }
}

function Get-FileIdentity {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return ''
    }

    try {
        $versionInfo = (Get-Item -LiteralPath $Path).VersionInfo
        return (($versionInfo.ProductName, $versionInfo.FileDescription, $versionInfo.OriginalFilename, $versionInfo.CompanyName) -join ' ')
    } catch {
        return ''
    }
}

function Get-FileVersion {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return ''
    }

    try {
        return [string](Get-Item -LiteralPath $Path).VersionInfo.FileVersion
    } catch {
        return ''
    }
}

function Test-ReShadeAddOnHost {
    param([string]$GameDirectory)

    $dxgi = Join-Path $GameDirectory 'dxgi.dll'
    if (-not (Test-Path -LiteralPath $dxgi -PathType Leaf)) {
        return $false
    }

    if ((Get-FileIdentity $dxgi) -match '(?i)reshade') {
        return $true
    }

    foreach ($logPath in @(
        (Join-Path $GameDirectory 'ReShade.log'),
        (Join-Path $GameDirectory 'ERVR\ERVR.log')
    )) {
        if (-not (Test-Path -LiteralPath $logPath -PathType Leaf)) {
            continue
        }
        try {
            $contents = Get-Content -LiteralPath $logPath -Raw
            if ($contents -match '(?i)registered as add-on|ReShade.*add-on') {
                return $true
            }
        } catch {
            # A log can be locked by a previous launch; the file check remains useful.
        }
    }

    return $false
}

function Get-IniValue {
    param(
        [string]$Contents,
        [string]$Section,
        [string]$Key
    )

    $currentSection = ''
    foreach ($line in [regex]::Split($Contents, '\r?\n')) {
        $trimmed = $line.Trim()
        if ($trimmed -match '^\[(.+?)\]$') {
            $currentSection = $Matches[1].Trim()
            continue
        }
        if ($currentSection -ine $Section) {
            continue
        }
        if ($trimmed -match '^(.*?)=(.*)$' -and $Matches[1].Trim() -ieq $Key) {
            return $Matches[2].Trim()
        }
    }

    return $null
}

function Set-IniValue {
    param(
        [System.Collections.Generic.List[string]]$Lines,
        [string]$Section,
        [string]$Key,
        [string]$Value
    )

    $sectionIndex = -1
    $sectionEnd = $Lines.Count
    for ($index = 0; $index -lt $Lines.Count; $index++) {
        $trimmed = $Lines[$index].Trim()
        if ($trimmed -match '^\[(.+?)\]$') {
            if ($sectionIndex -ge 0) {
                $sectionEnd = $index
                break
            }
            if ($Matches[1].Trim() -ieq $Section) {
                $sectionIndex = $index
            }
        }
    }

    if ($sectionIndex -lt 0) {
        if ($Lines.Count -gt 0 -and $Lines[$Lines.Count - 1].Trim().Length -gt 0) {
            [void]$Lines.Add('')
        }
        [void]$Lines.Add("[$Section]")
        [void]$Lines.Add("$Key=$Value")
        return $true
    }

    $keyPattern = '^\s*' + [regex]::Escape($Key) + '\s*='
    for ($index = $sectionIndex + 1; $index -lt $sectionEnd; $index++) {
        if ($Lines[$index] -match $keyPattern) {
            $current = Get-IniValue ($Lines -join [Environment]::NewLine) $Section $Key
            if ($current -ceq $Value) {
                return $false
            }
            $indentLength = $Lines[$index].Length - $Lines[$index].TrimStart().Length
            $indent = if ($indentLength -gt 0) { $Lines[$index].Substring(0, $indentLength) } else { '' }
            $Lines[$index] = "$indent$Key=$Value"
            return $true
        }
    }

    $Lines.Insert($sectionEnd, "$Key=$Value")
    return $true
}

function Apply-BaselineIni {
    param([string]$IniPath)

    $lines = New-Object 'System.Collections.Generic.List[string]'
    foreach ($line in @(Get-Content -LiteralPath $IniPath)) {
        [void]$lines.Add([string]$line)
    }

    $changed = $false
    foreach ($setting in $baselineValues) {
        if (Set-IniValue $lines $setting.Section $setting.Key $setting.Value) {
            $changed = $true
        }
    }

    if ($changed) {
        Set-Content -LiteralPath $IniPath -Value $lines -Encoding UTF8
    }
    return $changed
}

function Get-RegistryValueForState {
    param(
        [object]$Value,
        [string]$Kind
    )

    switch ($Kind) {
        'DWord' { return [int]$Value }
        'QWord' { return [long]$Value }
        'Binary' { return [Convert]::ToBase64String([byte[]]$Value) }
        'MultiString' { return @($Value) }
        default { return [string]$Value }
    }
}

function Get-CheekyOpenXrEntries {
    $entries = New-Object System.Collections.ArrayList
    foreach ($keyPath in @(
        'HKLM:\SOFTWARE\Khronos\OpenXR\1\ApiLayers\Implicit',
        'HKCU:\SOFTWARE\Khronos\OpenXR\1\ApiLayers\Implicit',
        'HKLM:\SOFTWARE\WOW6432Node\Khronos\OpenXR\1\ApiLayers\Implicit',
        'HKCU:\SOFTWARE\WOW6432Node\Khronos\OpenXR\1\ApiLayers\Implicit'
    )) {
        try {
            $key = Get-Item -Path $keyPath -ErrorAction Stop
            foreach ($name in $key.GetValueNames()) {
                if ([string]$name -notmatch '(?i)cheeky') {
                    continue
                }
                $kind = [string]$key.GetValueKind($name)
                [void]$entries.Add([pscustomobject]@{
                    Key = $keyPath
                    Name = [string]$name
                    Value = Get-RegistryValueForState ($key.GetValue($name)) $kind
                    Kind = $kind
                })
            }
        } catch {
            # Missing registry branches are expected.
        }
    }
    return @($entries)
}

function Merge-RegistryEntries {
    param(
        [object[]]$Existing,
        [object[]]$Current
    )

    $merged = New-Object System.Collections.ArrayList
    foreach ($entry in @($Existing)) {
        [void]$merged.Add($entry)
    }
    foreach ($entry in @($Current)) {
        $alreadyCaptured = @($merged | Where-Object {
            ([string]$_.Key -ieq [string]$entry.Key) -and
            ([string]$_.Name -ieq [string]$entry.Name)
        }).Count -gt 0
        if (-not $alreadyCaptured) {
            [void]$merged.Add($entry)
        }
    }
    return @($merged)
}

function Disable-CheekyOpenXr {
    param([object[]]$Entries)

    foreach ($entry in @($Entries)) {
        New-ItemProperty -Path ([string]$entry.Key) -Name ([string]$entry.Name) -Value 1 -PropertyType DWord -Force | Out-Null
        Write-Ok ("Cheeky OpenXR desativado: {0}" -f $entry.Name)
    }
}

function Restore-RegistryEntry {
    param([object]$Entry)

    $keyPath = [string]$Entry.Key
    if (-not (Test-Path -Path $keyPath)) {
        New-Item -Path $keyPath -Force | Out-Null
    }

    $kind = [string]$Entry.Kind
    $value = $Entry.Value
    if ($kind -eq 'Binary') {
        $value = [Convert]::FromBase64String([string]$Entry.Value)
    } elseif ($kind -eq 'MultiString') {
        $value = @($Entry.Value)
    }

    New-ItemProperty -Path $keyPath -Name ([string]$Entry.Name) -Value $value -PropertyType $kind -Force | Out-Null
    Write-Ok ("Cheeky OpenXR restaurado: {0}" -f $Entry.Name)
}

function Restore-CheekyOpenXr {
    param([object[]]$Entries)

    foreach ($entry in @($Entries)) {
        Restore-RegistryEntry $entry
    }
}

function Get-OpenXrRuntime {
    foreach ($keyPath in @(
        'HKLM:\SOFTWARE\Khronos\OpenXR\1',
        'HKCU:\SOFTWARE\Khronos\OpenXR\1'
    )) {
        try {
            $properties = Get-ItemProperty -Path $keyPath -Name 'ActiveRuntime' -ErrorAction Stop
            if ($properties.ActiveRuntime) {
                return [string]$properties.ActiveRuntime
            }
        } catch {
            # Continue to the next scope.
        }
    }
    return ''
}

function Get-OfxrState {
    $localAppData = [Environment]::GetEnvironmentVariable('LOCALAPPDATA')
    if ([string]::IsNullOrWhiteSpace($localAppData)) {
        $localAppData = [IO.Path]::GetTempPath()
    }

    $roots = @(
        (Join-Path $localAppData 'Moddin\tools\ofxr-bridge'),
        (Join-Path $localAppData 'OFXR Bridge')
    ) | Where-Object { Test-Path -LiteralPath $_ -PathType Container }

    $manifestPaths = New-Object System.Collections.ArrayList
    foreach ($root in $roots) {
        try {
            foreach ($manifest in @(Get-ChildItem -LiteralPath $root -Recurse -File -Filter 'XR_APILAYER_XRFrameBridge_manual-*.json' -ErrorAction SilentlyContinue)) {
                [void]$manifestPaths.Add($manifest.FullName)
            }
        } catch {
            # A partially removed OFXR install is reported below.
        }
    }

    $registryEntries = New-Object System.Collections.ArrayList
    foreach ($keyPath in @(
        'HKCU:\SOFTWARE\Khronos\OpenXR\1\ApiLayers\Implicit',
        'HKLM:\SOFTWARE\Khronos\OpenXR\1\ApiLayers\Implicit',
        'HKCU:\SOFTWARE\WOW6432Node\Khronos\OpenXR\1\ApiLayers\Implicit',
        'HKLM:\SOFTWARE\WOW6432Node\Khronos\OpenXR\1\ApiLayers\Implicit'
    )) {
        try {
            $key = Get-Item -Path $keyPath -ErrorAction Stop
            foreach ($name in $key.GetValueNames()) {
                if ([string]$name -match '(?i)XRFrameBridge|OFXR') {
                    [void]$registryEntries.Add([pscustomobject]@{
                        Key = $keyPath
                        Name = [string]$name
                        Value = $key.GetValue($name)
                    })
                }
            }
        } catch {
            # Missing registry branches are expected.
        }
    }

    $armed = $false
    foreach ($manifestPath in @($manifestPaths | Select-Object -Unique)) {
        if (@($registryEntries | Where-Object { [string]$_.Name -ieq $manifestPath }).Count -gt 0) {
            $armed = $true
            break
        }
    }

    $trayPath = Join-Path $localAppData 'Moddin\tools\ofxr-bridge\OFXRBridgeTray.exe'
    $trayRunning = @(Get-Process -Name 'OFXRBridgeTray' -ErrorAction SilentlyContinue).Count -gt 0
    $settingsPath = Join-Path $localAppData 'OFXR Bridge\tray.ini'

    return [pscustomobject]@{
        Roots = @($roots)
        Manifests = @($manifestPaths | Select-Object -Unique)
        RegistryEntries = @($registryEntries)
        Installed = (Test-Path -LiteralPath $trayPath -PathType Leaf) -or $manifestPaths.Count -gt 0
        TrayPath = $trayPath
        TrayRunning = $trayRunning
        SettingsPath = $settingsPath
        SettingsExists = Test-Path -LiteralPath $settingsPath -PathType Leaf
        Armed = $armed
    }
}

function Add-Candidate {
    param(
        [System.Collections.ArrayList]$Items,
        [string]$Path,
        [string]$Reason
    )

    if (Test-Path -LiteralPath $Path) {
        [void]$Items.Add([pscustomobject]@{
            Path = (Get-FullPathSafe $Path)
            Reason = $Reason
        })
    }
}

function Get-CleanCandidates {
    param([string]$GameDirectory)

    $items = New-Object System.Collections.ArrayList
    Add-Candidate $items (Join-Path $GameDirectory 'CheekyFoveatedDLSS.addon64') 'Cheeky Foveated DLSS nao faz parte do baseline.'
    Add-Candidate $items (Join-Path $GameDirectory 'ERSS-FG.dll') 'ERSS-FG nao faz parte do baseline.'
    Add-Candidate $items (Join-Path $GameDirectory 'ERSSReShadeStub.addon') 'Stub ReShade do ERSS-FG nao faz parte do baseline.'
    Add-Candidate $items (Join-Path $GameDirectory 'ERSS2') 'Runtime/configuracao do ERSS-FG nao faz parte do baseline.'

    foreach ($name in @('RealVR64.dll', 'openvr_api.dll', 'cudart64_12.dll', 'RealVR.ini')) {
        Add-Candidate $items (Join-Path $GameDirectory $name) 'R.E.A.L. VR antigo nao faz parte do baseline ERVR.'
    }

    Add-Candidate $items (Join-Path $GameDirectory 'Streamline') 'Streamline nao faz parte do baseline.'

    foreach ($pattern in @('nvngx_dlss*.dll', 'sl.*.dll')) {
        try {
            foreach ($file in @(Get-ChildItem -LiteralPath $GameDirectory -File -Filter $pattern -ErrorAction SilentlyContinue)) {
                Add-Candidate $items $file.FullName 'Runtime DLSS/Streamline experimental nao faz parte do baseline.'
            }
        } catch {}
    }

    try {
        foreach ($file in @(Get-ChildItem -LiteralPath $GameDirectory -Force -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -like 'OptiScaler*' })) {
            Add-Candidate $items $file.FullName 'OptiScaler nao faz parte do baseline.'
        }
    } catch {}

    foreach ($pattern in @('*.addon64', '*.addon')) {
        try {
            foreach ($file in @(Get-ChildItem -LiteralPath $GameDirectory -File -Filter $pattern -ErrorAction SilentlyContinue)) {
                Add-Candidate $items $file.FullName 'Add-on ReShade extra nao faz parte do baseline.'
            }
        } catch {}
    }

    $d3d12 = Join-Path $GameDirectory 'd3d12.dll'
    $erssEvidence = (Test-Path -LiteralPath (Join-Path $GameDirectory 'ERSS-FG.dll')) -or
        (Test-Path -LiteralPath (Join-Path $GameDirectory 'ERSS2')) -or
        (Test-Path -LiteralPath (Join-Path $GameDirectory 'ERSSReShadeStub.addon'))
    $optiEvidence = @($items | Where-Object { [string]$_.Reason -match '(?i)OptiScaler' }).Count -gt 0 -or
        ((Get-FileIdentity $d3d12) -match '(?i)OptiScaler|ERSS')
    if ((Test-Path -LiteralPath $d3d12 -PathType Leaf) -and ($erssEvidence -or $optiEvidence)) {
        Add-Candidate $items $d3d12 'd3d12.dll identificado no contexto como loader ERSS/OptiScaler.'
    }

    $unique = $items | Sort-Object @{ Expression = { $_.Path.Length } }, Path
    $selected = New-Object System.Collections.ArrayList
    foreach ($candidate in @($unique)) {
        $isChild = $false
        foreach ($parent in @($selected)) {
            $parentPrefix = ([string]$parent.Path).TrimEnd('\') + '\'
            if ([string]$candidate.Path -ine ([string]$parent.Path) -and
                [string]$candidate.Path.StartsWith($parentPrefix, [StringComparison]::OrdinalIgnoreCase)) {
                $isChild = $true
                break
            }
        }
        if (-not $isChild -and @($selected | Where-Object { [string]$_.Path -ieq [string]$candidate.Path }).Count -eq 0) {
            [void]$selected.Add($candidate)
        }
    }
    return @($selected)
}

function Get-SuspiciousUntouched {
    param([string]$GameDirectory)

    $paths = New-Object System.Collections.ArrayList
    foreach ($name in @('version.dll', 'winmm.dll', 'winhttp.dll', 'wininet.dll', 'dbghelp.dll')) {
        $path = Join-Path $GameDirectory $name
        if (Test-Path -LiteralPath $path) {
            [void]$paths.Add($path)
        }
    }

    $d3d12 = Join-Path $GameDirectory 'd3d12.dll'
    $knownCandidates = @(Get-CleanCandidates $GameDirectory | ForEach-Object { [string]$_.Path })
    if ((Test-Path -LiteralPath $d3d12) -and
        @($knownCandidates | Where-Object { $_ -ieq $d3d12 }).Count -eq 0) {
        [void]$paths.Add($d3d12)
    }

    return @($paths | Select-Object -Unique)
}

function Get-StatePath {
    param([string]$GameRoot)
    return (Join-Path $GameRoot '_MODDIN_BACKUPS\ERVR-OFXR-Baseline\active-state.json')
}

function Read-State {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $null
    }
    return (Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json)
}

function Write-State {
    param(
        [string]$Path,
        [object]$State
    )

    $parent = Split-Path -Parent $Path
    if (-not (Test-Path -LiteralPath $parent)) {
        New-Item -ItemType Directory -Path $parent -Force | Out-Null
    }
    $State | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $Path -Encoding UTF8
}

function Get-UniqueDestination {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        return $Path
    }

    $directory = Split-Path -Parent $Path
    $name = Split-Path -Leaf $Path
    $candidate = Join-Path $directory ($name + '.duplicate-' + $runId)
    $counter = 1
    while (Test-Path -LiteralPath $candidate) {
        $candidate = Join-Path $directory ($name + '.duplicate-' + $runId + '-' + $counter)
        $counter++
    }
    return $candidate
}

function Move-ToQuarantine {
    param(
        [string]$GameDirectory,
        [string]$QuarantineDirectory,
        [object]$Candidate
    )

    $path = [string]$Candidate.Path
    if (-not (Test-Path -LiteralPath $path)) {
        return $null
    }

    $gamePrefix = $GameDirectory.TrimEnd('\') + '\'
    if (-not $path.StartsWith($gamePrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw ("Candidato fora da pasta Game foi recusado: {0}" -f $path)
    }

    $relative = $path.Substring($gamePrefix.Length)
    $destination = Get-UniqueDestination (Join-Path $QuarantineDirectory $relative)
    $destinationParent = Split-Path -Parent $destination
    if (-not (Test-Path -LiteralPath $destinationParent)) {
        New-Item -ItemType Directory -Path $destinationParent -Force | Out-Null
    }

    Move-Item -LiteralPath $path -Destination $destination
    Write-Ok ("QUARENTENA: {0} -> {1}" -f $relative, $destination)
    return [pscustomobject]@{
        Original = $path
        Destination = $destination
        RelativePath = $relative
        Reason = [string]$Candidate.Reason
        IsDirectory = Test-Path -LiteralPath $destination -PathType Container
    }
}

function Restore-MoveRecords {
    param([object[]]$Records)

    $warnings = New-Object System.Collections.ArrayList
    foreach ($record in @($Records | Sort-Object { ([string]$_.Original).Length } -Descending)) {
        $original = [string]$record.Original
        $destination = [string]$record.Destination
        if (-not (Test-Path -LiteralPath $destination)) {
            [void]$warnings.Add("Quarentena ausente: $destination")
            continue
        }
        if (Test-Path -LiteralPath $original) {
            [void]$warnings.Add("Destino original ja existe; mantido sem sobrescrever: $original")
            continue
        }

        $parent = Split-Path -Parent $original
        if (-not (Test-Path -LiteralPath $parent)) {
            New-Item -ItemType Directory -Path $parent -Force | Out-Null
        }
        Move-Item -LiteralPath $destination -Destination $original
        Write-Ok ("RESTAURADO: {0}" -f $original)
    }
    return @($warnings)
}

function Add-ReportLine {
    param(
        [System.Collections.ArrayList]$Lines,
        [string]$Text
    )
    [void]$Lines.Add($Text)
}

function Get-VerificationLines {
    param(
        [string]$GameDirectory,
        [string]$GameRoot,
        [string]$Status
    )

    $lines = New-Object System.Collections.ArrayList
    $reportPath = Join-Path $GameRoot '_MODDIN_BACKUPS\ERVR-OFXR-Baseline\BASELINE-REPORT.txt'
    $iniPath = Join-Path $GameDirectory 'ERVR\ERVR.ini'
    $exePath = Join-Path $GameDirectory 'eldenring.exe'

    Add-ReportLine $lines 'Elden Ring - ERVR + VDXR + OFXR baseline'
    Add-ReportLine $lines ('Status: {0}' -f $Status)
    Add-ReportLine $lines ('Generated: {0}' -f (Get-Date).ToString('o'))
    Add-ReportLine $lines ('Game directory: {0}' -f $GameDirectory)
    Add-ReportLine $lines ''
    Add-ReportLine $lines 'Required files:'

    foreach ($required in @(
        @{ Label = 'eldenring.exe'; Path = $exePath },
        @{ Label = 'dxgi.dll (ReShade 6.8 add-on host)'; Path = (Join-Path $GameDirectory 'dxgi.dll') },
        @{ Label = 'ReShade.ini'; Path = (Join-Path $GameDirectory 'ReShade.ini') },
        @{ Label = 'dinput8.dll'; Path = (Join-Path $GameDirectory 'dinput8.dll') },
        @{ Label = 'ERVR\ERVR.dll'; Path = (Join-Path $GameDirectory 'ERVR\ERVR.dll') },
        @{ Label = 'ERVR\ERVR.ini'; Path = $iniPath }
    )) {
        $exists = Test-Path -LiteralPath $required.Path -PathType Leaf
        $mark = if ($exists) { 'OK ' } else { 'ERR' }
        Add-ReportLine $lines ("[{0}] {1} :: {2}" -f $mark, $required.Label, $required.Path)
    }

    $version = Get-FileVersion $exePath
    if ($version) {
        $versionState = if ($version -match '^(2\.6\.2\.0|2\.7\.1\.0)') { 'OK ' } else { 'WARN' }
        Add-ReportLine $lines ("[{0}] Elden Ring exe version :: {1}" -f $versionState, $version)
    } else {
        Add-ReportLine $lines '[INFO] Elden Ring exe version :: unavailable'
    }

    $reshadeConfirmed = Test-ReShadeAddOnHost $GameDirectory
    $reshadeMark = if ($reshadeConfirmed) { 'OK ' } else { 'WARN' }
    $reshadeDetail = if ($reshadeConfirmed) { 'detected/confirmed' } else { 'not confirmed from DLL/log identity' }
    Add-ReportLine $lines ("[{0}] ReShade add-on host :: {1}" -f $reshadeMark, $reshadeDetail)

    Add-ReportLine $lines ''
    Add-ReportLine $lines 'ERVR.ini baseline values:'
    $iniContents = if (Test-Path -LiteralPath $iniPath -PathType Leaf) { Get-Content -LiteralPath $iniPath -Raw } else { '' }
    foreach ($setting in $baselineValues) {
        $current = Get-IniValue $iniContents $setting.Section $setting.Key
        $matches = $current -ceq [string]$setting.Value
        $mark = if ($matches) { 'OK ' } else { 'WARN' }
        $currentText = if ($null -eq $current) { '<missing>' } else { $current }
        Add-ReportLine $lines ("[{0}] [{1}] {2} :: expected={3}; current={4}" -f $mark, $setting.Section, $setting.Key, $setting.Value, $currentText)
    }

    $openxr = Get-OpenXrRuntime
    Add-ReportLine $lines ''
    if ($openxr) {
        $openxrMark = if (Test-Path -LiteralPath $openxr -PathType Leaf) { 'OK ' } else { 'WARN' }
        Add-ReportLine $lines ("[{0}] OpenXR ActiveRuntime :: {1}" -f $openxrMark, $openxr)
    } else {
        Add-ReportLine $lines '[ERR] OpenXR ActiveRuntime :: not found'
    }

    $ofxr = Get-OfxrState
    Add-ReportLine $lines ''
    Add-ReportLine $lines 'OFXR (read-only inspection; no install/update/configuration performed):'
    $installedMark = if ($ofxr.Installed) { 'OK ' } else { 'WARN' }
    $installedDetail = if ($ofxr.Installed) { 'found' } else { 'not found in Moddin known locations' }
    Add-ReportLine $lines ("[{0}] OFXR files :: {1}" -f $installedMark, $installedDetail)
    $trayMark = if ($ofxr.TrayRunning) { 'OK ' } else { 'INFO' }
    $trayDetail = if ($ofxr.TrayRunning) { 'running' } else { 'not running' }
    Add-ReportLine $lines ("[{0}] OFXR tray :: {1}" -f $trayMark, $trayDetail)
    $armedMark = if ($ofxr.Armed) { 'OK ' } else { 'WARN' }
    $armedDetail = if ($ofxr.Armed) { 'registered/armed' } else { 'not confirmed as registered/armed' }
    Add-ReportLine $lines ("[{0}] OFXR OpenXR layer :: {1}" -f $armedMark, $armedDetail)
    foreach ($manifest in @($ofxr.Manifests)) {
        Add-ReportLine $lines ("  manifest: {0}" -f $manifest)
    }
    foreach ($entry in @($ofxr.RegistryEntries)) {
        Add-ReportLine $lines ("  registry: {0} :: {1} = {2}" -f $entry.Key, $entry.Name, $entry.Value)
    }

    $cheeky = @(Get-CheekyOpenXrEntries)
    Add-ReportLine $lines ''
    Add-ReportLine $lines 'Cheeky OpenXR:'
    if ($cheeky.Count -eq 0) {
        Add-ReportLine $lines '  not found'
    } else {
        foreach ($entry in $cheeky) {
            $value = [string]$entry.Value
            $state = if ($value -eq '0') { 'ACTIVE' } else { 'DISABLED' }
            Add-ReportLine $lines ("  {0} :: {1} = {2} ({3})" -f $entry.Key, $entry.Name, $value, $state)
        }
    }

    Add-ReportLine $lines ''
    Add-ReportLine $lines 'Known extras still active:'
    $extras = @(Get-CleanCandidates $GameDirectory)
    if ($extras.Count -eq 0) {
        Add-ReportLine $lines '  none'
    } else {
        foreach ($extra in $extras) {
            Add-ReportLine $lines ("  - {0} :: {1}" -f $extra.Path, $extra.Reason)
        }
    }

    $untouched = @(Get-SuspiciousUntouched $GameDirectory)
    if ($untouched.Count -gt 0) {
        Add-ReportLine $lines ''
        Add-ReportLine $lines 'Suspicious proxy names deliberately not moved automatically:'
        foreach ($path in $untouched) {
            Add-ReportLine $lines ("  - {0}" -f $path)
        }
    }

    Add-ReportLine $lines ''
    Add-ReportLine $lines 'Safety: offline only; this package does not disable or bypass Easy Anti-Cheat.'
    Add-ReportLine $lines 'Preserved: eldenring.exe, dxgi.dll, ReShade.ini, dinput8.dll, ERVR\, and OFXR.'
    Add-ReportLine $lines ('Report path: {0}' -f $reportPath)
    return @($lines)
}

function Write-BaselineReport {
    param(
        [string]$GameDirectory,
        [string]$GameRoot,
        [string]$Status
    )

    $reportDirectory = Join-Path $GameRoot '_MODDIN_BACKUPS\ERVR-OFXR-Baseline'
    if (-not (Test-Path -LiteralPath $reportDirectory)) {
        New-Item -ItemType Directory -Path $reportDirectory -Force | Out-Null
    }
    $reportPath = Join-Path $reportDirectory 'BASELINE-REPORT.txt'
    $lines = @(Get-VerificationLines $GameDirectory $GameRoot $Status)
    Set-Content -LiteralPath $reportPath -Value $lines -Encoding UTF8
    foreach ($line in $lines) {
        Write-Host $line
    }
    return $reportPath
}

function New-BaselineState {
    param(
        [string]$GameDirectory,
        [string]$GameRoot,
        [string]$SnapshotDirectory,
        [object[]]$CheekyEntries
    )

    return [pscustomobject]@{
        Schema = 1
        Status = 'Applying'
        CreatedAt = (Get-Date).ToString('o')
        GameDirectory = $GameDirectory
        GameRoot = $GameRoot
        SnapshotDirectory = $SnapshotDirectory
        IniPath = (Join-Path $GameDirectory 'ERVR\ERVR.ini')
        Moves = @()
        CheekyEntries = @($CheekyEntries)
    }
}

function Apply-Baseline {
    param([string]$GameDirectory)

    Assert-GameClosed
    $gameRoot = Get-GameRoot $GameDirectory
    $baselineRoot = Join-Path $gameRoot '_MODDIN_BACKUPS\ERVR-OFXR-Baseline'
    $statePath = Join-Path $baselineRoot 'active-state.json'
    $iniPath = Join-Path $GameDirectory 'ERVR\ERVR.ini'

    foreach ($requiredPath in @(
        (Join-Path $GameDirectory 'eldenring.exe'),
        (Join-Path $GameDirectory 'dxgi.dll'),
        (Join-Path $GameDirectory 'ReShade.ini'),
        (Join-Path $GameDirectory 'dinput8.dll'),
        (Join-Path $GameDirectory 'ERVR\ERVR.dll'),
        $iniPath
    )) {
        if (-not (Test-Path -LiteralPath $requiredPath -PathType Leaf)) {
            throw ("Arquivo obrigatorio do baseline nao encontrado: {0}" -f $requiredPath)
        }
    }

    if (-not (Test-Path -LiteralPath $baselineRoot)) {
        New-Item -ItemType Directory -Path $baselineRoot -Force | Out-Null
    }

    $existingState = Read-State $statePath
    $newApply = $true
    $state = $null
    if ($existingState -and
        ([string]$existingState.GameDirectory -ieq $GameDirectory) -and
        ([string]$existingState.Status -in @('Applied', 'Applying'))) {
        $state = $existingState
        $newApply = ([string]$existingState.Status -ne 'Applied')
        Write-Info ("Usando checkpoint existente: {0}" -f $statePath)
    } else {
        $snapshotDirectory = Join-Path $baselineRoot ('snapshot-' + $runId)
        $quarantineDirectory = Join-Path $snapshotDirectory 'quarantine'
        New-Item -ItemType Directory -Path $quarantineDirectory -Force | Out-Null
        Copy-Item -LiteralPath $iniPath -Destination (Join-Path $snapshotDirectory 'ERVR.ini') -Force
        $state = New-BaselineState $GameDirectory $gameRoot $snapshotDirectory @(Get-CheekyOpenXrEntries)
        Write-State $statePath $state
    }

    $currentCheeky = @(Get-CheekyOpenXrEntries)
    $state.CheekyEntries = @(Merge-RegistryEntries @($state.CheekyEntries) $currentCheeky)
    $quarantineDirectory = Join-Path ([string]$state.SnapshotDirectory) 'quarantine'
    if (-not (Test-Path -LiteralPath $quarantineDirectory)) {
        New-Item -ItemType Directory -Path $quarantineDirectory -Force | Out-Null
    }

    try {
        if (@($currentCheeky | Where-Object { [string]$_.Key -match '^HKLM:' }).Count -gt 0) {
            Ensure-Administrator 'Apply'
        }

        $knownOriginals = @($state.Moves | ForEach-Object { [string]$_.Original })
        foreach ($candidate in @(Get-CleanCandidates $GameDirectory)) {
            if (@($knownOriginals | Where-Object { $_ -ieq [string]$candidate.Path }).Count -gt 0) {
                continue
            }
            $record = Move-ToQuarantine $GameDirectory $quarantineDirectory $candidate
            if ($record) {
                $state.Moves = @($state.Moves) + @($record)
                $knownOriginals += [string]$record.Original
                Write-State $statePath $state
            }
        }

        [void](Apply-BaselineIni $iniPath)
        Disable-CheekyOpenXr $currentCheeky
        $state.Status = 'Applied'
        $state.AppliedAt = (Get-Date).ToString('o')
        Write-State $statePath $state
        Write-Info ''
        Write-Ok 'Baseline ERVR + VDXR + OFXR aplicado.'
        Write-Info 'OFXR nao foi atualizado nem modificado; confira o relatorio antes de armar o bridge.'
        [void](Write-BaselineReport $GameDirectory $gameRoot 'APPLIED')
    } catch {
        if ($newApply) {
            try {
                [void](Restore-MoveRecords @($state.Moves))
                if (Test-Path -LiteralPath (Join-Path ([string]$state.SnapshotDirectory) 'ERVR.ini')) {
                    Copy-Item -LiteralPath (Join-Path ([string]$state.SnapshotDirectory) 'ERVR.ini') -Destination $iniPath -Force
                }
                Restore-CheekyOpenXr @($state.CheekyEntries)
                $state.Status = 'Failed'
                $state.FailedAt = (Get-Date).ToString('o')
                Write-State $statePath $state
            } catch {
                Write-Warn ("Rollback automatico incompleto: {0}" -f $_.Exception.Message)
            }
        }
        throw
    }
}

function Restore-Baseline {
    param([string]$GameDirectory)

    Assert-GameClosed
    $gameRoot = Get-GameRoot $GameDirectory
    $baselineRoot = Join-Path $gameRoot '_MODDIN_BACKUPS\ERVR-OFXR-Baseline'
    $statePath = Join-Path $baselineRoot 'active-state.json'
    $state = Read-State $statePath
    if (-not $state) {
        throw ("Estado anterior nao encontrado: {0}" -f $statePath)
    }

    if (@($state.CheekyEntries | Where-Object { [string]$_.Key -match '^HKLM:' }).Count -gt 0) {
        Ensure-Administrator 'Restore'
    }

    $warnings = @(Restore-MoveRecords @($state.Moves))
    $snapshotIni = Join-Path ([string]$state.SnapshotDirectory) 'ERVR.ini'
    if (Test-Path -LiteralPath $snapshotIni -PathType Leaf) {
        Copy-Item -LiteralPath $snapshotIni -Destination ([string]$state.IniPath) -Force
        Write-Ok ("ERVR.ini restaurado: {0}" -f $state.IniPath)
    } else {
        $warnings += "Snapshot do ERVR.ini nao encontrado: $snapshotIni"
    }

    Restore-CheekyOpenXr @($state.CheekyEntries)
    $state.Status = 'Restored'
    $state.RestoredAt = (Get-Date).ToString('o')
    Write-State $statePath $state
    foreach ($warning in $warnings) {
        Write-Warn $warning
    }
    [void](Write-BaselineReport $GameDirectory $gameRoot 'RESTORED')
    Write-Ok 'Estado anterior restaurado. Nenhum arquivo foi apagado.'
}

try {
    $gameDirectory = Find-GameDirectory $GamePath
    if (-not $gameDirectory) {
        throw 'Nao consegui localizar ELDEN RING\Game. Passe a pasta Game como argumento do .cmd.'
    }

    Write-Info ("Pasta Game: {0}" -f $gameDirectory)
    switch ($Mode) {
        'Verify' {
            $gameRoot = Get-GameRoot $gameDirectory
            [void](Write-BaselineReport $gameDirectory $gameRoot 'VERIFIED')
        }
        'Apply' {
            Apply-Baseline $gameDirectory
        }
        'Restore' {
            Restore-Baseline $gameDirectory
        }
    }
    exit 0
} catch {
    Write-Err ("Falha: {0}" -f $_.Exception.Message)
    exit 1
}
