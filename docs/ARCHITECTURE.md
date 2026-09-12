# Moddin architecture

## Goal

Moddin should behave like a small declarative package manager for game mods and tooling, not as a collection of per-game scripts.

A game definition describes **what is supported**. Reusable modules describe **how a capability is applied**. The native desktop layer performs privileged/local operations and records destructive changes so they can be reversed.

## Layers

### 1. Vue / TypeScript UI

Responsibilities:

- show installed games;
- match them against the catalog;
- expose available modules and profiles;
- show planned changes before applying them;
- display transaction history and rollback controls.

The UI must not contain game-specific installation logic. It dispatches module actions using catalog configuration plus the selected installed game.

### 2. Declarative catalog

Game entries live as YAML and contain stable identifiers such as Steam App IDs, executable paths, enabled modules, and small module-specific overrides.

Example:

```yaml
id: elden-ring
name: Elden Ring
steamAppId: "1245620"
executable: Game/eldenring.exe
modules:
  - id: obs-vr
    category: vr
    status: available
    config:
      collectionName: Sem nome
      sceneName: vr
      sourceName: Elden Ring VR
      executableName: eldenring.exe
```

The catalog will later grow into versioned module recipes with compatibility metadata, checksums, conflicts, and per-game overrides.

### 3. Tauri native layer

Rust is intentionally kept narrow. It owns operations that benefit from native/local access:

- Steam/library discovery;
- Windows Registry discovery fallbacks;
- filesystem operations;
- process inspection/lifecycle integration;
- downloads and hash verification;
- backup/restore transactions;
- integration with OBS and external tooling.

The majority of product/UI logic remains TypeScript.

### 4. Action engine

The next generalization step is a small set of composable primitives instead of embedding arbitrary scripts everywhere:

- `backup`
- `copy`
- `move`
- `delete`
- `download`
- `extract`
- `verify_hash`
- `patch_ini`
- `patch_json`
- `run`
- `wait_process`

PowerShell remains an escape hatch for exceptional Windows-specific cases, not the default execution model.

### 5. Transactions and rollback

The first transaction store is already implemented under `%LOCALAPPDATA%/Moddin/transactions`.

A transaction currently records:

- transaction id and timestamp;
- game/module kind;
- human-readable label;
- target file;
- backup file;
- applied/rolled-back status.

The first consumer is the OBS module, which backs up the complete scene collection before editing it. The UI exposes transaction history and Undo. OBS-aware rollback closes OBS gracefully before restoring and reopens it afterwards so OBS cannot overwrite the restored collection on exit.

Future action-engine transactions will extend this model to multiple files, files created by a module, hashes, commands, and structured per-step results.

## Implemented vertical slices

### Library discovery

`Windows Registry / Steam roots -> Steam libraries -> installed games -> catalog match -> game detail UI`

### VR-ready launch profiles

`Elden Ring/Cyberpunk -> launch recipe -> executable/VR files/OpenXR preflight -> declared graphics settings -> backup -> launch -> transaction history`

Launch recipes are declarative. They define required VR markers, launch arguments, optional INI patches, and safety notes. The native layer never accepts an arbitrary absolute config path: relative paths are resolved below the game installation and only existing keys named by the recipe are changed. A missing OpenXR runtime, missing VR integration, or running game blocks launch.

Cyberpunk's current VR Port owns its first-launch `UserSettings.json` migration, so Moddin intentionally does not compete with it. Elden Ring's ERVR recipe applies the documented conservative starting values when the existing `Game/ERVR/ERVR.ini` contains those keys.

### First safe mutation: OBS VR

`Elden Ring/Cyberpunk -> OBS VR recipe -> preview -> graceful OBS close -> full collection backup -> apply -> validate -> reopen -> transaction history -> Undo`

The OBS module clones a Game Capture already present in the target scene. This preserves the user's transform and capture-related settings instead of inventing a generic source layout.

After this flow is validated against real OBS data, the action engine will be generalized and reused for OptiScaler, OpenXR helpers, ReShade, UE4SS, BepInEx, REFramework, and normal QoL mods.
