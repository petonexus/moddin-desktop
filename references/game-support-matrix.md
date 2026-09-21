# Game support matrix

| Game | Store | Store App ID | Executable | Modules | Notes |
| --- | --- | --- | --- | --- | --- |
| DOOM (2016) | Steam | 379720 | DOOMx64.exe | Desktop Shortcut: available; KHARVOX VR: planned | Steam manifest and executable path verified locally. The shortcut action uses the game executable as both launch target and icon source and records an undo transaction. KHARVOX 0.8 Beta remains an externally managed launcher until Moddin gains a managed external-launcher capability. |
| The Blood of Dawnwalker | Steam | 3751260 | Dawnwalker/Binaries/Win64/Dawnwalker.exe | Desktop Shortcut: available; OBS VR: available; OptiScaler: available; OFXR FrameGen: available; UEVR: available; UE4SS/QoL: planned; MFG/Ray Reconstruction profile: planned; Vortex migration: planned | Steam manifest, install layout, executable and Unreal Engine 5 layout verified locally. The existing Dawnwalker scripts cover UE4SS/Vortex migration, OptiScaler presets and reversible MFG/Ray Reconstruction changes; Moddin currently exposes the reusable modules and keeps those game-specific mutations planned until they have native transaction support. |
| S.T.A.L.K.E.R. 2: Heart of Chornobyl | Steam | 1643320 | Stalker2/Binaries/Win64/Stalker2-Win64-Shipping.exe | Desktop Shortcut: available; OBS VR: available; OFXR FrameGen: available; UEVR: available; Cheeky Foveated DLSS for UEVR: planned | Steam manifest, executable and Unreal layout verified locally. The external Cheeky scanner found a READY UEVR + native DLSS path and an existing UEVR profile; the current Moddin Cheeky module manages only the ReShade add-on variant, so the UEVR plugin path remains planned. |
| Dead Island 2 | Epic Games Store | Crow | DeadIsland/Binaries/Win64/DeadIsland-Win64-Shipping.exe | Desktop Shortcut: available; OBS VR: available; OptiScaler: available; OFXR FrameGen: available; UEVR: available; Cheeky Foveated DLSS for UEVR: planned | Epic manifest, install location, executable and Unreal layout verified locally. `Crow` is the Epic manifest `AppName`; the game-specific Cheeky UEVR transaction remains planned until it is represented by a native reusable module. |

## Reusable requirements

- A desktop shortcut action must only accept the catalogued executable below the selected game directory, preview its target/icon, and create a transaction so replacing or creating the .lnk can be undone.
