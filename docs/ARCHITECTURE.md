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
- OpenXR runtime discovery and launch-time overrides;
- persistent structured action history;
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

Available modules also expose a read-only verification pass. It reports the concrete prerequisites and current state separately from whether the action is merely available. Every mutating path refreshes this verification after applying, reinstalling, launching, or removing a module.

The OptiScaler action already uses the multi-file form: it records replaced and created files, created directories, process safety metadata, and source metadata. OBS removal and VR profile changes use the same transaction store so the module lifecycle remains reversible.

### 6. Persistent action history

Developer diagnostics and user-visible action history are intentionally separate systems.

`src/debug.ts` keeps a bounded in-memory diagnostic stream for frontend/Tauri troubleshooting. The persistent activity store is reserved for actions that change state or launch a configured game. The `invokeDebug` wrapper records the outcome of those mutating commands without making logging part of the success criteria for the action itself.

Persistent entries are JSON Lines records under `%LOCALAPPDATA%/Moddin/logs/actions.jsonl`. The store:

- records success or failure, action name, timestamp, game id, transaction id and compact structured details;
- rotates the current file at 5 MB, keeping one previous generation;
- validates payload size before accepting frontend-originated records;
- exposes read and clear commands to the desktop UI;
- never fails a game/mod mutation merely because the diagnostic write failed.

The Activity panel can search/filter this history independently from the transaction screen. Transactions remain the source of truth for rollback; action logs explain **what Moddin attempted and how it ended**.

## Implemented vertical slices

### Library discovery

`Windows Registry / Steam roots -> Steam libraries -> installed games -> catalog match -> game detail UI`

The environment inspection also reads bounded executable bytes and checks engine-specific game layout markers. It returns the engine, a coarse confidence level, and the evidence used. UEVR consumes this result as a hard Unreal Engine gate; an unknown engine is never treated as Unreal.

### OpenXR runtime management

`Windows OpenXR registry -> discover installed runtime manifests -> validate runtime libraries -> choose global or per-game runtime -> VR launch`

The OpenXR manager understands two deliberately separate scopes:

- **Windows global runtime** — backed by `HKLM\SOFTWARE\Khronos\OpenXR\1\ActiveRuntime`. Changing it is an explicit action and uses an administrator UAC prompt.
- **Per-game override** — stored by Moddin under `%LOCALAPPDATA%\Moddin\profiles\openxr`. It never changes the Windows global runtime. When Moddin launches that game it injects `XR_RUNTIME_JSON` only into the child process.

This separation makes it possible to keep, for example, SteamVR as the system default while launching a specific game through another installed OpenXR runtime. A stale per-game manifest is ignored and reported as a warning rather than silently breaking launch.

Runtime discovery combines the Khronos `AvailableRuntimes` registry entries with known default manifest locations for common Windows runtimes. Every candidate manifest is parsed and its runtime library is checked before the UI enables selection.

### VR-ready launch profiles

`Elden Ring/Cyberpunk -> launch recipe -> executable/VR files/OpenXR preflight -> declared graphics settings -> backup -> launch -> transaction history`

Launch recipes are declarative. They define required VR markers, launch arguments, optional INI patches, and safety notes. The native layer never accepts an arbitrary absolute config path: relative paths are resolved below the game installation and only existing keys named by the recipe are changed. A missing OpenXR runtime, missing VR integration, or running game blocks launch.

If a per-game OpenXR override exists, the launch preflight reports it as the effective runtime and the child process receives `XR_RUNTIME_JSON`. Otherwise the validated Windows `ActiveRuntime` is used.

Cyberpunk's current VR Port owns its first-launch `UserSettings.json` migration, so Moddin intentionally does not compete with it. Elden Ring's ERVR recipe applies the documented conservative starting values when the existing `Game/ERVR/ERVR.ini` contains those keys.

### Engine-aware UEVR

`selected game -> local engine detection -> backend/release resolution -> safe ZIP extraction -> tool transaction`

UEVR recipes declare the release API, version policy, optional pinned tag, and backend candidates. The native layer resolves the current official release at install time, verifies the UEVR Nightly checksum sidecar, refuses unsafe archive paths, and stores Nightly, JoeyHodge, AFW, and combined variants in separate user-tool directories. Community backend assets without an upstream checksum are recorded with their calculated hash and shown as such; they are never silently treated as locally proven compatibility.

### First safe mutation: OBS VR

`Elden Ring/Cyberpunk -> OBS VR recipe -> preview -> graceful OBS close -> full collection backup -> apply -> validate -> reopen -> transaction history -> Undo`

The OBS module clones a Game Capture already present in the target scene. This preserves the user's transform and capture-related settings instead of inventing a generic source layout.

The same safety model is now being reused across OptiScaler, OpenXR helpers, future ReShade support, UE4SS, BepInEx, REFramework, and normal QoL mods.
