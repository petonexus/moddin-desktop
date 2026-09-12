# Moddin

**Moddin** is a Windows-first desktop mod and tooling manager focused on making PC game modding repeatable, reversible, and easy to maintain.

Instead of keeping one-off PowerShell installers per game, Moddin detects installed games, matches them against a declarative catalog, previews supported actions, creates a backup transaction, applies the change, and lets the user undo it later.

## Stack

- Tauri 2
- Vue 3
- TypeScript
- Rust only for native Windows/filesystem/process integration
- YAML game catalog validated with Zod

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
- one-click Windows development bootstrap
- Windows CI for frontend build and Rust tests

### OBS VR workflow

For the first supported module, Moddin looks for the configured OBS scene (`vr` by default), clones an existing Game Capture source to preserve its transform/settings, targets the game executable, and stores a full backup of the scene collection before writing anything.

The initial catalog recipes are:

- **Elden Ring** → `Elden Ring VR` / `eldenring.exe`
- **Cyberpunk 2077** → `Cyberpunk 2077 VR` / `Cyberpunk2077.exe`

The defaults currently target the OBS collection `Sem nome` and scene `vr`; the backend falls back to a unique collection containing that scene when possible. These values will become user-editable presets later.

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

```bash
npm run tauri dev
```

## Project direction

Game-specific support should live in catalog recipes rather than hard-coded UI branches. Reusable modules such as OBS, OptiScaler, OpenXR, ReShade, UE4SS, BepInEx, and REFramework will be shared across games and composed from declarative configuration.

See [`ROADMAP.md`](ROADMAP.md) and [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the current plan.
