# Moddin frontend architecture

This document defines the target structure for the Vue frontend while Moddin grows from a bootstrap application into a reusable game tooling manager.

## Why this exists

The first product slices were intentionally built quickly. That proved the native module model, but the frontend accumulated most orchestration, presentation and module-specific UI in `src/App.vue`.

The application should now grow by **feature boundaries**, not by adding more state, dialogs and conditionals to the root view.

The goal is incremental decomposition. Working install/rollback behavior must not be rewritten merely to satisfy a folder structure.

## Frontend boundaries

### `App.vue`

`App.vue` is the workspace coordinator. Its long-term responsibilities are limited to:

- current top-level view;
- selected game;
- shell-level notifications;
- composing feature views.

It should not own the implementation details for every installer, verification checklist, update checker or modal.

### `components/shell`

Owns application-shell composition that is independent from the selected game.

Examples:

- global tools;
- future command palette;
- navigation shell;
- notification host.

`GlobalTools.vue` is the first extraction in this direction.

### `components/library`

Target home for the installed-game browser and selected-game workspace.

Suggested components:

- `LibraryHeader.vue`
- `GameFilters.vue`
- `GameList.vue`
- `GameDetails.vue`
- `SetupOverview.vue`

### `components/modules`

Target home for reusable module presentation.

Suggested components:

- `ModuleGrid.vue`
- `ModuleCard.vue`
- `ModuleVerification.vue`
- `ModuleUpdateStatus.vue`
- `CompatibilityPanel.vue`

A generic module component may receive a `ToolModuleDefinition`, but it must not contain native installation logic.

### `features/<module>`

When a module requires substantial UI of its own, prefer a feature folder instead of adding another special case to `App.vue`.

Example:

```text
src/features/uevr/
  components/
  requests.ts
  state.ts
  labels.ts
```

The feature may build typed requests and interpret previews. Native filesystem/process behavior remains in Rust.

### `composables`

Stateful reusable frontend behavior belongs here.

Good candidates for extraction from the current root view:

- `useInstalledGames`
- `useSelectedGameInspection`
- `useModuleVerification`
- `useModuleUpdates`
- `useTransactions`
- `useCompatibilityReports`

A composable should expose state and commands with a small public API. It should not know about layout.

### `services`

Stateless adapters and external boundaries live here.

Examples:

- catalog loading;
- future typed Tauri command gateway;
- safe persisted preferences;
- release metadata adapters.

### `types`

Keep transport/domain interfaces separate from presentation state. Prefer explicit types over large anonymous object shapes in components.

## Native command boundary

The UI may:

- build a typed request from the selected game plus catalog configuration;
- ask Rust for preview/inspection data;
- present the preview;
- invoke an explicit mutation after the user confirms it;
- refresh verification and transactions afterwards.

The UI must not:

- copy/delete arbitrary game files directly;
- run arbitrary PowerShell as the normal module path;
- duplicate game-specific installation logic that belongs in a catalog recipe or native module;
- consider a mutation successful without the native command succeeding.

## Styling

The stylesheet entrypoint is `src/styles/index.css`.

Current cascade:

1. `style.css` — legacy application styles while the large root view is decomposed;
2. `environment.css` — environment/inspection-specific legacy styles;
3. `styles/tokens.css` — shared product tokens and compatibility aliases;
4. `styles/polish.css` — cross-component interaction and visual polish.

New colors, radii, text tones and semantic states should use tokens instead of introducing one-off values when practical.

Do not append another redesign block to the bottom of `style.css`. When a component is extracted from `App.vue`, move its layout styles with it or into a clearly named stylesheet and remove the obsolete legacy selectors.

The UI must preserve:

- visible keyboard focus;
- disabled states for unsafe actions;
- reduced-motion preferences;
- readable contrast;
- keyboard-accessible native controls where possible.

## Internationalization

The current shared `i18n.ts` and the local dictionaries inside some global tools are transitional.

Target structure:

```text
src/i18n/
  index.ts
  locales/
    pt-BR.ts
    en.ts
    es.ts
```

Rules:

- user-facing strings should not be added directly to feature logic;
- game/module names from the catalog may remain data;
- safety messages that depend on a native result should have stable translation keys whenever possible;
- locale persistence remains a frontend concern.

## State and persistence

Not every state belongs in global storage.

Use in-memory component/composable state for transient UI such as loading, open dialogs and filters. Persist only user preferences or local evidence that must survive application restarts, such as locale, compatibility test results and per-game selections.

Persistent writes must tolerate unavailable browser storage because Tauri/webview policy can change independently from product logic.

## Module lifecycle

All user-visible modules should converge on the same lifecycle vocabulary:

```text
inspect -> verify -> preview -> apply -> verify -> update/remove -> rollback
```

The UI should make these states visible without requiring users to understand DLL injection or filesystem layout.

Technical evidence belongs in expandable diagnostics. The primary card should answer three questions quickly:

1. What is this?
2. What state is it in?
3. What is the safe next action?

## Activity logging

`invokeDebug` is the frontend gateway for commands that need diagnostics. Mutating commands that are meaningful to the user must be included in the persistent activity allowlist.

Preview and inspection calls should remain in the bounded debug stream rather than filling the persistent action log.

When a new module gains install/uninstall actions, updating the persistent command allowlist is part of the feature definition of done.

## Refactor sequence

The preferred order is deliberately incremental:

1. establish shared style tokens and shell boundaries;
2. extract library/game-list presentation;
3. extract module-card presentation;
4. move verification/update orchestration into composables;
5. move large module-specific request builders into feature folders;
6. split translations by locale;
7. remove obsolete rules from the legacy stylesheet;
8. keep `App.vue` as a small coordinator.

Every step must keep `npm run build`, Rust tests and `cargo check` green.
