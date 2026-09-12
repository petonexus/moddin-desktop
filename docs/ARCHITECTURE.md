# Moddin architecture

## Goal

Moddin should behave like a small declarative package manager for game mods and tooling, not as a collection of per-game scripts.

A game definition describes **what is supported**. Reusable modules describe **how a capability is applied**. The native desktop layer performs privileged/local operations and records every destructive change so it can be reversed.

## Layers

### 1. Vue / TypeScript UI

Responsibilities:

- show installed games;
- match them against the catalog;
- expose available modules and profiles;
- show planned changes before applying them;
- display transaction history and rollback controls.

The UI must not contain game-specific installation logic.

### 2. Declarative catalog

Game entries live as YAML and contain stable identifiers such as Steam App IDs, executable paths, and enabled modules.

Example:

```yaml
id: elden-ring
name: Elden Ring
steamAppId: "1245620"
executable: Game/eldenring.exe
modules:
  - id: obs-vr
    category: vr
    status: planned
```

The catalog will later grow into versioned module recipes with compatibility metadata, checksums, conflicts, and per-game overrides.

### 3. Tauri native layer

Rust is intentionally kept narrow. It owns operations that benefit from native access:

- Steam/library discovery;
- filesystem operations;
- process inspection;
- Windows Registry access where necessary;
- downloads and hash verification;
- backup/restore transactions;
- integration with OBS and external tooling.

The majority of product/UI logic remains TypeScript.

### 4. Action engine

The action engine will expose a small set of composable primitives instead of embedding arbitrary scripts everywhere:

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

Every mutating operation should belong to a transaction. A transaction records:

- game/module identifiers;
- timestamp;
- files created;
- files replaced;
- backups created;
- commands executed;
- resulting status.

This makes `Undo` a first-class feature rather than an afterthought.

## Initial vertical slice

The bootstrap branch implements the first complete read-only path:

`Steam libraries -> installed games -> catalog match -> game detail UI`

The next vertical slice is:

`Elden Ring/Cyberpunk -> OBS VR module -> preview changes -> backup -> apply -> rollback`

After OBS is stable, the same engine can be reused for OptiScaler, OpenXR helpers, ReShade, UE4SS, BepInEx, REFramework, and normal QoL mods.
