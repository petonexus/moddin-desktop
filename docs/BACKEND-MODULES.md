# Backend modules — which file is authoritative

`src-tauri/src/` contained several `X.rs` / `X_module.rs` pairs with overlapping
names. Nothing in the repo said which one a maintainer should edit, so this
document does. Every claim below is traceable to a `path:line`.

**The recommendations in this document have been carried out.** The five
`*_module.rs` adapters and `updater.rs` were deleted, along with their `mod`
declarations in `lib.rs`. What follows is the map that justified it, kept so
the reasoning survives the files it describes. The authoritative `X.rs`
files are unchanged and still the ones to edit.

## How to read a verdict

* **Authoritative** — the file the live `#[tauri::command]` functions live in,
  the file `generate_handler!` registers, and the file the frontend's
  `invoke(...)` calls land on.
* **Deleted** — the file has been removed. It was verified unreferenced
  (or, for `updater.rs`, never compiled) at the time of deletion; the
  evidence is preserved below.

Reachability was established from the frontend, not from the Rust side. A
`#[tauri::command]` that is registered but never `invoke`d is not live.

Where a command is cited by line, the line is the `#[tauri::command]`
attribute; the `fn` is on the next line.

Line numbers reflect the worktree as of writing, which had the F-14 cleanup
applied (see the overlap section), plus this document's own deletions.
Frontend call sites were re-verified after F-14 landed; the `X.rs` line
numbers below were correct at that point and the deletions that followed did
not touch those files, so they are still accurate.

## Summary

| Concern | Authoritative | Other file | Other file's state |
| --- | --- | --- | --- |
| OBS VR | `obs.rs` | `obs_module.rs` | Deleted |
| OFXR FrameGen | `ofxr.rs` | `ofxr_module.rs` | Deleted |
| OpenXR runtimes | `openxr.rs` | `openxr_module.rs` | Deleted |
| OptiScaler | `optiscaler.rs` | `optiscaler_module.rs` | Deleted |
| ReShade host | `capabilities/reshade.yaml` | *(Rust pair deleted — see below)* | Resolved |
| UEVR | `uevr.rs` | `uevr_module.rs` | Deleted |
| Module version check | `updates.rs` | `updater.rs` | Deleted |
| Cheeky Foveated DLSS | `cheeky.rs` + `capabilities/cheeky-foveated-dlss.yaml` | — | Both live, different shape |

The short version: **in every surviving `X` / `X_module` pair the `X.rs` file is
the one to edit.** `X_module.rs` was a `Module` trait adapter layer that was
scaffolded in September 2026 and never wired to anything. All five are now gone.
The ReShade pair was already resolved by deletion.

## The two registration gates

`src-tauri/src/lib.rs` is the only place a `#[tauri::command]` becomes
reachable from the UI. It registers the plain names — `obs::`, `ofxr::`,
`openxr::`, `optiscaler::`, `uevr::`, `cheeky::`,
`updates::check_module_update` — and **no** `*_module::` entry ever existed.
The five adapters were declared at `lib.rs:19,21,23,25,30` and named nowhere
else:

```
git grep -n "obs_module|ofxr_module|openxr_module|optiscaler_module|uevr_module" -- src-tauri/
```

returned only the `mod` declarations in `lib.rs`, the adapters' own
`//!` doc-links, and their own unit-test names. No registry, no dispatch
table, no `dyn Module`. Those five `mod` lines are now gone too.

`crate::module::Module` itself has no production implementor. The only
`impl Module` in the crate was `DummyModule` in `src-tauri/src/module.rs`,
used by that file's own tests.

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

**Done: `obs_module.rs` and its `mod` line are deleted.** It could not
install anything, and its own doc comment (`obs_module.rs:5-11`) said the
forwarding was waiting on a refactor that the capability layer later made moot.

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

**Done: `ofxr_module.rs` and its `mod` line are deleted.** The
`ModuleContext.config` plumbing it was waiting on was never built, and the
YAML capability path (`capabilities/ofxr-bridge.yaml`) covers the declarative
case instead.

> **Cross-file consequence, still open.** `ofxr_module.rs` held the crate's
> only `fn id(&self) -> &'static str { "ofxr-framegen" }`.
> `scripts/validate-catalog.mjs` builds its `backendModuleIds` set by scanning
> every `src-tauri/src/*.rs` for exactly that shape
> (`validate-catalog.mjs:291-300`), so deleting the file removed
> `ofxr-framegen` from the set of ids the app "offers". `check:catalog` now
> fails on `src/catalog/games/cyberpunk-2077.yaml` and
> `src/catalog/games/elden-ring.yaml`, which both list a
> `modules[ofxr-framegen]` card.
>
> This is the validator doing its job: the card was only "backed" by a dead
> file whose string happened to match. Nothing ever dispatched to
> `OfxrModule`, so the card's action did nothing. The fix belongs to whoever
> owns `src/catalog/**` — either drop the card, or repoint it at the live
> `ofxr-bridge` capability recipe.

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

**Done: `openxr_module.rs` and its `mod` line are deleted.**

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

**Done: `optiscaler_module.rs` and its `mod` line are deleted.**

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

**Done: `uevr_module.rs` and its `mod` line are deleted.**

> Correction to existing docs: `docs/AUDIT-2026-09-29.md:156` says the library
> grid installs *"through the working `ofxr.rs` / `uevr_module.rs` modules."*
> The second half is wrong — the grid calls `uevr::preview_uevr` and friends
> in `uevr.rs`. `uevr_module.rs` had no caller, and now does not exist.

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

**Done: `updater.rs` is deleted.** If the `Updater` abstraction is wanted
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

| File | Recommendation | Status |
| --- | --- | --- |
| `obs_module.rs` | Delete, plus its `mod` line | Done |
| `ofxr_module.rs` | Delete, plus its `mod` line | Done |
| `openxr_module.rs` | Delete, plus its `mod` line | Done |
| `optiscaler_module.rs` | Delete, plus its `mod` line | Done |
| `uevr_module.rs` | Delete, plus its `mod` line | Done |
| `updater.rs` | Delete — no `mod` line to remove | Done |
| `obs.rs`, `ofxr.rs`, `openxr.rs`, `optiscaler.rs`, `uevr.rs`, `cheeky.rs`, `updates.rs` | Keep; add the header line below | **Outstanding** |
| `reshade.rs`, `reshade_module.rs` | None — already deleted by F-14 | Done |

Deleting the five adapters left `src-tauri/src/module.rs` with no production
implementor, as predicted. That is now stated in the file's own module doc and
enforced by the compiler, not just described here: the un-adopted `Module`
trait and the types that exist only to serve it (`ModuleCategory`,
`ModuleContext`, `PreviewReport`, `ApplyResult`, `UpdateInfo`, `UpdateStatus`)
are `#[cfg(test)]`, so they are not compiled into the shipped binary. The rest
of the file — `CheckCategory`, `CheckSeverity`, `CheckDefinition`,
`CheckOutcome`, `VerificationReport`, `ModuleStatus` — stays live, because
`capability_runner.rs` returns those to the UI.

## Exact header comments to add

Still outstanding. One `//!` line at the very top of each file, above any
existing content. `obs.rs`, `ofxr.rs`, `openxr.rs`, `optiscaler.rs`,
`uevr.rs`, `cheeky.rs` and `updates.rs` open with a `use` statement today, so
the line becomes line 1.

```rust
// obs.rs
//! Authoritative OBS VR implementation — the three `#[tauri::command]`s invoked from src/App.vue. See docs/BACKEND-MODULES.md.

// ofxr.rs
//! Authoritative OFXR FrameGen implementation — the three `#[tauri::command]`s invoked from src/App.vue. See docs/BACKEND-MODULES.md.

// openxr.rs
//! Authoritative OpenXR runtime manager — the three `#[tauri::command]`s invoked from src/features/openxr/service.ts. See docs/BACKEND-MODULES.md.

// optiscaler.rs
//! Authoritative OptiScaler installer — the three `#[tauri::command]`s invoked from src/App.vue. See docs/BACKEND-MODULES.md.

// uevr.rs
//! Authoritative UEVR installer — the three `#[tauri::command]`s invoked from src/App.vue. See docs/BACKEND-MODULES.md.

// updates.rs
//! Authoritative per-module version check (`check_module_update`) — this is not an app self-updater. See docs/BACKEND-MODULES.md.

// cheeky.rs
//! Authoritative Cheeky Foveated DLSS commands; a capability spec for the same mod also ships and is also live. See docs/BACKEND-MODULES.md.
```

The header lines this document originally proposed for the five adapters and
`updater.rs` are moot — those files no longer exist.

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
by the F-14 cleanup while this document was being written. The five
`*_module.rs` adapters and `updater.rs` were deleted afterwards, on the
evidence recorded above. `git log --diff-filter=A` still dates them all at the
commits above.

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
  document only established that it has no production implementor today. What
  it *should* become is a design decision, not a reachability finding.
  **Partially resolved:** the trait and its private satellites are now
  `#[cfg(test)]` so they cost nothing in the shipped binary, and the three
  items nothing constructs even in tests carry a reasoned
  `#[allow(dead_code, reason = ...)]`. That keeps the contract legible
  without pretending it is used. Whether to revive it is still open.
* **The `ofxr-framegen` catalogue card** is now offered by nothing, because
  the only thing that made `check:catalog` see it was a dead adapter's `fn id`.
  See the OFXR section above. Owned by `src/catalog/**`.
* **The Rust-command vs YAML-capability duplication** for `ofxr-bridge`,
  `optiscaler`, `openxr-helpers` and `uevr` is real but out of scope here.
  Both are live; neither is dead. ReShade has been resolved in favour of the
  YAML, but that was a deletion, not a rule — establishing which one a
  maintainer *should* edit for the other four requires a product decision this
  document does not make.
* **Whether F-14's deletions are still in flight.** This document reflects an
  uncommitted worktree. If the ReShade commands are restored, the ReShade
  section needs redoing; check `git status --short src-tauri/src/` first.
