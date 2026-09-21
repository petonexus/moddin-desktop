# Merge notes — reconciling the local WIP stash with `refactor/scope-and-rename`

The work in this branch already incorporates the parts of your pending
WIP stash that were safe to land without conflict. After you run
`git checkout main && git stash pop` in your working copy, the areas
below may need a manual three-way merge.

## What this branch already took from the stash

These files were extracted from `stash@{0}` (untracked portion) and
committed as part of the catalog preset work:

- `src/catalog/games/dawnwalker.yaml`
- `src/catalog/games/dead-island-2.yaml`
- `src/catalog/games/stalker-2.yaml`

The originals are still safe inside `stash@{0}`; running `stash pop`
will warn about the overwrite but the contents on disk already match
the version this branch committed.

## Likely conflict zones

### `src/types/game.ts`

The branch rewrote this file to add `GameStore`, `EnginePreset`, and
the `enginePreset?` field on `GameCatalogEntry`. Your stash contains
an in-progress refactor that also touches this file (likely moving
`steamAppId` to optional and adding `epicAppId`).

**Expected resolution:** keep this branch's version. The optional
`steamAppId?` / `epicAppId?` shape matches what your `catalog.ts`
refactor was also moving toward, and the Zod validator in
`src/services/catalog.ts` already enforces "at least one of the two
must be present" with `.refine(...)`.

### `src/services/catalog.ts`

The branch rewrote this file to (a) make `steamAppId` / `epicAppId`
optional with a `.refine(...)` guard, (b) load engine presets via
`src/services/preset.ts`, and (c) add the `byEpicAppId` index plus
`findCatalogGameByEpicAppId` / `findCatalogGameByInstalledGame`.

Your stash touches the same file (your `git stash show --stat` shows
`src/services/catalog.ts | 29 +++++-`).

**Expected resolution:** keep this branch's version. It already
incorporates the same direction (steam/epic flexibility, separate
indexes per store).

### `src-tauri/src/lib.rs`

The branch adds the `mod reshade;` declaration and registers three
new Tauri commands (`preview_reshade`, `install_reshade`,
`uninstall_reshade`). If your stash also touches `lib.rs`, prefer
this branch's `invoke_handler` list — it has the new ReShade commands
plus the renamed crate (`moddin_desktop_lib::run()`).

### `src-tauri/src/inspection.rs`

The branch did **not** touch this file. Your stash contains
`src-tauri/src/inspection.rs | 49 +++++++++-` — likely the engine
detection hardening you were working on. Take the stash's version;
there is no conflict on this branch.

### `src-tauri/src/steam.rs`

Same: branch did not touch. Your stash contains
`src-tauri/src/steam.rs | 161 ++++++++++++++++++++++++++++++--`. Take
the stash's version; merge it into `main` after `stash pop`.

### Vue / i18n / OpenXR feature work

`src/App.vue`, `src/features/openxr/*`, `src/i18n/locales/*.ts` —
branch did not touch. Take the stash's versions on `main`.

## Recommended merge order

1. `git checkout main`
2. `git merge --no-ff refactor/scope-and-rename` — merge the branch
   first. Conflicts in `src/types/game.ts`, `src/services/catalog.ts`,
   and `src-tauri/src/lib.rs` will appear; resolve them in favour of
   the branch as described above.
3. `git stash pop` — apply the remaining WIP changes (steam.rs,
   inspection.rs, App.vue, i18n, OpenXR feature). Resolve any
   remaining conflicts in favour of the stash where the branch did not
   touch, and re-run `npm run build` + `cargo test`.
4. Commit the merged result and open a PR back into `main` if you
   prefer a review gate, or fast-forward `main` if you reviewed
   already.

## Sanity check after merging

```powershell
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

Both should pass. If `cargo test` shows new failures from the steam.rs
or inspection.rs work, that's expected — your WIP changes were not
exercised by the existing test suite and may need follow-up coverage.