# Game support matrix

Generated from the six `src/catalog/games/*.yaml` files on 2026-09-29. The
catalog is the single source of truth; if this table and the YAML disagree, the
YAML wins and this file is stale.

`vr-launch` is called out separately because it is the difference between
"the VR toolchain is installable" and "the game starts in VR on its own". Only
Elden Ring and Cyberpunk 2077 have the profile today.

| Game | Store | Store App ID | Engine preset | Executable | `vr-launch` | Modules |
| --- | --- | --- | --- | --- | --- | --- |
| Cyberpunk 2077 | Steam | 1091500 | redengine | `bin/x64/Cyberpunk2077.exe` | yes | VR launch profile: available; OBS VR Capture: available; OptiScaler: available; Cheeky Foveated DLSS: available; OFXR FrameGen: available; OpenXR tools: planned; UEVR engine-aware installer: available |
| The Blood of Dawnwalker | Steam | 3751260 | unreal5 | `Dawnwalker/Binaries/Win64/Dawnwalker.exe` | no | Criar atalho na Área de Trabalho: available; Captura OBS VR: available; OptiScaler: available; OFXR FrameGen: available; Instalador UEVR com detecção de engine: available; UE4SS e pacote QoL: planned; Perfil gráfico MFG + Ray Reconstruction: planned; Migração de mods manuais para Vortex: planned |
| Dead Island 2 | Epic Games Store | Crow | unreal5 | `DeadIsland/Binaries/Win64/DeadIsland-Win64-Shipping.exe` | no | Create Desktop Shortcut: available; OBS VR Capture: available; OptiScaler: available; OFXR FrameGen: available; UEVR Engine-Aware Installer: available; Cheeky Foveated DLSS for UEVR: planned |
| DOOM (2016) | Steam | 379720 | idtech | `DOOMx64.exe` | no | Criar atalho na Area de Trabalho: available; KHARVOX VR: planned |
| Elden Ring | Steam | 1245620 | — (no preset declared) | `Game/eldenring.exe` | yes | VR launch profile: available; OBS VR Capture: available; OptiScaler: available; Cheeky Foveated DLSS: available; OFXR FrameGen: available; OpenXR tools: planned; UEVR engine-aware installer: available |
| S.T.A.L.K.E.R. 2: Heart of Chornobyl | Steam | 1643320 | unreal5 | `Stalker2/Binaries/Win64/Stalker2-Win64-Shipping.exe` | no | Criar atalho na Área de Trabalho: available; Captura OBS VR: available; OFXR FrameGen: available; Instalador UEVR com detecção de engine: available; Cheeky Foveated DLSS para UEVR: planned; OptiScaler: available; ReShade host: planned |

## Notes

- **Dead Island 2** is Unreal Engine 5. It previously declared
  `enginePreset: re-engine`, which contradicted both its own UEVR module and
  the `re-engine` preset's note that UEVR does not target that engine.
  Corrected to `unreal5` on 2026-09-29.
- **Elden Ring** declares no `enginePreset` at all. It is the most-used entry
  in the catalog and the only one missing an engine declaration.
- **`Crow`** is the Epic manifest `AppName`, not a numeric app id.
- **DOOM (2016)** is the smallest entry: a desktop shortcut and a planned
  KHARVOX integration. It is the one title in the catalog that is not a
  VR-focused workflow.
- **`unity.yaml`** is an engine preset with no game using it, and it is the
  only preset that declares `bepinex`. BepInEx is therefore installable for
  Unity games that Moddin does not yet catalogue.

## Reusable requirements

- A desktop shortcut action must only accept the catalogued executable below the
  selected game directory, preview its target/icon, and create a transaction so
  replacing or creating the `.lnk` can be undone.
- Game-specific mutations stay `planned` until they have native transaction
  support. Several catalogued modules (Cheeky's UEVR plugin path, the
  Dawnwalker MFG/Ray Reconstruction profile, the Vortex migration) are
  game-specific scripts that have no reusable transaction-backed module yet.
- Nothing validates a wrong `executable` or a wrong `enginePreset` at CI time.
  Both mistakes are silent and both have reached this file at least once.
