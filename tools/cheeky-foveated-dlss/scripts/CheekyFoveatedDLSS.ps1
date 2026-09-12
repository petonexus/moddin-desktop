[CmdletBinding()]
param(
    [ValidateSet('Scan', 'Install', 'Uninstall')]
    [string]$Action = 'Scan',

    [switch]$IncludeExperimental,
    [switch]$SkipOpenXRSetup,

    [string]$Stalker2Path,
    [string]$DeadIsland2Path,
    [string]$Cyberpunk2077Path,
    [string]$EldenRingPath
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RootDir = Split-Path -Parent $ScriptDir
$LogsDir = Join-Path $RootDir 'logs'
$BackupsDir = Join-Path $RootDir 'backups'
$CacheDir = Join-Path $RootDir '.cache'
$StateDir = Join-Path $RootDir 'state'
$StateFile = Join-Path $StateDir 'cheeky-install-state.json'
$RunId = Get-Date -Format 'yyyyMMdd-HHmmss'
$LogFile = Join-Path $LogsDir ("cheeky-{0}-{1}.log" -f $Action.ToLowerInvariant(), $RunId)

@($LogsDir, $BackupsDir, $CacheDir, $StateDir) | ForEach-Object {
    if (-not (Test-Path $_)) { New-Item -ItemType Directory -Path $_ -Force | Out-Null }
}

function Write-Log {
    param(
        [string]$Message,
        [ValidateSet('INFO','OK','WARN','ERROR','DEBUG')]
        [string]$Level = 'INFO'
    )

    $line = '[{0}] [{1}] {2}' -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'), $Level, $Message
    Add-Content -LiteralPath $LogFile -Value $line -Encoding UTF8

    switch ($Level) {
        'OK'    { Write-Host $Message -ForegroundColor Green }
        'WARN'  { Write-Host $Message -ForegroundColor Yellow }
        'ERROR' { Write-Host $Message -ForegroundColor Red }
        'DEBUG' { Write-Host $Message -ForegroundColor DarkGray }
        default { Write-Host $Message }
    }
}

function Normalize-PathSafe {
    param([string]$Path)
    if ([string]::IsNullOrWhiteSpace($Path)) { return $null }
    try { return [IO.Path]::GetFullPath($Path.Trim('"')) } catch { return $Path.Trim('"') }
}

function Get-FileProductText {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return '' }
    try {
        $v = (Get-Item -LiteralPath $Path).VersionInfo
        return (($v.ProductName, $v.FileDescription, $v.CompanyName, $v.OriginalFilename) -join ' ')
    } catch {
        return ''
    }
}

function Test-ReShadeAddonHost {
    param([string]$ExeDir, [switch]$TrustERVR)

    if (-not (Test-Path -LiteralPath $ExeDir)) { return $false }

    if ($TrustERVR -and (Test-Path -LiteralPath (Join-Path $ExeDir 'ERVR\ERVR.dll'))) {
        if (Test-Path -LiteralPath (Join-Path $ExeDir 'ReShade.ini')) { return $true }
    }

    $candidates = @('dxgi.dll','d3d11.dll','d3d12.dll','ReShade64.dll')
    foreach ($name in $candidates) {
        $path = Join-Path $ExeDir $name
        if (-not (Test-Path -LiteralPath $path)) { continue }
        $text = Get-FileProductText -Path $path
        if ($text -match '(?i)reshade') { return $true }
    }

    if ((Test-Path -LiteralPath (Join-Path $ExeDir 'ReShade.ini')) -and
        (Test-Path -LiteralPath (Join-Path $ExeDir 'ReShade.log'))) {
        return $true
    }

    return $false
}

function Test-AnyReShade {
    param([string]$ExeDir)
    return (Test-ReShadeAddonHost -ExeDir $ExeDir) -or
           (Test-Path -LiteralPath (Join-Path $ExeDir 'ReShade.ini')) -or
           (Test-Path -LiteralPath (Join-Path $ExeDir 'ReShadePreset.ini'))
}

function Get-SteamLibraries {
    $roots = New-Object System.Collections.Generic.List[string]
    $steamRoot = $null

    foreach ($regPath in @('HKCU:\Software\Valve\Steam', 'HKLM:\SOFTWARE\WOW6432Node\Valve\Steam')) {
        try {
            $p = Get-ItemProperty -Path $regPath -ErrorAction Stop
            foreach ($prop in @('SteamPath','InstallPath')) {
                if ($p.PSObject.Properties.Name -contains $prop -and $p.$prop) {
                    $steamRoot = Normalize-PathSafe $p.$prop
                    if ($steamRoot) { $roots.Add($steamRoot) }
                }
            }
        } catch {}
    }

    if (-not $steamRoot) {
        $fallback = Join-Path ${env:ProgramFiles(x86)} 'Steam'
        if (Test-Path -LiteralPath $fallback) {
            $steamRoot = $fallback
            $roots.Add($fallback)
        }
    }

    if ($steamRoot) {
        $vdf = Join-Path $steamRoot 'steamapps\libraryfolders.vdf'
        if (Test-Path -LiteralPath $vdf) {
            $raw = Get-Content -LiteralPath $vdf -Raw -ErrorAction SilentlyContinue
            if ($raw) {
                foreach ($m in [regex]::Matches($raw, '"path"\s+"([^"]+)"')) {
                    $path = $m.Groups[1].Value -replace '\\\\','\'
                    $path = Normalize-PathSafe $path
                    if ($path) { $roots.Add($path) }
                }
            }
        }
    }

    return $roots | Where-Object { $_ -and (Test-Path -LiteralPath $_) } | Select-Object -Unique
}

function Get-SteamInstalls {
    $installs = @()
    foreach ($lib in (Get-SteamLibraries)) {
        $steamapps = Join-Path $lib 'steamapps'
        if (-not (Test-Path -LiteralPath $steamapps)) { continue }

        foreach ($manifest in (Get-ChildItem -LiteralPath $steamapps -Filter 'appmanifest_*.acf' -File -ErrorAction SilentlyContinue)) {
            try {
                $raw = Get-Content -LiteralPath $manifest.FullName -Raw
                $nameM = [regex]::Match($raw, '(?m)^\s*"name"\s+"([^"]+)"')
                $dirM  = [regex]::Match($raw, '(?m)^\s*"installdir"\s+"([^"]+)"')
                if (-not $nameM.Success -or -not $dirM.Success) { continue }
                $root = Join-Path (Join-Path $steamapps 'common') $dirM.Groups[1].Value
                if (Test-Path -LiteralPath $root) {
                    $installs += [pscustomobject]@{ Store='Steam'; Name=$nameM.Groups[1].Value; Root=(Normalize-PathSafe $root) }
                }
            } catch {}
        }
    }
    return $installs
}

function Get-EpicInstalls {
    $installs = @()
    $manifestDir = Join-Path $env:ProgramData 'Epic\EpicGamesLauncher\Data\Manifests'
    if (-not (Test-Path -LiteralPath $manifestDir)) { return $installs }

    foreach ($file in (Get-ChildItem -LiteralPath $manifestDir -Filter '*.item' -File -ErrorAction SilentlyContinue)) {
        try {
            $j = Get-Content -LiteralPath $file.FullName -Raw | ConvertFrom-Json
            if ($j.DisplayName -and $j.InstallLocation -and (Test-Path -LiteralPath $j.InstallLocation)) {
                $installs += [pscustomobject]@{ Store='Epic'; Name=[string]$j.DisplayName; Root=(Normalize-PathSafe ([string]$j.InstallLocation)) }
            }
        } catch {}
    }
    return $installs
}

function Get-GogInstalls {
    $installs = @()
    foreach ($base in @('HKLM:\SOFTWARE\WOW6432Node\GOG.com\Games', 'HKLM:\SOFTWARE\GOG.com\Games')) {
        if (-not (Test-Path $base)) { continue }
        foreach ($key in (Get-ChildItem $base -ErrorAction SilentlyContinue)) {
            try {
                $p = Get-ItemProperty $key.PSPath
                $name = $null
                foreach ($prop in @('gameName','GAMENAME')) {
                    if ($p.PSObject.Properties.Name -contains $prop -and $p.$prop) { $name = [string]$p.$prop; break }
                }
                $root = $null
                foreach ($prop in @('path','PATH')) {
                    if ($p.PSObject.Properties.Name -contains $prop -and $p.$prop) { $root = [string]$p.$prop; break }
                }
                if ($name -and $root -and (Test-Path -LiteralPath $root)) {
                    $installs += [pscustomobject]@{ Store='GOG'; Name=$name; Root=(Normalize-PathSafe $root) }
                }
            } catch {}
        }
    }
    return $installs
}

function Find-Exe {
    param([string]$Root, [string[]]$RelativePaths, [string]$FileName)
    foreach ($rel in $RelativePaths) {
        $candidate = Join-Path $Root $rel
        if (Test-Path -LiteralPath $candidate -PathType Leaf) { return (Normalize-PathSafe $candidate) }
    }
    try {
        $found = Get-ChildItem -LiteralPath $Root -Filter $FileName -File -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($found) { return $found.FullName }
    } catch {}
    return $null
}

function Find-DlssDll {
    param([string]$Root, [string[]]$PreferredPaths)
    foreach ($rel in $PreferredPaths) {
        $p = Join-Path $Root $rel
        if (Test-Path -LiteralPath $p -PathType Leaf) { return $p }
    }
    try {
        $found = Get-ChildItem -LiteralPath $Root -Filter 'nvngx_dlss.dll' -File -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($found) { return $found.FullName }
    } catch {}
    return $null
}

function Find-UevrConfig {
    param([string]$ExePath, [string]$GameId)
    if (-not $env:APPDATA) { return $null }
    $base = Join-Path $env:APPDATA 'UnrealVRMod'
    if (-not (Test-Path -LiteralPath $base)) { return $null }

    $exeBase = [IO.Path]::GetFileNameWithoutExtension($ExePath)
    $exact = Join-Path $base $exeBase
    if (Test-Path -LiteralPath $exact) { return $exact }

    $tokens = switch ($GameId) {
        'stalker2'    { @('stalker2','stalker') }
        'deadisland2' { @('deadisland','dead island') }
        default       { @($exeBase) }
    }

    foreach ($dir in (Get-ChildItem -LiteralPath $base -Directory -ErrorAction SilentlyContinue)) {
        $n = $dir.Name.ToLowerInvariant()
        foreach ($t in $tokens) {
            if ($n -like ('*' + $t.ToLowerInvariant().Replace(' ','') + '*') -or $n -like ('*' + $t.ToLowerInvariant() + '*')) {
                return $dir.FullName
            }
        }
    }
    return $null
}

function Test-OptiScaler {
    param([string]$ExeDir)
    $markers = @('OptiScaler.ini','OptiScaler.log','OptiScaler.dll','OptiScalerSetup.exe','optiscaler.ini')
    foreach ($m in $markers) {
        if (Test-Path -LiteralPath (Join-Path $ExeDir $m)) { return $true }
    }
    try {
        $hit = Get-ChildItem -LiteralPath $ExeDir -File -ErrorAction SilentlyContinue | Where-Object { $_.Name -match '(?i)optiscaler' } | Select-Object -First 1
        if ($hit) { return $true }
    } catch {}
    return $false
}

function Test-CyberpunkVrPort {
    param([string]$Root, [string]$ExeDir)
    $newPlugin = Join-Path $Root 'red4ext\plugins\CyberpunkVR_Stereo\CyberpunkVR_Stereo.dll'
    if (Test-Path -LiteralPath $newPlugin) { return 'Plugin' }

    $legacyMarkers = @(
        (Join-Path $ExeDir 'vrport.ini'),
        (Join-Path $ExeDir 'cyberpunkvrport.log'),
        (Join-Path $Root 'red4ext\plugins\CyberpunkVR_Hands\CyberpunkVR_Hands.dll')
    )
    foreach ($m in $legacyMarkers) {
        if (Test-Path -LiteralPath $m) { return 'LegacyProxyOrFork' }
    }
    return 'None'
}

function Test-ERVR {
    param([string]$ExeDir)
    return (Test-Path -LiteralPath (Join-Path $ExeDir 'ERVR\ERVR.dll'))
}

function Test-ERSSFG {
    param([string]$ExeDir)
    return (Test-Path -LiteralPath (Join-Path $ExeDir 'ERSS-FG.dll')) -and
           (Test-Path -LiteralPath (Join-Path $ExeDir 'ERSS2\bin\nvngx_dlss.dll'))
}

function Get-GameScan {
    $allInstalls = @()
    $allInstalls += Get-SteamInstalls
    $allInstalls += Get-EpicInstalls
    $allInstalls += Get-GogInstalls

    $defs = @(
        [pscustomobject]@{
            Id='stalker2'; Name='STALKER 2'; Match='(?i)(S\.T\.A\.L\.K\.E\.R\.\s*2|STALKER\s*2)';
            Override=$Stalker2Path; Exe='Stalker2-Win64-Shipping.exe'; ExeRel=@('Stalker2\Binaries\Win64\Stalker2-Win64-Shipping.exe');
            PreferredDlss=@('Engine\Plugins\Marketplace\DLSS\Binaries\ThirdParty\Win64\nvngx_dlss.dll','Stalker2\Binaries\Win64\nvngx_dlss.dll')
        },
        [pscustomobject]@{
            Id='deadisland2'; Name='Dead Island 2'; Match='(?i)^Dead Island 2$';
            Override=$DeadIsland2Path; Exe='DeadIsland-Win64-Shipping.exe'; ExeRel=@('DeadIsland\Binaries\Win64\DeadIsland-Win64-Shipping.exe');
            PreferredDlss=@('DeadIsland\Binaries\Win64\nvngx_dlss.dll')
        },
        [pscustomobject]@{
            Id='cyberpunk2077'; Name='Cyberpunk 2077'; Match='(?i)^Cyberpunk 2077$';
            Override=$Cyberpunk2077Path; Exe='Cyberpunk2077.exe'; ExeRel=@('bin\x64\Cyberpunk2077.exe');
            PreferredDlss=@('bin\x64\nvngx_dlss.dll')
        },
        [pscustomobject]@{
            Id='eldenring'; Name='ELDEN RING'; Match='(?i)^ELDEN RING$';
            Override=$EldenRingPath; Exe='eldenring.exe'; ExeRel=@('Game\eldenring.exe','eldenring.exe');
            PreferredDlss=@('Game\ERSS2\bin\nvngx_dlss.dll','ERSS2\bin\nvngx_dlss.dll')
        }
    )

    $results = @()
    foreach ($def in $defs) {
        $matches = @()
        if ($def.Override) {
            $matches += [pscustomobject]@{ Store='Manual'; Name=$def.Name; Root=(Normalize-PathSafe $def.Override) }
        } else {
            $matches += $allInstalls | Where-Object { $_.Name -match $def.Match }
        }

        if (-not $matches) {
            $results += [pscustomobject]@{
                Id=$def.Id; Name=$def.Name; Found=$false; Store=''; Root=''; Exe=''; ExeDir=''; Integration=''; Status='NOT_FOUND';
                Experimental=$false; Reason='Jogo nao encontrado nas bibliotecas Steam/Epic/GOG.'; UevrConfig=''; DlssDll=''; Notes=@()
            }
            continue
        }

        $install = $matches | Select-Object -First 1
        $exe = Find-Exe -Root $install.Root -RelativePaths $def.ExeRel -FileName $def.Exe
        if (-not $exe) {
            $results += [pscustomobject]@{
                Id=$def.Id; Name=$def.Name; Found=$true; Store=$install.Store; Root=$install.Root; Exe=''; ExeDir=''; Integration=''; Status='BLOCKED';
                Experimental=$false; Reason='Instalacao encontrada, mas o executavel esperado nao foi localizado.'; UevrConfig=''; DlssDll=''; Notes=@()
            }
            continue
        }

        $exeDir = Split-Path -Parent $exe
        $dlss = Find-DlssDll -Root $install.Root -PreferredPaths $def.PreferredDlss
        $status = 'BLOCKED'
        $experimental = $false
        $integration = ''
        $reason = ''
        $notes = New-Object System.Collections.Generic.List[string]
        $uevr = ''

        switch ($def.Id) {
            'stalker2' {
                $uevr = Find-UevrConfig -ExePath $exe -GameId $def.Id
                $integration = 'UEVR'
                if (-not $uevr) {
                    $reason = 'UEVR config nao encontrada. Injete/import o perfil no jogo pelo menos uma vez antes de instalar Cheeky.'
                } elseif (-not $dlss) {
                    $reason = 'DLSS nativo nao foi encontrado nos arquivos do jogo.'
                } elseif (Test-AnyReShade -ExeDir $exeDir) {
                    $reason = 'ReShade detectado no jogo. O plugin UEVR do Cheeky exige remover ReShade desta instalacao; o script nao remove mods automaticamente.'
                } else {
                    $status = 'READY'
                    $reason = 'UEVR + DLSS nativo + OpenXR: melhor caminho para Cheeky.'
                }
                $notes.Add('Use Native Stereo no UEVR primeiro.')
            }
            'deadisland2' {
                $uevr = Find-UevrConfig -ExePath $exe -GameId $def.Id
                $integration = 'UEVR'
                $opti = Test-OptiScaler -ExeDir $exeDir
                if (-not $uevr) {
                    $reason = 'UEVR config nao encontrada. Injete/import o perfil de Dead Island 2 pelo menos uma vez.'
                } elseif (-not $opti) {
                    $reason = 'Dead Island 2 nao tem DLSS nativo; OptiScaler nao foi detectado.'
                } elseif (-not $dlss) {
                    $reason = 'OptiScaler foi detectado, mas nvngx_dlss.dll nao foi encontrado. Sem uma avaliacao DLSS real, Cheeky nao tem o que fovear.'
                } elseif (Test-AnyReShade -ExeDir $exeDir) {
                    $reason = 'ReShade detectado. Para a integracao UEVR do Cheeky, remova ReShade primeiro ou trate manualmente o encadeamento.'
                } else {
                    $status = 'EXPERIMENTAL_READY'
                    $experimental = $true
                    $reason = 'UEVR + OptiScaler + DLSS injetado detectados. Estruturalmente compativel, mas sem validacao oficial Cheeky + OptiScaler para este jogo.'
                }
                $notes.Add('No jogo, o OptiScaler precisa estar realmente usando DLSS SR; apenas ter a DLL no disco nao basta.')
            }
            'cyberpunk2077' {
                $integration = 'ReShade'
                $vrPort = Test-CyberpunkVrPort -Root $install.Root -ExeDir $exeDir
                $reshade = Test-ReShadeAddonHost -ExeDir $exeDir
                $dxgi = Join-Path $exeDir 'dxgi.dll'
                $dxgiText = Get-FileProductText -Path $dxgi

                if ($vrPort -eq 'None') {
                    $reason = 'Cyberpunk VR Port nao foi detectado. O script nao instala o mod VR, apenas o Cheeky.'
                } elseif ($vrPort -eq 'LegacyProxyOrFork' -and (Test-Path -LiteralPath $dxgi) -and $dxgiText -notmatch '(?i)reshade') {
                    $reason = 'Um VR proxy antigo/fork parece ocupar dxgi.dll. Nao vou substituir ou encadear esse hook automaticamente.'
                } elseif (-not $dlss) {
                    $reason = 'DLSS nativo do Cyberpunk nao foi localizado.'
                } elseif (-not $reshade) {
                    $reason = 'O VR Port atual deixa o slot dxgi livre, mas ReShade com full add-on support nao foi detectado. Instale-o primeiro para usar Cheeky fora do UEVR.'
                } else {
                    $status = 'EXPERIMENTAL_READY'
                    $experimental = $true
                    $reason = 'Cyberpunk VR Port + DLSS nativo + ReShade add-on host detectados. O pipeline e promissor, mas ainda nao aparece na lista oficial de jogos validados do Cheeky.'
                }
                $notes.Add('O Cyberpunk VR Port atual renderiza uma segunda view real e trata NGX/DLSS por view; isso e o que torna o teste interessante.')
            }
            'eldenring' {
                $integration = 'ReShade'
                $ervr = Test-ERVR -ExeDir $exeDir
                $erss = Test-ERSSFG -ExeDir $exeDir
                $reshade = Test-ReShadeAddonHost -ExeDir $exeDir -TrustERVR
                if (-not $ervr) {
                    $reason = 'Ilya ERVR nao foi detectado. O script foi desenhado para o mod OpenXR/ReShade atual, nao para R.E.A.L. VR antigo.'
                } elseif (-not $reshade) {
                    $reason = 'ERVR foi detectado, mas o host ReShade com add-on support nao foi confirmado.'
                } elseif (-not $erss) {
                    $reason = 'Elden Ring base nao tem DLSS. ERSS-FG + ERSS2\\bin\\nvngx_dlss.dll nao foram detectados.'
                } else {
                    $status = 'EXPERIMENTAL_READY'
                    $experimental = $true
                    $reason = 'ERVR (ReShade/OpenXR) + ERSS-FG + DLSS detectados. Cheeky pode ser carregado pelo mesmo ReShade, mas esta combinacao ainda e experimental.'
                }
                $notes.Add('OFFLINE ONLY: ERVR exige EAC desabilitado; este script nao toca no anti-cheat.')
                $notes.Add('Se usar OptiScaler junto, prefira version.dll para nao sobrescrever o dxgi.dll do ReShade/ERVR.')
            }
        }

        $results += [pscustomobject]@{
            Id=$def.Id; Name=$def.Name; Found=$true; Store=$install.Store; Root=$install.Root; Exe=$exe; ExeDir=$exeDir;
            Integration=$integration; Status=$status; Experimental=$experimental; Reason=$reason; UevrConfig=$uevr; DlssDll=$dlss; Notes=@($notes)
        }
    }
    return $results
}

function Get-CheekyRelease {
    Write-Log 'Consultando ultima release estavel do CheekyFoveatedDLSS...' 'INFO'
    $headers = @{ 'User-Agent'='moddin-cheeky-installer'; 'Accept'='application/vnd.github+json' }
    $release = Invoke-RestMethod -Uri 'https://api.github.com/repos/ClarkCheekyKent/CheekyFoveatedDLSS/releases/latest' -Headers $headers -UseBasicParsing
    if (-not $release -or -not $release.tag_name) { throw 'Nao foi possivel obter a release do Cheeky.' }
    return $release
}

function Get-ReleaseAsset {
    param($Release, [string]$NameRegex)
    $asset = $Release.assets | Where-Object { $_.name -match $NameRegex } | Select-Object -First 1
    if (-not $asset) { throw "Asset nao encontrado na release $($Release.tag_name): $NameRegex" }
    return $asset
}

function Download-Asset {
    param($Asset)
    $dest = Join-Path $CacheDir $Asset.name
    $headers = @{ 'User-Agent'='moddin-cheeky-installer' }
    Write-Log ("Baixando {0}..." -f $Asset.name) 'INFO'
    Invoke-WebRequest -Uri $Asset.browser_download_url -OutFile $dest -UseBasicParsing -Headers $headers

    if ($Asset.PSObject.Properties.Name -contains 'digest' -and $Asset.digest -and ([string]$Asset.digest) -match '^sha256:([0-9a-fA-F]+)$') {
        $expected = $Matches[1].ToLowerInvariant()
        $actual = (Get-FileHash -LiteralPath $dest -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $expected) {
            Remove-Item -LiteralPath $dest -Force -ErrorAction SilentlyContinue
            throw "SHA256 invalido para $($Asset.name). Esperado $expected, obtido $actual."
        }
        Write-Log ("SHA256 OK: {0}" -f $Asset.name) 'OK'
    } else {
        Write-Log ("A release nao forneceu digest verificavel para {0}; download mantido sem hash oficial." -f $Asset.name) 'WARN'
    }
    return $dest
}

function Load-State {
    if (-not (Test-Path -LiteralPath $StateFile)) {
        return [pscustomobject]@{ Schema=1; Targets=@(); LastVersion='' }
    }
    try {
        $state = Get-Content -LiteralPath $StateFile -Raw | ConvertFrom-Json
        if (-not ($state.PSObject.Properties.Name -contains 'Targets')) { $state | Add-Member -NotePropertyName Targets -NotePropertyValue @() }
        return $state
    } catch {
        Write-Log 'State existente esta corrompido; preservando arquivo e iniciando um novo state.' 'WARN'
        Copy-Item -LiteralPath $StateFile -Destination ($StateFile + '.broken-' + $RunId) -Force
        return [pscustomobject]@{ Schema=1; Targets=@(); LastVersion='' }
    }
}

function Save-State {
    param($State)
    $State | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $StateFile -Encoding UTF8
}

function Get-TrackedFileRecord {
    param($State, [string]$Path)
    foreach ($t in @($State.Targets)) {
        foreach ($f in @($t.Files)) {
            if ([string]::Equals([string]$f.Path, $Path, [StringComparison]::OrdinalIgnoreCase)) { return $f }
        }
    }
    return $null
}

function Protect-TargetFile {
    param($State, [string]$GameId, [string]$Path)
    $tracked = Get-TrackedFileRecord -State $State -Path $Path
    if ($tracked) { return $tracked }

    $backupPath = ''
    $existed = Test-Path -LiteralPath $Path -PathType Leaf
    if ($existed) {
        $folder = Join-Path (Join-Path $BackupsDir $RunId) $GameId
        if (-not (Test-Path -LiteralPath $folder)) { New-Item -ItemType Directory -Path $folder -Force | Out-Null }
        $backupPath = Join-Path $folder ([IO.Path]::GetFileName($Path))
        $n = 1
        while (Test-Path -LiteralPath $backupPath) {
            $backupPath = Join-Path $folder (([IO.Path]::GetFileNameWithoutExtension($Path)) + "-$n" + ([IO.Path]::GetExtension($Path)))
            $n++
        }
        Copy-Item -LiteralPath $Path -Destination $backupPath -Force
        Write-Log ("Backup: {0} -> {1}" -f $Path, $backupPath) 'INFO'
    }

    return [pscustomobject]@{ Path=$Path; OriginalExisted=$existed; BackupPath=$backupPath }
}

function Upsert-StateTarget {
    param($State, $Game, [string]$Version, [string]$Integration, [object[]]$FileRecords)
    $others = @($State.Targets | Where-Object { $_.GameId -ne $Game.Id })
    $target = [pscustomobject]@{
        GameId=$Game.Id; GameName=$Game.Name; Version=$Version; Integration=$Integration; InstalledAt=(Get-Date).ToString('o');
        Root=$Game.Root; Exe=$Game.Exe; UevrConfig=$Game.UevrConfig; Files=@($FileRecords)
    }
    $State.Targets = @($others + $target)
    $State.LastVersion = $Version
}

function Install-UevrTarget {
    param($State, $Game, [string]$ZipPath, [string]$Version)
    $temp = Join-Path $CacheDir ('uevr-' + $Game.Id + '-' + $RunId)
    if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force }
    New-Item -ItemType Directory -Path $temp -Force | Out-Null
    Expand-Archive -LiteralPath $ZipPath -DestinationPath $temp -Force

    $map = @(
        @{ Name='CheekyFoveatedDLSS.dll'; Rel='plugins\CheekyFoveatedDLSS.dll' },
        @{ Name='CheekyFoveatedDLSSRuntime.dll'; Rel='plugins\CheekyFoveatedDLSS\CheekyFoveatedDLSSRuntime.dll' },
        @{ Name='cheeky_foveated_dlss.lua'; Rel='scripts\cheeky_foveated_dlss.lua' }
    )

    $records = @()
    foreach ($entry in $map) {
        $source = Get-ChildItem -LiteralPath $temp -Filter $entry.Name -File -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
        if (-not $source) { throw "Arquivo $($entry.Name) nao encontrado no ZIP UEVR." }
        $dest = Join-Path $Game.UevrConfig $entry.Rel
        $destDir = Split-Path -Parent $dest
        if (-not (Test-Path -LiteralPath $destDir)) { New-Item -ItemType Directory -Path $destDir -Force | Out-Null }
        $records += Protect-TargetFile -State $State -GameId $Game.Id -Path $dest
        Copy-Item -LiteralPath $source.FullName -Destination $dest -Force
        Write-Log ("Instalado: {0}" -f $dest) 'OK'
    }

    Upsert-StateTarget -State $State -Game $Game -Version $Version -Integration 'UEVR' -FileRecords $records
}

function Install-ReShadeTarget {
    param($State, $Game, [string]$AddonPath, [string]$Version)
    $dest = Join-Path $Game.ExeDir 'CheekyFoveatedDLSS.addon64'
    $record = Protect-TargetFile -State $State -GameId $Game.Id -Path $dest
    Copy-Item -LiteralPath $AddonPath -Destination $dest -Force
    Write-Log ("Instalado: {0}" -f $dest) 'OK'
    Upsert-StateTarget -State $State -Game $Game -Version $Version -Integration 'ReShade' -FileRecords @($record)
}

function Get-InstalledOpenXRVersion {
    $roots = @(
        'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*',
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*',
        'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*'
    )
    foreach ($root in $roots) {
        try {
            $app = Get-ItemProperty $root -ErrorAction SilentlyContinue | Where-Object { $_.DisplayName -like 'Cheeky OpenXR Support*' } | Select-Object -First 1
            if ($app) { return [string]$app.DisplayVersion }
        } catch {}
    }
    return ''
}

function Install-OpenXRSupport {
    param($Release)
    if ($SkipOpenXRSetup) {
        Write-Log 'Cheeky OpenXR Support ignorado por -SkipOpenXRSetup.' 'WARN'
        return
    }

    $wanted = ([string]$Release.tag_name).TrimStart('v')
    $installed = Get-InstalledOpenXRVersion
    if ($installed -and $installed -eq $wanted) {
        Write-Log ("Cheeky OpenXR Support {0} ja instalado." -f $installed) 'OK'
        return
    }

    $asset = Get-ReleaseAsset -Release $Release -NameRegex '^CheekyOpenXRSetup\.exe$'
    $setup = Download-Asset -Asset $asset
    Write-Log 'Abrindo o instalador oficial do Cheeky OpenXR Support. Conclua a janela exibida.' 'INFO'
    $p = Start-Process -FilePath $setup -Wait -PassThru
    if ($p.ExitCode -eq 0) {
        Write-Log 'Cheeky OpenXR Support concluido.' 'OK'
    } else {
        Write-Log ("Cheeky OpenXR Setup terminou com codigo {0}. A calibracao automatica OpenXR pode nao funcionar." -f $p.ExitCode) 'WARN'
    }
}

function Uninstall-CheekyTargets {
    $state = Load-State
    if (-not $state.Targets -or @($state.Targets).Count -eq 0) {
        Write-Log 'Nenhuma instalacao do Cheeky registrada por este script.' 'WARN'
        return
    }

    foreach ($target in @($state.Targets)) {
        Write-Log ("Removendo Cheeky de {0}..." -f $target.GameName) 'INFO'
        foreach ($f in @($target.Files)) {
            if ($f.OriginalExisted -and $f.BackupPath -and (Test-Path -LiteralPath $f.BackupPath)) {
                $parent = Split-Path -Parent $f.Path
                if (-not (Test-Path -LiteralPath $parent)) { New-Item -ItemType Directory -Path $parent -Force | Out-Null }
                Copy-Item -LiteralPath $f.BackupPath -Destination $f.Path -Force
                Write-Log ("Restaurado backup: {0}" -f $f.Path) 'OK'
            } else {
                if (Test-Path -LiteralPath $f.Path) {
                    Remove-Item -LiteralPath $f.Path -Force
                    Write-Log ("Removido: {0}" -f $f.Path) 'OK'
                }
            }
        }
    }

    Remove-Item -LiteralPath $StateFile -Force -ErrorAction SilentlyContinue
    Write-Log 'Arquivos por jogo removidos/restaurados. O Cheeky OpenXR Support global foi mantido porque pode ser compartilhado com outros jogos.' 'WARN'
    Write-Log 'Se quiser remover a camada global, use Configuracoes do Windows > Aplicativos > Cheeky OpenXR Support.' 'INFO'
}

function Show-Scan {
    param([object[]]$Games, [string]$ReleaseTag)
    Write-Host ''
    Write-Host 'Cheeky Foveated DLSS - verificador Moddin' -ForegroundColor Cyan
    if ($ReleaseTag) { Write-Host ("Release Cheeky detectada: {0}" -f $ReleaseTag) }
    Write-Host ('-' * 78)

    foreach ($g in $Games) {
        $color = switch ($g.Status) {
            'READY'              { 'Green' }
            'EXPERIMENTAL_READY' { 'Yellow' }
            'NOT_FOUND'          { 'DarkGray' }
            default              { 'Red' }
        }
        Write-Host ("[{0}] {1}" -f $g.Status, $g.Name) -ForegroundColor $color
        if ($g.Found) {
            Write-Host ("  Loja: {0}" -f $g.Store)
            Write-Host ("  EXE:  {0}" -f $g.Exe)
            Write-Host ("  Integracao Cheeky: {0}" -f $g.Integration)
            if ($g.UevrConfig) { Write-Host ("  UEVR: {0}" -f $g.UevrConfig) }
            if ($g.DlssDll) { Write-Host ("  DLSS: {0}" -f $g.DlssDll) }
        }
        Write-Host ("  {0}" -f $g.Reason)
        foreach ($note in @($g.Notes)) { Write-Host ("  - {0}" -f $note) -ForegroundColor DarkGray }
        Write-Host ''
    }
}

try {
    Write-Log ("Acao: {0}; IncludeExperimental={1}" -f $Action, $IncludeExperimental.IsPresent) 'INFO'

    if ($Action -eq 'Uninstall') {
        Uninstall-CheekyTargets
        Write-Host ''
        Write-Host ("Log: {0}" -f $LogFile)
        exit 0
    }

    $gpuNames = @()
    try { $gpuNames = @(Get-CimInstance Win32_VideoController -ErrorAction Stop | Select-Object -ExpandProperty Name) } catch {}
    if ($gpuNames.Count -gt 0) {
        Write-Log ("GPU(s): {0}" -f ($gpuNames -join ' | ')) 'INFO'
        if (-not ($gpuNames -match '(?i)NVIDIA.*RTX')) {
            Write-Log 'Nenhuma NVIDIA RTX detectada. Cheeky Foveated DLSS requer uma GPU RTX.' 'ERROR'
            exit 2
        }
    } else {
        Write-Log 'Nao consegui consultar a GPU; continuando a verificacao.' 'WARN'
    }

    $release = Get-CheekyRelease
    $games = @(Get-GameScan)
    Show-Scan -Games $games -ReleaseTag ([string]$release.tag_name)

    $reportPath = Join-Path $LogsDir ("scan-{0}.json" -f $RunId)
    $games | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $reportPath -Encoding UTF8
    Write-Log ("Relatorio JSON: {0}" -f $reportPath) 'INFO'

    if ($Action -eq 'Scan') {
        Write-Host 'Nenhum arquivo foi alterado.' -ForegroundColor Cyan
        Write-Host ("Log: {0}" -f $LogFile)
        exit 0
    }

    $targets = @($games | Where-Object {
        $_.Status -eq 'READY' -or ($IncludeExperimental -and $_.Status -eq 'EXPERIMENTAL_READY')
    })

    if ($targets.Count -eq 0) {
        Write-Log 'Nenhum alvo esta pronto para instalacao com as regras atuais.' 'WARN'
        if (-not $IncludeExperimental) {
            Write-Log 'Use -IncludeExperimental para permitir os candidatos experimentais que passaram em todos os pre-requisitos.' 'INFO'
        }
        exit 0
    }

    Write-Host ''
    Write-Host 'Alvos que serao modificados:' -ForegroundColor Cyan
    foreach ($t in $targets) {
        $flag = if ($t.Experimental) { ' [EXPERIMENTAL]' } else { '' }
        Write-Host ("  - {0}{1} via {2}" -f $t.Name, $flag, $t.Integration)
    }
    Write-Host ''

    $state = Load-State
    $uevrTargets = @($targets | Where-Object { $_.Integration -eq 'UEVR' })
    $reshadeTargets = @($targets | Where-Object { $_.Integration -eq 'ReShade' })

    $uevrZip = $null
    $addon = $null
    if ($uevrTargets.Count -gt 0) {
        $asset = Get-ReleaseAsset -Release $release -NameRegex '^CheekyFoveatedDLSS-.*-UEVR-release\.zip$'
        $uevrZip = Download-Asset -Asset $asset
    }
    if ($reshadeTargets.Count -gt 0) {
        $asset = Get-ReleaseAsset -Release $release -NameRegex '^CheekyFoveatedDLSS\.addon64$'
        $addon = Download-Asset -Asset $asset
    }

    foreach ($t in $targets) {
        if ($t.Integration -eq 'UEVR') {
            Install-UevrTarget -State $state -Game $t -ZipPath $uevrZip -Version ([string]$release.tag_name)
        } else {
            Install-ReShadeTarget -State $state -Game $t -AddonPath $addon -Version ([string]$release.tag_name)
        }
        Save-State -State $state
    }

    Install-OpenXRSupport -Release $release
    Save-State -State $state

    Write-Host ''
    Write-Host 'Instalacao concluida.' -ForegroundColor Green
    Write-Host 'No Quest 3: deixe Foveation center = Fixed, Automatic stereo alignment = ON e Automatic eye calibration = ON.'
    Write-Host 'Comece com os defaults e valide a borda vermelha nos dois olhos antes de apertar mais a fovea.'
    Write-Host ("Log: {0}" -f $LogFile)
    exit 0
}
catch {
    Write-Log ("Falha: {0}" -f $_.Exception.Message) 'ERROR'
    Write-Log $_.ScriptStackTrace 'DEBUG'
    Write-Host ''
    Write-Host ("Log: {0}" -f $LogFile)
    exit 1
}
