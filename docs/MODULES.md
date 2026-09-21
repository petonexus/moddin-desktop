# Moddin Desktop — module design notes

This document captures the **design intent** of the four reusable modules
still pending from `ROADMAP.md` (v0.4 — mod frameworks and graphics
primitives) and the conventions any new module must follow. It is the
hand-off document between roadmap entries and concrete implementations.

## Shared contract

Every module eventually implements the Rust trait in
[`src-tauri/src/module.rs`](../src-tauri/src/module.rs):

```rust
pub trait Module: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn category(&self) -> ModuleCategory;
    fn preview(&self, context: &ModuleContext) -> Result<PreviewReport, String>;
    fn apply(&self, context: &ModuleContext) -> Result<ApplyResult, String>;
    fn verify(&self, context: &ModuleContext) -> Result<VerificationReport, String>;
    fn rollback(&self, transaction_id: &str) -> Result<TransactionRecord, String>;
    fn remove(&self, context: &ModuleContext) -> Result<ApplyResult, String>;
    fn update_check(&self, context: &ModuleContext) -> Result<UpdateInfo, String>;
}
```

Existing modules (`obs`, `optiscaler`, `openxr`, `ofxr`, `uevr`, `cheeky`,
`vr_launch`, `desktop_shortcut`) do **not** implement this trait yet —
adopting it is an incremental per-module refactor tracked here.

## Catalog integration

Each module is enabled per-game through the YAML catalog
(`src/catalog/games/*.yaml`) by listing it under `modules:`. Per-game
entries override preset entries with the same id; engine presets
(`src/catalog/engines/*.yaml`) supply defaults for the rest.

---

## ReShade — `reshade`

**Goal.** Install a ReShade build compatible with the selected game, with
optional add-ons (Cheeky Foveated DLSS is the first add-on supported
through a separate module). ReShade is the de-facto post-processing
injector for PC games; Moddin already has Cheeky as a ReShade add-on, but
the host ReShade install is currently expected to be present.

**State.** Tracked in `ROADMAP.md` (v0.3 — ReShade). The Cheeky module
(`src-tauri/src/cheeky.rs`) already validates SHA-256 and writes the
add-on next to the host; the host itself is not yet managed.

**Upstream feed.** ReShade ships official releases at
`https://www.reShade.org` and mirrors on GitHub for the add-on API.
Versioning follows semver; `versionPolicy: latest` with an
`updateUrl: https://api.github.com/repos/crosire/reshade/releases/latest`
should work for add-on discovery.

**Configuration surface.**
- `versionPolicy: latest | pinned`
- `releaseApiUrl: <GitHub releases URL>`
- `pinnedTag: <tag name>` (when versionPolicy is `pinned`)
- `sha256: <expected digest>` (when versionPolicy is `pinned`)
- `proxyCandidates: dxgi.dll,version.dll,d3d11.dll,opengl32.dll` (which
  ReShade-provided proxy DLL to copy next to the game executable)
- `effects: <add-on file list>` — optional add-ons to install together
  with the host (Cheeky is excluded here; it has its own module)
- `addons: <path>` — destination folder for `.addon64` files
- `iniPreset: <path>` — optional ReShade preset to copy to the game
  directory
- `safetyNotes:` — see the existing `cheeky` config for the shape

**State markers.** Persist a `.moddin-reshade.json` marker under
`%LOCALAPPDATA%/Moddin/tools/reshade/<gameId>/` recording the installed
host version, add-on list, installed files, and SHA-256.

**Safety rules.**
- ReShade must not be installed next to the executable if an EAC/BE
  runtime is detected, unless the user has explicitly opted in. The
  catalog recipe's `safetyNotes` must call this out (mirroring the
  Elden Ring baseline).
- Add-ons without a verifiable SHA-256 (or `compatibilityStatus:
  unverified` in the catalog) are shown in the UI as experimental.
- ReShade's `dxgi.dll` proxy conflicts with OptiScaler when the latter
  is also installed as `dxgi.dll`. The catalog should refuse to enable
  both unless the per-game recipe specifies a non-conflicting proxy
  (e.g. `version.dll` for OptiScaler).

**Files.**
- Native: `src-tauri/src/reshade.rs`
- Types: reuse the existing `cheeky`-shaped `Request/Preview/Result`
  structs and the `ToolModuleDefinition` schema in `src/types/game.ts`
- Catalog: a new `reshade` module entry per game that opts in

---

## UE4SS — `ue4ss`

**Goal.** Install BepInEx-style scripting for Unreal Engine 4.20–5.4
games. Moddin ships UEVR for VR injection but does not yet ship the
scripting framework that the most popular QoL mods (more markers,
auto-loot, journal, time tracker, uncapped cutscenes) depend on.

**State.** Planned in `ROADMAP.md` (v0.4 — UE4SS). Several games in the
catalog already advertise a planned `ue4ss` entry
(`dawnwalker.yaml` has the most explicit one); the catalogue can move it
to `available` only once the module ships.

**Upstream feed.** `https://api.github.com/repos/UE4SS-RE/RE-UE4SS/releases/latest`.
Versioning is semver with build metadata; releases carry a SHA-256
sidecar.

**Configuration surface.**
- `versionPolicy: latest | pinned`
- `releaseApiUrl: <URL>` (default above)
- `pinnedTag: <tag>` (when pinned)
- `targetEngine: <UE 4.x | UE 5.x>` — UE4SS branches differ across major
  versions; the install must pick the matching archive
- `proxyCandidates: <list>` (UE4SS does not provide a proxy DLL, but
  some installs use `version.dll`)
- `mods: <list of zip URLs + sha256>` — optional QoL mod bundle the
  module can install in the same transaction (the per-game recipe
  decides what to bundle)
- `safetyNotes:` — UE4SS cannot coexist with EAC; the recipe must warn.

**State markers.** Persist `.moddin-ue4ss.json` under
`%LOCALAPPDATA%/Moddin/tools/ue4ss/<gameId>/` with installed version,
target engine, mods installed, and SHA-256 of the host archive.

**Safety rules.**
- Reject installation if `inspection::GameEnvironmentInspection.engine`
  is not Unreal, or if the engine version is outside UE4SS's supported
  range. The recipe's `versionPolicy` is responsible for picking the
  right archive; the module refuses to install when the resolved
  archive does not match.
- UE4SS and UEVR share `dinput8.dll` as the loader on some configs.
  Recipes that enable both must declare the conflict resolution
  explicitly (typically UEVR owns `dinput8.dll` and UE4SS is loaded via
  `version.dll`).

**Files.**
- Native: `src-tauri/src/ue4ss.rs`
- Catalog: a new `ue4ss` module entry per game that opts in

---

## BepInEx — `bepinex`

**Goal.** Install BepInEx for Unity and .NET games. Moddin's catalog
already supports `optiscaler` and `cheeky-foveated-dlss` for Unity
games, but the scripting framework BepInEx hosts is not yet managed.

**State.** Planned in `ROADMAP.md` (v0.4 — BepInEx). No catalog recipe
references it yet; the design should land before any `available` entry
is added.

**Upstream feed.**
- Stable: `https://api.github.com/repos/BepInEx/BepInEx/releases/latest`
- Bleeding edge (for Unity IL2CPP previews):
  `https://api.github.com/repos/BepInEx/BepInEx/releases`

**Configuration surface.**
- `versionPolicy: latest | pinned`
- `releaseApiUrl: <URL>` (default above)
- `pinnedTag: <tag>` (when pinned)
- `unityArchitecture: il2cpp | mono` — BepInEx ships separate builds
  for each; the install must inspect the executable before resolving
- `mods: <list of plugin URLs + sha256>` — optional QoL plugin bundle
- `safetyNotes:`

**State markers.** Persist `.moddin-bepinex.json` under
`%LOCALAPPDATA%/Moddin/tools/bepinex/<gameId>/`.

**Safety rules.**
- BepInEx bundles its own `winhttp.dll` proxy. Recipes that also
  enable `optiscaler` must declare an explicit non-conflicting proxy
  for OptiScaler (`version.dll`).
- For IL2CPP games, refuse to install until the architecture is
  confirmed by inspecting the executable (the existing
  `inspection::inspect_game_environment` already returns engine and
  architecture hints — reuse it).

**Files.**
- Native: `src-tauri/src/bepinex.rs`
- Catalog: a new `bepinex` module entry per Unity game

---

## REFramework — `reframework`

**Goal.** Install REFramework for Capcom RE Engine games (Resident Evil
Village, Resident Evil 4 Remake, Monster Hunter Rise, Dragon's Dogma 2,
Dead Island 2). REFramework hosts scripting mods that don't fit
elsewhere.

**State.** Planned in `ROADMAP.md` (v0.4 — REFramework). No catalog
recipe currently opts in. The `re-engine` engine preset covers
Resident Evil / Dead Island games but does not advertise `reframework`
yet.

**Upstream feed.**
- `https://api.github.com/repos/praydog/REFramework-nightly/releases/latest`

**Configuration surface.**
- `versionPolicy: latest | pinned`
- `releaseApiUrl: <URL>`
- `pinnedTag: <tag>` (when pinned)
- `mods: <list of script URLs + sha256>` — optional mod bundle
- `safetyNotes:` — REFramework cannot coexist with EAC.

**State markers.** Persist `.moddin-reframework.json` under
`%LOCALAPPDATA%/Moddin/tools/reframework/<gameId>/`.

**Safety rules.**
- Refuse installation if the engine is not RE Engine. The
  `inspection::inspect_game_environment` result already detects this.
- Refuse installation if EAC is detected running (process snapshot via
  `crate::process::is_process_running`).

**Files.**
- Native: `src-tauri/src/reframework.rs`
- Catalog: add `reframework` to the `re-engine` preset and to specific
  RE Engine games (Dead Island 2 already exists; Resident Evil titles
  will need new catalog entries).

---

## Implementation order

1. **ReShade** — completes the VR pipeline (Cheeky already needs a host),
   the smallest surface, and the highest reuse across engines.
2. **BepInEx** — Unity games are common, the install flow is well
   understood, and the module mirrors ReShade's shape.
3. **UE4SS** — couples naturally with UEVR; the conflict rules with
   `dinput8.dll` are the main design work.
4. **REFramework** — gated behind RE Engine detection; ships after the
   `re-engine` preset stabilises.

Each module follows the same adoption pattern as F2's trait:
- introduce the native file under `src-tauri/src/<module>.rs`;
- register `preview_*` / `install_*` / `uninstall_*` commands in
  `lib.rs`;
- add the catalog recipe(s) in YAML form;
- update `references/game-support-matrix.md` once a game is moved from
  `planned` to `available`.

## Adopting the `Module` trait

For each existing module, the refactor follows this skeleton:

```rust
pub struct MyModule;

impl Module for MyModule {
    fn id(&self) -> &'static str { "my-module" }
    fn name(&self) -> &'static str { "My Module" }
    fn category(&self) -> ModuleCategory { ModuleCategory::Vr }

    fn preview(&self, context: &ModuleContext) -> Result<PreviewReport, String> {
        // wrap the existing preview_* command body
        let preview = my_module::preview_my_module(context.into_request()?)?;
        Ok(preview.into())
    }

    fn apply(&self, context: &ModuleContext) -> Result<ApplyResult, String> {
        // wrap the existing install_*/configure_* body and record the transaction
        let result = my_module::install_my_module(context.into_request()?)?;
        Ok(ApplyResult {
            transaction: result.transaction,
            installed: result.installed,
            started: result.started,
            armed: result.armed,
            version: result.version,
            backend: result.backend,
        })
    }

    // …
}
```

The Tauri command shim keeps its current signature so the Vue layer is
unchanged; the trait object is only used internally (e.g. by a future
generic UI dispatcher).