# Capability contract

A **capability** is a data-driven recipe that declares everything a
mod needs to know about itself. The Moddin Desktop backend and the
desktop UI both consume the same YAML schema, so adding a new mod is
**one file plus one** catalog entry — no Rust code change is required
unless a new step / check kind is introduced.

This document is the authoritative spec; the Rust schema in
[`src-tauri/src/capability.rs`](../src-tauri/src/capability.rs) and
the TypeScript mirror in
[`src/types/capability.ts`](../src/types/capability.ts) are the
generated siblings of this doc.

## The promise

* The agent (LLM) only has to know the **capability id** and a
  **typed config** to install a mod on a chosen game.
* The UI renders the verification list **automatically** from the
  declared checks.
* The backend executes the install **step by step** through typed
  primitives; nothing is bespoke per mod unless the mod genuinely
  needs a new kind.
* Every destructive change still flows through the existing
  [`transaction`](../src-tauri/src/transaction.rs) store, so Undo
  works identically across capability-driven and bespoke modules.

## File location

```
src-tauri/capabilities/<capability-id>.yaml
```

The runner loads every YAML at process start via `include_str!`, so
adding a new capability is one file plus one `include_str!` line in
[`src-tauri/src/capability_runner.rs`](../src-tauri/src/capability_runner.rs).

## Top-level schema

```yaml
id: ofxr-bridge                    # unique; required
displayName: OFXR Bridge FrameGen  # human label; required
category: vr                       # vr | graphics | qol | system; required
status: available                  # available | planned; required
supportedEngines:                  # optional
  - unreal5
  - redengine
dependencies:                      # optional; ids of capabilities that
  - optiscaler                    # must be installed before this one
compatibility:                     # optional; game-build window, see
  gameExe: Game/Binaries/Win64/Game-Win64-Shipping.exe  # "Compatibility"
  minExeVersion: 1.16.0
configSchema:                      # optional; drives the UI form
  - name: downloadUrl
    type: url
    required: true
  - name: nvidiaPreset
    type: enum
    enumValues: [fast, medium, slow]
    default: medium
checks:    # optional; see "Checks" below
install:   # optional; see "Steps" below
uninstall: # optional; see "Steps" below
verify:    # optional; same shape as `checks`
safetyNotes:                      # optional; shown in the UI banner
  - "Experimental pre-release"
```

## Checks

```yaml
checks:
  - id: tray-process-running
    label: OFXRBridgeTray.exe is running
    kind: process-running
    severity: warning
    category: modulespecific       # global | category | modulespecific
    description: Helps the user understand what this row means.
    params:
      processName: OFXRBridgeTray.exe
      # OR: processNameField: configField    (pulls from ResolvedConfig.values)
```

Built-in check kinds (extend `builtin_checks.rs` to register more):

| kind              | What it evaluates                                        |
|-------------------|----------------------------------------------------------|
| `process-running` | `crate::process::is_process_running(processName)`         |
| `file-exists`     | `pathField` or `path` resolved against the game dir    |
| `file-absent`     | same, inverted                                           |
| `archive-reachable` | HTTPS HEAD against `urlField` or `url`                 |
| `archive-sha256`  | downloads + compares SHA-256 to `expectedField` / `expected` |
| `exe-version`     | reads the game exe `FileVersion` via PowerShell and compares it to the accepted window |

The `exe-version` kind accepts a `pathField` (config field) or `path`
(literal, relative to the game directory) plus any of `minVersion` /
`maxVersion` (inclusive bounds), `blockedVersions` (exact versions
that must never match), and `exactVersions` (whitelist — when
non-empty the detected version must equal one entry). Versions compare
with natural numeric ordering and tolerate prefixed tags, so
`1.16`, `1.16.0.0`, and `v1.16` are equivalent bounds; an exe whose
`FileVersion` cannot be determined fails the check.

```yaml
checks:
  - id: game-build-supported
    label: Game build is supported
    kind: exe-version
    severity: blocker
    category: modulespecific
    params:
      path: Game/Binaries/Win64/Game-Win64-Shipping.exe
      minVersion: 1.16.0
      maxVersion: 1.17.9
      blockedVersions: ["1.16.3.0"]
```

`severity` drives the UI badge colour and the `canApply` gate:

| severity   | Effect when failed                                          |
|------------|-------------------------------------------------------------|
| `info`     | Surfaced; does not block Apply.                              |
| `warning`  | Surfaced as a warning banner; does not block on its own.    |
| `blocker`  | Disables the Apply button until cleared.                     |

`category` drives the three-layer UI layout the desktop client
renders (Global / Category / Module-specific).

## Steps

```yaml
install:
  - kind: download-file
    description: Fetch the pinned release into Moddin's download cache.
    params:
      urlField: downloadUrl
      expectedField: sha256
      # A bare filename is stored under %LOCALAPPDATA%\Moddin\downloads,
      # so the archive never lands in the game folder. A name with a
      # directory component is resolved against the executable directory.
      target: ofxr-bridge.zip
  - kind: extract-zip
    description: Extract the OFXR-Bridge archive into the executable directory.
    params:
      # `archivePath` is a literal; `archivePathField` resolves to a
      # config field holding a path. Neither takes a URL — the download
      # step is what fetches it.
      archivePath: ofxr-bridge.zip
  - kind: write-text-file
    description: Render and write tray.ini.
    params:
      pathField: trayIni
      template: |
        [tray]
        backend={backend}
        nvidia_preset={nvidiaPreset}
```

Built-in step kinds (extend `builtin_steps.rs` to register more):

| kind               | What it does                                                |
|--------------------|-------------------------------------------------------------|
| `download-file`    | Fetches `urlField`/`url` (HTTPS only, host allow-list, size cap) into `targetField`/`target`, verifying SHA-256 when `expectedField`/`expected` is given. A bare `target` filename goes to Moddin's download cache, not the game folder. Not part of the rollback set. |
| `extract-zip`      | Reads a `zip` archive (`archivePath` literal or `archivePathField`) and extracts every member into the executable directory, sanitising paths. |
| `verify-hash`     | Reads `pathField` and compares SHA-256 to `expectedField`.    |
| `file-delete`     | Removes `pathField` (config field holding an absolute path). |
| `write-text-file` | Writes `pathField` with `template` rendered (`{field}` → `ResolvedConfig.values`). |
| `write-binary-file` | Same as `write-text-file` but for `base64` payload — useful for small DLLs and manifest blobs carried inline. |
| `move-file`       | Renames / moves a file inside the executable directory (used for picking a proxy DLL among the candidates). |
| `spawn-process`   | Starts `executable` (resolved against the game dir).         |
| `kill-process`    | Runs `taskkill /IM <name> /T` (optionally `/F`) so an install can stop the previous instance. |
| `registry-write`  | Runs `reg.exe add <key> /v <name> /t <type> [/d <data>] /f`. `key`, `value` and `data` are rendered as templates, so `{configField}` reads a field. `dataField` names a config field to take the payload from, and accepts a non-string field — a `number` field is how a `REG_DWORD` gets a value. `/d` is passed only when there is a payload. Windows-only; capability loader skips this kind on non-Windows. |
| `registry-delete` | Runs `reg.exe delete <key> [/v <name>] /f`. `key` and `value` are rendered as templates. Idempotent: reg.exe exits 1 both for "nothing to delete" and for a real failure, and the message that tells them apart is localised, so a non-zero exit is resolved by asking `reg query` whether the target is still there. Windows-only. |

There is no `keyField` / `nameField` / `typeField` on these steps. The
param names are the literals above and a field is read by templating it
(`key: '{myKey}'`) or, for the payload, by `dataField`. A step naming a
`*Field` param for the key is not read by the runner and fails with
"registry-write: key is required".

**A registry write is not a file operation.** `plan_step_targets` plans no
target for these steps, so a registry-only install records no transaction:
it is outside Undo, and `is_capability_installed` cannot see it. And no
check kind can address a registry value — the check vocabulary is
processes, paths inside the game folder, and archive URLs — so a recipe
that writes one should say what it did in `safetyNotes` rather than
declare a check that cannot pass.

`uninstall` follows the same shape. If it is empty the runner falls
back to rolling back the latest transaction recorded for the
capability id — which is the common case for a one-step install.

## Dependencies

```yaml
id: cheeky-foveated-dlss
dependencies:
  - optiscaler          # loader must be active before the addon
```

Before running `install`, the runner walks `dependencies` recursively
and installs every capability that is not yet active for the game. A
capability counts as **installed** when the transaction store holds an
applied record for the `(gameId, capabilityId)` pair — the same
predicate the uninstall path uses — so the game catalog's installed
marker and the dependency check can never disagree.

Rules:

* Transitive dependencies install first (depth-first order); the
  request's `InstallResult.installedDependencies` lists what was
  auto-installed, in install order.
* Chains longer than 8 capabilities abort with the full chain in the
  error, as do cycles (`a -> b -> a`) and ids that are not loaded in
  the registry (`missing chain: x -> y`).
* Auto-installed dependencies receive the same `ResolvedConfig` as the
  requested capability, so dependent recipes in one bundle can share
  config field names.
* `force: true` never cascades: it bypasses the compatibility gate
  (below) for the requested capability only, never for a dependency.

## Compatibility (game builds)

```yaml
compatibility:
  gameExe: Game/Binaries/Win64/Game-Win64-Shipping.exe
  minExeVersion: 1.16.0
  maxExeVersion: 1.17.9
  blockedExeVersions: ["1.16.3.0"]
```

When `compatibility` names an exe and at least one bound, the runner
evaluates it as a synthesized `exe-version` check (id
`exe-version-compat`, severity `blocker`) in two places:

* `capability_install` / `community_capability_install` refuse to
  install while the check fails, unless the request sets
  `force: true`. The evaluated outcome is still returned in
  `InstallResult.compatibility` so the UI can show exactly what the
  user overrode.
* `capability_evaluate` reports the outcome as the first row of the
  `VerificationReport.checks`, so the UI can disable Apply and explain
  why before the user even tries.

A spec without a `compatibility` block — or with a block that sets no
bound — is treated as compatible with every game build and skips the
probe entirely. A **blank `executableDir`** also skips it: the caller
has no game to check against (the AI recommend flow and the community
panel install without a selected game), and failing closed there would
make every community spec with a compatibility block uninstallable. An
executable directory that exists but whose `FileVersion` cannot be read
is a different case and still fails closed — that one is a real "I could
not verify this build". Module-level dependency order for the game catalog
(frontend) is resolved by `getMissingDependenciesForModule` in
[`src/services/catalog.ts`](../src/services/catalog.ts), so the UI can
prompt for prerequisite modules before calling install.

## Template rendering

The `write-text-file` step (and any future text-emitting step) renders
the supplied `template` by replacing `{configFieldName}` occurrences
with the matching entry in `ResolvedConfig.values`. Unknown
placeholders are left intact so a UI typo never silently drops a key.

```yaml
template: |
  backend={backend}
  diagnostic={diagnostics}
```

With:

```ts
ResolvedConfig.values = { backend: "fidelityfx", diagnostics: "0" }
```

→ renders `backend=fidelityfx\ndiagnostic=0\n`.

## Tauri command surface

| Command                                              | Purpose                                          |
|------------------------------------------------------|--------------------------------------------------|
| `capability_list()`                                  | Enumerate every loaded capability (id + summary). |
| `capability_install({ capabilityId, gameId, gameName, installDir, executableDir, config, force? })` | Run `spec.install` (auto-installing `dependencies` first) and record a transaction. `force: true` bypasses the `compatibility` gate for this capability only. |
| `capability_uninstall({ capabilityId, gameId, installDir })` | Run `spec.uninstall` (or roll back the latest transaction). |
| `capability_evaluate({ capabilityId, executableDir, config })` | Run every check in `spec.checks` and `spec.verify` and return a `VerificationReport`. |
| `capability_compatibility({ capabilityId, executableDir })` | Evaluate ONLY the `compatibility` block and return its `CheckOutcome`, or `null` when the spec does not constrain the game build. Cheap enough to call while rendering a card, unlike `capability_evaluate` (which downloads archives for `archive-sha256`). |
| `validate_capability_yaml({ yaml })`                 | Parse + schema/semantic-check an AI-drafted YAML (id shape, category/status enums, known step/check kinds). Returns an `AuthorSpecSummary`. |
| `preview_capability_plan({ yaml })`                  | Render the human-readable dry-run plan the AI dialog shows before saving. |
| `save_capability_yaml({ yaml, overwrite })`          | Validate and atomically write `%LOCALAPPDATA%\Moddin\capabilities\<id>.yaml` (with `.bak` backup). Refuses to shadow built-in ids; `overwrite` replaces a previous local file. |
| `validate_recommendations_yaml({ yaml })`            | Validate the recommendation list the AI returns in `recommend` mode against the live registry. |

The three authoring commands live in
[`src-tauri/src/capability_authoring.rs`](../src-tauri/src/capability_authoring.rs)
and back the in-app "Ask AI" flow (`src/features/ai-assistant/`); they reuse
the same `CapabilitySpec` serde schema as the runner, so an AI-authored recipe
is checked exactly like a shipped one.

The TypeScript layer mirrors these as `install_capability`,
`uninstall_capability`, `evaluate_capability`, and
`get_capability_compatibility` inside `invoke(...)` calls, in
[`src/features/capability-modules/service.ts`](../src/features/capability-modules/service.ts).

## UI rendering contract

The UI receives the live `VerificationReport` (from
`capability_evaluate` or from a parallel `Module::verify` path) and:

1. Groups checks by `category` into three sections — Global,
   Category, Module-specific — each with its own heading.
2. Colours each row by `severity` (`info` → neutral, `warning` →
   amber, `blocker` → red).
3. Reconciles each result against the `VerificationReport.definitions`
   array via the `id` field. When a definition is present without a
   matching result yet, the row still renders (in pending state).
4. Surfaces `safetyNotes` in the card header banner.

## Adding a new capability

1. Drop `src-tauri/capabilities/<id>.yaml` with the schema above.
2. Add `const <ID>_YAML: &str = include_str!("../capabilities/<id>.yaml");`
   to `src-tauri/src/capability_runner.rs` and reference it in the
   `load()` loop.
3. Add a `gameId`-keyed entry in `src/catalog/games/<game>.yaml`:
   ```yaml
   modules:
     - id: <id>
       name: <displayName>
       description: <one-liner>
       category: vr
       status: available
   ```
4. (Optional) Add `capability_<id>` to the engine preset's module
   list under `src/catalog/engines/` if the capability is universal
   across engines.

No Rust code change is required for the recipe itself. New step /
check kinds require a new match arm in `builtin_steps.rs` /
`builtin_checks.rs` plus a unit test.

## See also

- [`docs/SCOPE.md`](SCOPE.md) — overall project scope.
- [`docs/MODULES.md`](MODULES.md) — design notes for the four
  bespoke modules (ReShade, UE4SS, BepInEx, REFramework). The bespoke
  modules are still implemented in Rust for now; capabilities are the
  long-term target for all of them.
- [`src-tauri/src/capability.rs`](../src-tauri/src/capability.rs) —
  Rust schema.
- [`src-tauri/src/capability_runner.rs`](../src-tauri/src/capability_runner.rs) —
  registry + dispatch.
- [`src-tauri/src/builtin_steps.rs`](../src-tauri/src/builtin_steps.rs) —
  step kinds.
- [`src-tauri/src/builtin_checks.rs`](../src-tauri/src/builtin_checks.rs) —
  check kinds.
- [`src-tauri/capabilities/ofxr-bridge.yaml`](../src-tauri/capabilities/ofxr-bridge.yaml) —
  complete sample.
- [`src-tauri/capabilities/optiscaler.yaml`](../src-tauri/capabilities/optiscaler.yaml) —
  second sample covering `move-file`, `file-delete`, and the
  `proxyCandidates` workflow.
- [`src/types/capability.ts`](../src/types/capability.ts) —
  TypeScript mirror.