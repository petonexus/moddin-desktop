# Backend modules — which file is authoritative

`src-tauri/src/` contains several `X.rs` / `X_module.rs` pairs with overlapping
names. Nothing in the repo said which one a maintainer should edit, so this
document does. Every claim below is traceable to a `path:line`.

Nothing here has been changed. This is a map plus a recommendation; no Rust
file was deleted or edited to produce it.

## How to read a verdict

* **Authoritative** — the file the live `#[tauri::command]` functions live in,
  the file `generate_handler!` registers, and the file the frontend's
  `invoke(...)` calls land on.
* **Unreferenced** — the file is declared in `lib.rs` and therefore compiled,
  but nothing in the crate names it. It is not a fallback and not a legacy
  path; it has no caller at all.
* **Not compiled** — the file exists on disk but has no `mod` declaration, so
  rustc never sees it. Deleting it changes no behaviour and loses no coverage.

Reachability was established from the frontend, not from the Rust side. A
`#[tauri::command]` that is registered but never `invoke`d is not live.

Where a command is cited by line, the line is the `#[tauri::command]`
attribute; the `fn` is on the next line.

Line numbers reflect the worktree as of writing, which had the F-14 cleanup
applied (see the overlap section). Frontend call sites were re-verified
after it landed.

## Summary

| Concern | Authoritative | Other file | Other file's state |
| --- | --- | --- | --- |
| OBS VR | `obs.rs` | `obs_module.rs` | Unreferenced |
| OFXR FrameGen | `ofxr.rs` | `ofxr_module.rs` | Unreferenced |
| OpenXR runtimes | `openxr.rs` | `openxr_module.rs` | Unreferenced |
| OptiScaler | `optiscaler.rs` | `optiscaler_module.rs` | Unreferenced |
| ReShade host | `capabilities/reshade.yaml` | *(Rust pair deleted — see below)* | Resolved |
| UEVR | `uevr.rs` | `uevr_module.rs` | Unreferenced |
| Module version check | `updates.rs` | `updater.rs` | Not compiled |
| Cheeky Foveated DLSS | `cheeky.rs` + `capabilities/cheeky-foveated-dlss.yaml` | — | Both live, different shape |

The short version: **in every surviving `X` / `X_module` pair the `X.rs` file is
the one to edit.** `X_module.rs` is a `Module` trait adapter layer that was
scaffolded in September 2026 and never wired to anything. The ReShade pair has
already been resolved by deletion.

## The two registration gates

`src-tauri/src/lib.rs:107-168` is the only place a `#[tauri::command]` becomes
reachable from the UI. It registers the plain names — `obs::`, `ofxr::`,
`openxr::`, `optiscaler::`, `uevr::`, `cheeky::`,
`updates::check_module_update` — and **no** `*_module::` entry anywhere. The
five surviving adapters are declared at `lib.rs:19,21,23,25,30` and named
nowhere else:

```
git grep -n "obs_module|ofxr_module|openxr_module|optiscaler_module|uevr_module" -- src-tauri/
```

returns only the `mod` declarations in `lib.rs`, the adapters' own
`//!` doc-links, and their own unit-test names. No registry, no dispatch
table, no `dyn Module`.

`crate::module::Module` itself has no production implementor. The only
`impl Module` in the crate outside the adapters is `DummyModule` at
`src-tauri/src/module.rs:377`, used by that file's own tests.

## Per pair

### OBS — `obs.rs` authoritative, `obs_module.rs` unreferenced

Live commands: `preview_obs_vr` (`obs.rs:593`), `configure_obs_vr`
(`obs.rs:600`), `uninstall_obs_vr` (`obs.rs:690`).

Frontend callers, all in `src/App.vue`: `preview_obs_vr` at `:939` and
`:1417`, `configure_obs_vr` at `:1028`, `uninstall_obs_vr` at `:1690`. The two
mutating commands are also listed in
`src/services/activity-log.ts:8-9`, which wraps every persistent-action
`invoke`.

`obs_module.rs` (`ObsVrModule`, 149 lines) implements `Module` and returns a
stub preview: `obs_module.rs:39-52` returns `can_apply: false` with the change
string *"Forward to obs::preview_obs_vr…"*. `apply` at `obs_module.rs:54` is
a hard `Err` with the message *"…requires a fully-built ObsVrRequest; the
catalog config plumbing is pending."* `remove` (`obs_module.rs:80`) is the one
method that really does work, because it only calls the transaction store.

**Recommendation: delete `obs_module.rs` and its `mod` line.** It cannot
install anything, and its own doc comment (`obs_module.rs:5-11`) says the
forwarding is waiting on a refactor that the capability layer later made moot.

### OFXR — `ofxr.rs` authoritative, `ofxr_module.rs` unreferenced

Live commands: `preview_ofxr` (`ofxr.rs:867`), `install_ofxr` (`ofxr.rs:874`),
`uninstall_ofxr` (`ofxr.rs:974`).

Frontend callers in `src/App.vue`: `preview_ofxr` at `:956`, `:1472` and
`:1608`; `install_ofxr` at `:1107`; `uninstall_ofxr` at `:1698`. Also
`src/services/activity-log.ts:12-13`.

`ofxr_module.rs` (`OfxrModule`, 404 lines) is the most complete adapter — it
really does translate `ModuleContext.config` into an `OfxrRequest`
(`ofxr_module.rs:30`) and delegate. It is still unreachable: no dispatcher
constructs it.

**Recommendation: delete `ofxr_module.rs` and its `mod` line.** The
`ModuleContext.config` plumbing it was waiting on has not been built, and the
YAML capability path (`capabilities/ofxr-bridge.yaml`) covers the declarative
case instead.

### OpenXR — `openxr.rs` authoritative, `openxr_module.rs` unreferenced

Live commands: `inspect_openxr` (`openxr.rs:441`), `set_game_openxr_runtime`
(`openxr.rs:446`), `set_system_openxr_runtime` (`openxr.rs:455`).

This is the only module reached through a service layer rather than `App.vue`:
`src/features/openxr/service.ts:10,14,21` wraps the three commands, consumed by
`src/features/openxr/useOpenXrManager.ts:46,79,91`, which
`src/features/openxr/OpenXrManager.vue:37` calls, which
`src/components/shell/GlobalTools.vue:22` renders. The two mutating commands
are also in `src/services/activity-log.ts:22-23`.

`openxr_module.rs` (`OpenXrModule`, 168 lines) is a pilot adapter whose own
header (`openxr_module.rs:5-9`) states that *"the existing
`openxr::inspect_openxr` / … Tauri commands remain the supported entry
points."*

**Recommendation: delete `openxr_module.rs` and its `mod` line.**

### OptiScaler — `optiscaler.rs` authoritative, `optiscaler_module.rs` unreferenced

Live commands: `preview_optiscaler` (`optiscaler.rs:401`),
`install_optiscaler` (`optiscaler.rs:650`), `uninstall_optiscaler`
(`optiscaler.rs:687`).

Frontend callers in `src/App.vue`: `preview_optiscaler` at `:948`, `:1055`,
`:1430` and `:1597`; `install_optiscaler` at `:1081`; `uninstall_optiscaler` at
`:1694`. Also `src/services/activity-log.ts:10-11`.

`optiscaler_module.rs` (`OptiScalerModule`, 177 lines) is a pilot adapter;
its header (`optiscaler_module.rs:5-8`) says the Tauri commands *"keep their
bespoke request structs until the catalog config plumbing lands."*

**Recommendation: delete `optiscaler_module.rs` and its `mod` line.**

### ReShade — resolved by deletion; the YAML recipe is the live path

**`src-tauri/src/reshade.rs` and `src-tauri/src/reshade_module.rs` no longer
exist.** They were deleted while this document was being written, by the
in-flight F-14 effort (see the overlap section below). The authoritative
ReShade implementation is `src-tauri/capabilities/reshade.yaml`.

For the record, here is why the Rust pair was dead when it was removed, and
why the verdict is safe rather than a regression.

They had no frontend caller. `preview_reshade`, `install_reshade` and
`uninstall_reshade` were registered but had no `invoke` anywhere in `src/`:

1. The catalog `modules:` list drives the module cards, and
   `openModule` in `src/App.vue` only knows five ids — `obs-vr`, `optiscaler`,
   `ofxr-framegen`, `cheeky-foveated-dlss`, `uevr`
   (`src/App.vue:936-977`). A `reshade` card falls through to
   `throw new Error(t('moduleNoAction'))` at `src/App.vue:977`. On remove it
   falls to the generic `rollback_latest_module_transaction` branch
   (`src/App.vue:1708`). The catalog entries that do exist are marked
   `status: planned` (`src/catalog/games/stalker-2.yaml:87`,
   `src/catalog/engines/unity.yaml:32`, `src/catalog/engines/unreal5.yaml:52`).
2. ReShade *is* reachable — through the capability layer.
   `src-tauri/capabilities/reshade.yaml` is `status: available` and is compiled
   in at `capability_runner.rs:41`; the UI installs it via `capability_install`
   (`src/features/capability-modules/service.ts:93`).

So the live ReShade path was always the YAML recipe, and `lib.rs:144-148` now
carries a comment saying so in place of the deleted registrations.

**Recommendation: none. Already done.** The one thing still worth doing is
recording *why*, so nobody re-adds a `reshade.rs`: a bespoke Rust installer
for a mod that the capability contract already covers is a new copy of the
problem this document exists to stop. The `Module` adapter (`ReshadeModule`,
407 lines) went with it, and its own header called itself *"the **first** real
adoption of `Module`"* — it was a spike, and it was never hooked up.

### UEVR — `uevr.rs` authoritative, `uevr_module.rs` unreferenced

Live commands: `preview_uevr` (`uevr.rs:658`), `install_uevr` (`uevr.rs:836`),
`uninstall_uevr` (`uevr.rs:879`).

Frontend callers in `src/App.vue`: `preview_uevr` at `:972`, `:1508`;
`install_uevr` at `:1160`; `uninstall_uevr` at `:1706`. Also
`src/services/activity-log.ts:16-17`.

`uevr_module.rs` (`UevrModule`, 162 lines) is a pilot adapter; its header
(`uevr_module.rs:5-8`) says the Tauri commands *"keep their bespoke request
structs until the catalog config plumbing lands."*

**Recommendation: delete `uevr_module.rs` and its `mod` line.**

> Correction to existing docs: `docs/AUDIT-2026-09-29.md:156` says the library
> grid installs *"through the working `ofxr.rs` / `uevr_module.rs` modules."*
> The second half is wrong — the grid calls `uevr::preview_uevr` and friends
> in `uevr.rs`. `uevr_module.rs` has no caller.

### Version checking — `updates.rs` authoritative, `updater.rs` not compiled

These two are **not** a pair of implementations of the same thing, and neither
is an app self-updater. There is no `tauri-plugin-updater` in `Cargo.toml` and
no `plugins.updater` in `tauri.conf.json` (`git grep updater -- Cargo.toml
src-tauri/Cargo.toml src-tauri/tauri.conf.json` returns nothing);
`docs/AUDIT-2026-09-29.md:284-287` records the same, and `ROADMAP.md:367`
already carries the correction. `ROADMAP.md`'s "GitHub Releases updater" item
is misclassified on that basis.

`updates.rs` is live:

* `check_module_update` (`updates.rs:199`) is registered at `lib.rs:152` and
  called from `src/App.vue:1628`, behind a timeout, for the "updates
  available" panel. `App.vue:1596-1620` uses the per-module previews to build
  the request.
* Its parsing helpers are used by the capability check engine, which *is* live:
  `builtin_checks.rs:367,378,387,396,410,719` call
  `crate::updates::parse_version` / `compare_versions` /
  `compare_version_cores`. So `updates.rs` has two independent live callers.

`updater.rs` (`Updater` trait + `GitHubReleaseUpdater`, 129 lines) was added
in `f4f6838` (2026-09-21) as a reusable "check upstream version" contract. It
has **no `mod updater;` declaration**, at any commit:

```
git log -S"mod updater" -- src-tauri/src/lib.rs     # no output
git show f4f6838:src-tauri/src/lib.rs | grep '^mod ' # updater absent
```

It has never been compiled, so its two unit tests have never run. Its
doc comment already names `crate::updates::check_module_update` as the thing it
adapts (`updater.rs:55-57`).

**Recommendation: delete `updater.rs`.** If the `Updater` abstraction is wanted
later, the live `updates.rs` is the place to lift it from.

## `cheeky.rs` — the seventh case, and a different shape

`cheeky.rs` has no `_module` twin. It is a seventh example of *the same
problem in a different form*, so it is worth stating explicitly.

**It is live.** `preview_cheeky_foveated_dlss` (`cheeky.rs:221`),
`install_cheeky_foveated_dlss` (`cheeky.rs:318`) and
`uninstall_cheeky_foveated_dlss` (`cheeky.rs:358`) are registered at
`lib.rs:125-127` and called from `src/App.vue:964`, `:1135` and `:1702`, plus
`:1491` and `:1619` for the verification and update passes. Also
`src/services/activity-log.ts:14-15`.

**But it also ships a capability spec.** `capabilities/cheeky-foveated-dlss.yaml`
is `status: available` and is compiled in at `capability_runner.rs:39-40`, so
Cheeky is reachable *twice*, through two independent engines. Both are live;
neither supersedes the other. The catalog card
(`src/catalog/engines/unreal5.yaml:43`, and the game catalogs) drives the
Rust path, and the capability section drives the YAML path.

This matters for the reader because it is the shape every other module is
moving toward: `ofxr-bridge`, `optiscaler`, `openxr-helpers` and `uevr` all
have a `capabilities/*.yaml` alongside their Rust module, and `reshade` has
already gone the other way. So the real long-term duplication in
`src-tauri/src/` is not `X.rs` vs `X_module.rs` — it is **Rust command vs YAML
capability spec**. That is a separate decision from this document's scope, but
a maintainer editing `ofxr.rs`, `optiscaler.rs`, `openxr.rs` or `uevr.rs`
should know that the matching `capabilities/*.yaml` is the version the
capability section runs.

## Overlap with F-14, which resolved mid-document

`ROADMAP.md:148-152` and `ROADMAP.md:226-230` track **F-14 — "Wire or delete
the unused backend commands"**, and its list explicitly included
`install_reshade` / `preview_reshade` / `uninstall_reshade`.

**F-14 landed while this document was being written.** It deleted
`src-tauri/src/reshade.rs` and `src-tauri/src/reshade_module.rs` outright
rather than wiring them, and its comment now sits in their place at
`lib.rs:144-148`: *"UE4SS, BepInEx, REFramework and ReShade no longer expose
dedicated commands: they are capability specs…"* The same commit removed
`compat_report.rs`, `library_state.rs`, the four snapshot commands and the
GOG command.

This document was re-verified against that state. The five `*_module.rs`
adapters are untouched by F-14, and none of the other verdicts changed.

One consequence worth naming: F-14 is scoped to *registered* commands, so the
`*_module.rs` adapters fell outside it and are still here. They are the
remaining half of the same cleanup.

## Recommendations in one place

| File | Recommendation |
| --- | --- |
| `obs_module.rs` | Delete, plus `lib.rs:19` |
| `ofxr_module.rs` | Delete, plus `lib.rs:21` |
| `openxr_module.rs` | Delete, plus `lib.rs:23` |
| `optiscaler_module.rs` | Delete, plus `lib.rs:25` |
| `uevr_module.rs` | Delete, plus `lib.rs:30` |
| `updater.rs` | Delete — no `mod` line to remove |
| `obs.rs`, `ofxr.rs`, `openxr.rs`, `optiscaler.rs`, `uevr.rs`, `cheeky.rs`, `updates.rs` | Keep; add the header line below |
| `reshade.rs`, `reshade_module.rs` | None — already deleted by F-14 |

Deleting the five adapters also leaves `src-tauri/src/module.rs` with no
production implementor. That is already true today, so it is not a regression,
but it does mean `module.rs` becomes test-only and should say so.

## Exact header comments to add

One `//!` line at the very top of each file, above any existing content.
`obs.rs`, `ofxr.rs`, `openxr.rs`, `optiscaler.rs`, `uevr.rs`, `cheeky.rs` and
`updates.rs` open with a `use` statement today, so the line becomes line 1.
The five `*_module.rs` files already open with a `//!` block — insert the line
above it, and the old block follows as the detail.

```rust
// obs.rs
//! Authoritative OBS VR implementation — the three `#[tauri::command]`s invoked from src/App.vue. See docs/BACKEND-MODULES.md.

// obs_module.rs
//! Unreferenced `Module` trait adapter over `obs` — not a Tauri command and called from nowhere. See docs/BACKEND-MODULES.md.

// ofxr.rs
//! Authoritative OFXR FrameGen implementation — the three `#[tauri::command]`s invoked from src/App.vue. See docs/BACKEND-MODULES.md.

// ofxr_module.rs
//! Unreferenced `Module` trait adapter over `ofxr` — not a Tauri command and called from nowhere. See docs/BACKEND-MODULES.md.

// openxr.rs
//! Authoritative OpenXR runtime manager — the three `#[tauri::command]`s invoked from src/features/openxr/service.ts. See docs/BACKEND-MODULES.md.

// openxr_module.rs
//! Unreferenced `Module` trait adapter over `openxr` — not a Tauri command and called from nowhere. See docs/BACKEND-MODULES.md.

// optiscaler.rs
//! Authoritative OptiScaler installer — the three `#[tauri::command]`s invoked from src/App.vue. See docs/BACKEND-MODULES.md.

// optiscaler_module.rs
//! Unreferenced `Module` trait adapter over `optiscaler` — not a Tauri command and called from nowhere. See docs/BACKEND-MODULES.md.

// uevr.rs
//! Authoritative UEVR installer — the three `#[tauri::command]`s invoked from src/App.vue. See docs/BACKEND-MODULES.md.

// uevr_module.rs
//! Unreferenced `Module` trait adapter over `uevr` — not a Tauri command and called from nowhere. See docs/BACKEND-MODULES.md.

// updates.rs
//! Authoritative per-module version check (`check_module_update`) — this is not an app self-updater. See docs/BACKEND-MODULES.md.

// updater.rs
//! Not compiled — no `mod updater;` in lib.rs. The live path is updates.rs. See docs/BACKEND-MODULES.md.

// cheeky.rs
//! Authoritative Cheeky Foveated DLSS commands; a capability spec for the same mod also ships and is also live. See docs/BACKEND-MODULES.md.
```

## History

`git log --diff-filter=A` and `git log --follow` (no renames detected for any
file in scope):

| File | Added | Commit |
| --- | --- | --- |
| `obs.rs` | 2026-09-12 | `0e10819` bootstrap MVP |
| `ofxr.rs` | 2026-09-12 | `cdf3845` |
| `openxr.rs` | 2026-09-12 | `43e9ecc` |
| `optiscaler.rs` | 2026-09-12 | `70d9f15` |
| `uevr.rs` | 2026-09-12 | `5c6b507` |
| `cheeky.rs` | 2026-09-12 | `b4f0809` |
| `updates.rs` | 2026-09-12 | `7d4ee66` |
| `module.rs` | 2026-09-21 | `03629e8` |
| `reshade.rs` | 2026-09-21 | `03629e8` |
| `ofxr_module.rs` | 2026-09-21 | `0e8f9c0` *"…async Module trait + OfxrModule pilot"* |
| `updater.rs` | 2026-09-21 | `f4f6838` *"extract ArchiveSource, Updater, and PathGuard"* |
| `obs_module.rs`, `openxr_module.rs`, `optiscaler_module.rs`, `uevr_module.rs` | 2026-09-21 | `1e4b047` |
| `reshade_module.rs` | 2026-09-21 | `1e4b047` |

`reshade.rs` and `reshade_module.rs` were deleted in the uncommitted worktree
by the F-14 cleanup while this document was being written; see the overlap
section. `git log --diff-filter=A` still dates them at the commits above.

The history does explain the twins, and it explains them as a spike. The
command files are the bootstrap MVP (2026-09-12). `module.rs` arrived on
2026-09-21 as an attempt to generalise them behind one `Module` trait;
`0e8f9c0` calls the OFXR one a *"pilot"*, `1e4b047` calls the rest
*"pilot adapters"*, and `f4f6838` added `updater.rs` in the same wave. The
commit bodies say the adapters wait for *"the catalog config plumbing"*, which
was superseded by the YAML capability system (`cbf1e40`, 2026-09-21) before it
arrived. `1e4b047` itself records the build as *"clean except expected
dead-code warnings on stubs."*

So: the twins exist because a `Module`-trait generalisation was started, the
capability YAML path won instead, and the trait layer was left in place.

## Unresolved

* **Whether `module.rs` itself is worth keeping** once the adapters go. This
  document only establishes that it has no production implementor today. What
  it *should* become is a design decision, not a reachability finding.
* **The Rust-command vs YAML-capability duplication** for `ofxr-bridge`,
  `optiscaler`, `openxr-helpers` and `uevr` is real but out of scope here.
  Both are live; neither is dead. ReShade has been resolved in favour of the
  YAML, but that was a deletion, not a rule — establishing which one a
  maintainer *should* edit for the other four requires a product decision this
  document does not make.
* **Whether F-14's deletions are still in flight.** This document reflects an
  uncommitted worktree. If the ReShade commands are restored, the ReShade
  section needs redoing; check `git status --short src-tauri/src/` first.
