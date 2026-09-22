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
  - kind: extract-zip
    description: Extract OFXR-Bridge archive into the game's executable directory.
    params:
      # `archivePathField` resolves to a config field whose value
      # is a path; `archiveBytesField` would point at a buffer the
      # runner hands in (download step, future work).
      archivePathField: downloadUrl
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
| `extract-zip`      | Reads a `zip` archive and extracts every member into the executable directory, sanitising paths. |
| `verify-hash`     | Reads `pathField` and compares SHA-256 to `expectedField`.    |
| `file-delete`     | Removes `pathField` (config field holding an absolute path). |
| `write-text-file` | Writes `pathField` with `template` rendered (`{field}` → `ResolvedConfig.values`). |
| `write-binary-file` | Same as `write-text-file` but for `base64` payload — useful for small DLLs and manifest blobs carried inline. |
| `move-file`       | Renames / moves a file inside the executable directory (used for picking a proxy DLL among the candidates). |
| `spawn-process`   | Starts `executable` (resolved against the game dir).         |
| `kill-process`    | Runs `taskkill /IM <name> /T` (optionally `/F`) so an install can stop the previous instance. |
| `registry-write`  | Runs `reg.exe add <key> /v <name> /t <type> [/d <data>] /f`. Windows-only; capability loader skips this kind on non-Windows. |
| `registry-delete` | Runs `reg.exe delete <key> [/v <name>] /f`. Windows-only.     |

`uninstall` follows the same shape. If it is empty the runner falls
back to rolling back the latest transaction recorded for the
capability id — which is the common case for a one-step install.

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
| `capability_install({ capabilityId, gameId, gameName, installDir, executableDir, config })` | Run `spec.install` and record a transaction.      |
| `capability_uninstall({ capabilityId, gameId, installDir })` | Run `spec.uninstall` (or roll back the latest transaction). |
| `capability_evaluate({ capabilityId, executableDir, config })` | Run every check in `spec.checks` and `spec.verify` and return a `VerificationReport`. |

The TypeScript layer mirrors these as `install_capability`,
`uninstall_capability`, `evaluate_capability`, and `list_capabilities`
inside `invoke(...)` calls.

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