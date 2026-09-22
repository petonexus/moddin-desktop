# Modules — design notes for contributors

This document captures the **design intent** of the bespoke modules and how to add new ones. It is the hand-off between roadmap entries and concrete implementations.

> **Looking for the recipe schema?** See [CAPABILITY-CONTRACT.md](CAPABILITY-CONTRACT.md).
> **Want to add a capability to a game?** See [CONTRIBUTING.md](CONTRIBUTING.md).

---

## Two ways to add a mod

Moddin has two parallel paths for adding mod support to a game:

### 1. Capability (recommended)

A single YAML file in [petonexus/moddin-community-capabilities](https://github.com/petonexus/moddin-community-capabilities) `capabilities/<id>/` or in `%LOCALAPPDATA%\Moddin\capabilities\`. Runs through the same data-driven pipeline as built-in capabilities.

**Use when**: the mod fits one of the existing step kinds (extract-zip, write-text-file, registry-write, …).

### 2. Bespoke Rust module

A Rust file in `src-tauri/src/<name>.rs` with a `Module` trait implementation. Requires a Rust PR in the app repo.

**Use when**: the mod needs a step kind that doesn't exist yet (e.g. installing a Windows service, talking to a proprietary driver).

---

## Adding a capability

See [CONTRIBUTING.md → Adding a capability](CONTRIBUTING.md#adding-a-capability). The minimum schema is:

```yaml
id: community-my-mod
displayName: My Community Mod
category: graphics
status: available
configSchema:
  - name: version
    type: string
    required: true
  - name: downloadUrl
    type: url
    required: true
  - name: sha256
    type: sha256
    required: true
install:
  - kind: extract-zip
    params:
      archivePathField: downloadUrl
safetyNotes:
  - Always close the game before applying.
```

Validate locally before opening a PR:

```bash
python scripts/validate_capability.py capabilities/community-my-mod/capability.yaml
```

The validator checks: schema, kind allow-list, URL format (HTTPS only), SHA-256 format, absolute paths, and id collisions with other capabilities.

---

## Adding a new step kind

Step kinds live in `src-tauri/src/builtin_steps.rs` as `match` arms in `execute_step()`. Each kind has its own `run_*` function. To add a kind:

1. Add the kind name to `known_kinds()`.
2. Write `fn run_<kind>(step: &StepSpec, context: &StepContext) -> Result<StepResult, String>`.
3. Add the kind to `execute_step()`'s match.
4. Add 2-3 unit tests covering happy path + edge cases.
5. Document in [CAPABILITY-CONTRACT.md → Built-in step kinds](CAPABILITY-CONTRACT.md#built-in-step-kinds).

Currently supported kinds (10):

- `extract-zip`, `verify-hash`, `file-delete`, `write-text-file`, `write-binary-file`
- `move-file`, `spawn-process`, `kill-process`
- `registry-write`, `registry-delete`

---

## Adding a new check kind

Check kinds live in `src-tauri/src/builtin_checks.rs` as `match` arms in `evaluate_check()`. The same steps as above, but in `evaluate_check()` instead.

Currently supported check kinds (5):

- `process-running`, `file-exists`, `file-absent`, `archive-reachable`, `archive-sha256`

---

## Bespoke modules

The current set of bespoke modules (still being migrated to capabilities):

| Module | File | Notes |
| --- | --- | --- |
| OBS VR Capture | `src-tauri/src/obs.rs` | Scene collection cloning, target resolution, transparent re-apply |
| OFXR Bridge FrameGen | `src-tauri/src/ofxr.rs` | Tray lifecycle, manual layer arm, NIST-Fidelity configuration |
| UEVR engine-aware installer | `src-tauri/src/uevr.rs` | Local engine detection, three backend variants, SHA-256 verification |
| OptiScaler | `src-tauri/src/optiscaler.rs` | Proxy candidate walk, marker-file rotation |
| Cheeky Foveated DLSS | `src-tauri/src/cheeky.rs` | ReShade add-on drop + hash |
| ReShade host | `src-tauri/src/reshade.rs` | Full installer + proxy rename |
| OpenXR per-game runtime | `src-tauri/src/openxr.rs` | HKCU registry override |
| VR launch profile | `src-tauri/src/vr_launch.rs` | Catalog-declared INI patches |
| Desktop shortcut | `src-tauri/src/desktop_shortcut.rs` | `.lnk` writer |

Each one is a candidate for migrating to a YAML capability. The migration pattern:

1. Identify which steps the module runs.
2. Map them to existing step kinds.
3. Write the YAML.
4. Add a `cli-style` entry in the catalog recipe for the game.
5. Remove the bespoke Rust code.

---

## See also

- [CAPABILITY-CONTRACT.md](CAPABILITY-CONTRACT.md) — full YAML schema + step / check kinds
- [CONTRIBUTING.md](CONTRIBUTING.md) — practical walkthroughs for adding a capability
- [ARCHITECTURE.md](ARCHITECTURE.md) — how the Rust code is organised
- [DEVELOPMENT.md](DEVELOPMENT.md) — local build setup
