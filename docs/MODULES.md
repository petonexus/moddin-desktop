# Modules — design notes for contributors

This document captures the **design intent** of the bespoke modules and how to add new ones. It is the hand-off between roadmap entries and concrete implementations.

> **Looking for the recipe schema?** See [CAPABILITY-CONTRACT.md](CAPABILITY-CONTRACT.md).
> **Want to add a capability to a game?** See [CONTRIBUTING.md](CONTRIBUTING.md).

---

## Two ways to add a mod

Reusable work becomes **a YAML capability**. That is the direction the project is
moving in, and it is not a preference: one YAML is the only form that serves the
built-in, community and local catalogs identically, and it is what the
transaction store and Undo were built around. Rust is where you add an
*operation the engine does not have yet*, not a second way to install the same
mod.

### 1. Capability (the default)

A single YAML file in `src-tauri/capabilities/` (built-in), in
[petonexus/moddin-community-capabilities](https://github.com/petonexus/moddin-community-capabilities)
`capabilities/<id>/capability.yaml` (community), or in
`%LOCALAPPDATA%\Moddin\capabilities\` (local, private). All three run through the
same data-driven pipeline as built-in capabilities.

**Use when**: the mod fits the existing step and check kinds (11 steps, 6 checks —
see [CAPABILITY-CONTRACT.md](CAPABILITY-CONTRACT.md)).

### 2. Rust — only to add a step or check kind

`src-tauri/src/builtin_steps.rs` and `builtin_checks.rs` hold the `match` arms the
runner executes. A Rust PR is justified when, and only when, the work needs a new
step or check kind.

**Use when**: the mod genuinely needs an operation no existing kind expresses —
e.g. extracting into a staging directory, moving a directory, or building a git
tag. A recipe that needs one of those is marked `status: planned` and names the
missing kind in its `safetyNotes` until the kind exists. `ofxr-bridge` and `uevr`
are in exactly that state today.

**Do not** write a bespoke `src-tauri/src/<name>.rs` installer for something a
recipe can already do. Every one of those is a candidate for migration; the
pattern is in [Bespoke modules](#bespoke-modules) below.

---

## Adding a capability

See [CONTRIBUTING.md → Adding a capability](CONTRIBUTING.md#adding-a-capability). The minimum schema is:

```yaml
id: community-my-mod
displayName: My Community Mod
category: graphics
status: available
configSchema:
  - name: downloadUrl
    type: url
    required: true
  - name: sha256
    type: sha256
    required: true
  - name: version
    type: string
    required: true
checks:
  - id: archive-reachable
    label: Archive reachable
    kind: archive-reachable
    severity: warning
    category: modulespecific
    params:
      urlField: downloadUrl
  - id: archive-sha256
    label: Archive SHA-256 matches
    kind: archive-sha256
    severity: blocker
    category: modulespecific
    params:
      urlField: downloadUrl
      expectedField: sha256
install:
  - kind: download-file
    params:
      urlField: downloadUrl
      expectedField: sha256
      target: my-mod.zip
  - kind: extract-zip
    params:
      archivePath: my-mod.zip
safetyNotes:
  - Always close the game before applying.
```

**Download first, then extract. `extract-zip` never fetches.** `download-file`
writes the archive into Moddin's own cache (`%LOCALAPPDATA%\Moddin\downloads`)
under a bare `target` filename, so it never lands in the game folder and is not
part of the rollback set; `extract-zip` then reads that cached file. This is the
shape in `src-tauri/capabilities/bepinex.yaml`.

### Why the older pattern does not work

Earlier versions of this file taught `extract-zip` with
`archivePathField: downloadUrl`, and a `move-file` to do the proxy rename. Both
were wrong, and both failed on the first install step:

- `archivePathField` resolves the config field and hands the string to
  `resolve_download_path`, which only special-cases a **bare filename** and
  otherwise joins it to the executable directory. A URL became
  `<game dir>/https:/…` and the step died on a read. It is a *path* parameter,
  not a source.
- `move-file` renames a file that already exists. Pointing it at a URL asked it
  to rename a download that was never made.

`extract-zip` now renames the proxy DLL **natively**: give it `proxyField`, and
any archive member matching a known proxy filename is written out under the
config field's value. See `src-tauri/capabilities/reshade.yaml`, which is the
canonical example. The community validator now refuses the old shapes too —
`extract-zip` may not read a field declared `type: url`, and `move-file` may not
have identical source and destination.

Validate locally before opening a PR (the validator lives in the community
repo, not this one):

```bash
python scripts/validate_capability.py capabilities/community-my-mod/capability.yaml
```

The validator checks: schema, kind allow-list, URL format (HTTPS only), SHA-256
format, absolute paths, id collisions with other capabilities, and the two rules
above.

`status: planned` is enforced. A planned recipe refuses to install with a
message instead of reporting success while writing nothing — `ofxr-bridge`,
`reframework`, `ue4ss` and `uevr` are all planned today.

---

## Adding a new step kind

Step kinds live in `src-tauri/src/builtin_steps.rs` as `match` arms in `execute_step()`. Each kind has its own `run_*` function. To add a kind:

1. Add the kind name to `known_kinds()`.
2. Write `fn run_<kind>(step: &StepSpec, context: &StepContext) -> Result<StepResult, String>`.
3. Add the kind to `execute_step()`'s match.
4. Add 2-3 unit tests covering happy path + edge cases.
5. Document in [CAPABILITY-CONTRACT.md → Steps](CAPABILITY-CONTRACT.md#steps), where the
   built-in step kinds table lives.

Currently supported kinds (11):

- `download-file`, `extract-zip`, `verify-hash`, `file-delete`
- `write-text-file`, `write-binary-file`
- `move-file`, `spawn-process`, `kill-process`
- `registry-write`, `registry-delete`

---

## Adding a new check kind

Check kinds live in `src-tauri/src/builtin_checks.rs` as `match` arms in `evaluate_check()`. The same steps as above, but in `evaluate_check()` instead.

Currently supported check kinds (6):

- `process-running`, `file-exists`, `file-absent`, `archive-reachable`, `archive-sha256`, `exe-version`

---

## Bespoke modules

The current set of bespoke modules, and how far each one is from being a recipe.
Several of these files have an `X_module.rs` twin; read
[BACKEND-MODULES.md](BACKEND-MODULES.md) before you edit one.

| Module | File | Reachable from the UI? | Notes |
| --- | --- | --- | --- |
| OBS VR Capture | `src-tauri/src/obs.rs` | Yes | Scene collection cloning, target resolution, transparent re-apply |
| OFXR Bridge FrameGen | `src-tauri/src/ofxr.rs` | Yes — via the module, not the recipe | Tray lifecycle, manual layer arm, NIST-Fidelity configuration. The `ofxr-bridge` recipe is `status: planned` (no staging-directory extract) |
| UEVR engine-aware installer | `src-tauri/src/uevr.rs` | Yes — via the module, not the recipe | Local engine detection, three backend variants, SHA-256 verification. The `uevr` recipe is `status: planned` (`pinnedTag` is a git tag, not an archive) |
| OptiScaler | `src-tauri/src/optiscaler.rs` | Yes | Proxy candidate walk, marker-file rotation. The `optiscaler` recipe is `available` and uses `proxyField` |
| Cheeky Foveated DLSS | `src-tauri/src/cheeky.rs` | Yes | ReShade add-on drop + hash. The recipe is `available` |
| ReShade host | `src-tauri/src/reshade.rs` | **No** | The Rust commands have no caller in `src/`, and every catalog `reshade` module entry is `status: planned`. The `available` recipe is not offered anywhere yet |
| OpenXR per-game runtime | `src-tauri/src/openxr.rs` | Yes | HKCU registry override |
| VR launch profile | `src-tauri/src/vr_launch.rs` | Yes | Catalog-declared INI patches |
| Desktop shortcut | `src-tauri/src/desktop_shortcut.rs` | Yes | `.lnk` writer |

Each one is a candidate for migrating to a YAML capability. The migration pattern:

1. Identify which steps the module runs.
2. Map them to existing step kinds. If one has no home, that is the point to add a
   step kind in Rust — not to keep the bespoke module.
3. Write the YAML in `src-tauri/capabilities/`, following the download-then-extract
   pattern above.
4. Add a module entry to the engine preset in `src/catalog/engines/*.yaml` (and
   to the game YAML in `src/catalog/games/*.yaml` if it is not inherited). The
   module `id` **is** the capability `id`; give the entry a `name`, a
   `description`, a `category`, a `status` that matches the recipe's, and a
   `config:` block for any per-game overrides.
5. Remove the bespoke Rust code — and only once the recipe is actually reachable,
   or you have replaced a working install with nothing.

Step 4 is the one that gets skipped. A registered recipe that no catalog module
references is invisible: that is how `bepinex.yaml` was correct and unreachable
at the same time for a release.

---

## See also

- [README.md](README.md) — the docs index
- [CAPABILITY-CONTRACT.md](CAPABILITY-CONTRACT.md) — full YAML schema + step / check kinds
- [CONTRIBUTING.md](CONTRIBUTING.md) — practical walkthroughs for adding a capability
- [ARCHITECTURE.md](ARCHITECTURE.md) — how the Rust code is organised
- [DEVELOPMENT.md](DEVELOPMENT.md) — local build setup
