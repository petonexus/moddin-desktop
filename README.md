# Moddin Desktop

[![Repository](https://img.shields.io/badge/github-petonexus%2Fmoddin--desktop-181717?logo=github)](https://github.com/petonexus/moddin-desktop)
[![License: GPL-3.0](https://img.shields.io/github/license/petonexus/moddin-desktop)](https://github.com/petonexus/moddin-desktop/blob/main/LICENSE)

**Moddin Desktop** is a Windows-first desktop mod and tooling manager focused on making PC game modding repeatable, reversible, and easy to maintain.

Instead of keeping one-off PowerShell installers per game, Moddin Desktop detects installed games, matches them against a declarative catalog, previews supported actions, creates a backup transaction, applies the change, and lets the user undo it later.

See [`docs/SCOPE.md`](docs/SCOPE.md) for what is and isn't inside this project, and [`tools/README.md`](tools/README.md) for the standalone Windows tools that ship alongside the app.

## Stack

- Tauri 2
- Vue 3
- TypeScript
- Rust only for native Windows/filesystem/process integration
- YAML game catalog validated with Zod and discovered automatically at build time

## Current MVP

The first vertical slice already includes:

- Steam install discovery, including Windows Registry fallback
- multiple Steam libraries
- installed-game scanning
- Elden Ring and Cyberpunk 2077 catalog entries
- reusable OBS VR Capture module
- preview before applying OBS changes
- automatic backup transaction before mutation
- transaction history and Undo
- graceful OBS close/reopen when needed
- read-only verification checklist for each available module
- idempotent reapply and transaction-backed removal for OBS VR, OptiScaler, and VR profiles
- VR-ready launch profiles for Elden Ring and Cyberpunk 2077
- OFXR Bridge FrameGen integration: verified install, tray startup, recommended optical-flow configuration, OpenXR arm confirmation, and safe removal
- engine-aware UEVR installer: local engine detection, live official Nightly resolution, optional JoeyHodge/AFW variants, SHA-256 verification where upstream publishes it, and rollback
- one-click Windows development bootstrap
- Windows CI for frontend build and Rust tests

### OBS VR workflow

For the first supported module, Moddin looks for the configured OBS scene (`vr` by default), clones an existing Game Capture source to preserve its transform/settings, targets the game executable, and stores a full backup of the scene collection before writing anything.

The initial catalog recipes are:

- **Elden Ring** → `Elden Ring VR` / `eldenring.exe`
- **Cyberpunk 2077** → `Cyberpunk 2077 VR` / `Cyberpunk2077.exe`

The defaults currently target the OBS collection `Sem nome` and scene `vr`; the backend falls back to a unique collection containing that scene when possible. These values will become user-editable presets later.

### VR-ready launch workflow

The game detail view now exposes a VR launch profile for Elden Ring and Cyberpunk 2077. Moddin checks the executable, required VR files, active Windows OpenXR runtime, and whether the game is already running before launch. It then applies only catalog-declared INI changes with a backup. Cyberpunk leaves the VR Port first-launch settings to the mod itself; Elden Ring uses the documented full-stereo baseline with `GameResScale=1.0`, `FpsTarget=60`, `CameraBob=0`, quad/body HUD, and debug logging.

The standalone reversible baseline package is available under [`tools/elden-ring-ervr-ofxr-baseline`](tools/elden-ring-ervr-ofxr-baseline). It verifies the existing ReShade/ERVR/OpenXR/OFXR chain, quarantines identified experimental extras without deleting them, and restores the previous state through `02-RESTAURAR-ESTADO-ANTERIOR.cmd`. It never updates or reconfigures OFXR and never changes anti-cheat.

The launcher does not disable Easy Anti-Cheat or select an OpenXR runtime on the user's behalf. Elden Ring must be started through the user's existing offline, EAC-disabled mod setup.

### UEVR workflow

The UEVR module inspects the selected executable and game layout before it resolves a release. Only games detected as Unreal Engine can proceed; Unity, RE Engine, REDengine, and unknown engines remain blocked. The selected backend is resolved at install time: official UEVR Nightly, Nightly plus JoeyHodge's backend, PureDark AFW, or the combined JoeyHodge/AFW archive. Nightly archives are checked against the upstream `.sha256` asset. Each variant lives in its own `%LOCALAPPDATA%/Moddin/tools/uevr` folder and is transaction-backed.

Per-game recipes can pin the only known-good build with `versionPolicy: pinned` and `releaseTag`, or keep `versionPolicy: latest` to follow the newest release. Backend compatibility labels in the catalog are evidence/status metadata, not a claim that Moddin has tested the game; a local VR test is still required.

### OFXR FrameGen workflow

The explicit OFXR module can prepare OFXR Bridge before starting either supported game. It downloads the pinned official pre-release archive, verifies its SHA-256, installs it under the user profile instead of the game directory, writes the recommended FidelityFX configuration, starts `OFXRBridgeTray.exe`, and confirms that the manual OpenXR layer is armed for the current Windows user. If activation cannot be confirmed, the game is not launched. The module can also be configured and armed from its own preview, checked for new pre-releases, and removed through the transaction history. The Elden Ring baseline package deliberately does not invoke this install/update path; it only inspects the existing OFXR state.

OFXR Bridge is experimental. It requires the Microsoft Visual C++ Redistributable x64 and the game must run without administrator privileges because Windows OpenXR does not apply per-user implicit layers to elevated processes. The tray can remain in the notification area while the game is running; Moddin disables configuration, verification, update, removal, undo, and launch actions until the game closes.

### Module lifecycle

Available modules are checked automatically when a game is selected and can be checked again manually at any time. The checklist distinguishes a module that is ready to apply from one that is already configured, reports individual failed prerequisites, and is refreshed after every apply, reinstall, launch, or removal. Removal is transaction-backed: OBS removes only the Moddin-created source, OptiScaler restores its managed files, and VR profiles restore the last declared INI changes.

The module cards also check configured update sources. Versioned tools such as OptiScaler are compared with the latest official GitHub release, while recipes without a trustworthy local version signal remain explicitly marked as unable to identify the installed version instead of claiming they are current.

## Development

On Windows, after cloning the repository, run:

```bat
SETUP-WINDOWS.bat
```

The bootstrap checks or installs Node.js, Rust/Cargo using the MSVC toolchain, Microsoft C++ Build Tools, npm dependencies, and validates the Tauri project. After the environment is ready, use:

```bat
RODAR-MODDIN.bat
```

or:

```powershell
npm run tauri dev
```

Game recipes under `src/catalog/games/*.yaml` are loaded automatically. Adding support for another game should not require registering a new TypeScript import; the catalog loader also rejects duplicate catalog IDs and Steam App IDs.

For the complete local validation and frontend conventions, see [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) and [`docs/FRONTEND.md`](docs/FRONTEND.md).

### Debugging

During development, Moddin logs lifecycle events and every Tauri `invoke` call to the WebView console. Runtime errors and rejected promises are also captured in an on-screen diagnostic panel instead of leaving a blank window.

The diagnostic API is available from the WebView console:

```js
window.__MODDIN_DEBUG__.logs()
window.__MODDIN_DEBUG__.enable()
window.__MODDIN_DEBUG__.disable()
window.__MODDIN_DEBUG__.clear()
```

`enable()` persists verbose logging in the current WebView profile. Each log entry includes an ISO timestamp, scope, duration for native calls, and the related error or payload.

## Project direction

Game-specific support should live in catalog recipes rather than hard-coded UI branches. Reusable modules such as OBS, OptiScaler, OpenXR, ReShade, UE4SS, BepInEx, and REFramework will be shared across games and composed from declarative configuration.

See [`ROADMAP.md`](ROADMAP.md), [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md), and [`docs/FRONTEND.md`](docs/FRONTEND.md) for the current plan.
