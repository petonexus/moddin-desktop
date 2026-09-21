# Moddin frontend architecture

This document describes the current Vue frontend boundaries and the remaining decomposition work as Moddin grows into a reusable game tooling manager.

## Design rule

The frontend grows by **feature boundaries**, not by adding more state, dialogs and native-command branches to `src/App.vue`.

Refactoring is incremental. Working install/rollback behavior must not be rewritten merely to satisfy a folder structure. Prefer small moves that preserve contracts and can be reviewed independently.

## Current structure

```text
src/
  App.vue                         # legacy workspace coordinator; still the main hotspot
  Root.vue                        # root composition

  components/
    shell/
      GlobalTools.vue             # shell-owned global tool dock
      global-tools.css

  composables/
    useDialogLifecycle.ts         # shared modal keyboard/focus behavior

  features/
    activity/
      ActivityLogPanel.vue
      activity-log-panel.css
      copy.ts
      service.ts
      types.ts
      useActivityLogPanel.ts

    openxr/
      OpenXrManager.vue
      openxr-manager.css
      copy.ts
      service.ts
      types.ts
      useOpenXrManager.ts

  i18n/
    locale.ts                     # locale resolution/options/date locale
    localizedCopy.ts              # typed feature-copy helper
    locales/
      pt-BR.ts
      en.ts
      es.ts
  i18n.ts                         # vue-i18n bootstrap only

  services/
    activity-log.ts               # persistent action logging infrastructure
    catalog.ts                    # validated/indexed YAML catalog
    debug-panel.ts                # emergency runtime error panel
    storage.ts                    # safe localStorage boundary

  styles/
    index.css                     # explicit global cascade entrypoint
    base.css
    module-states.css
    library-layout.css
    tokens.css
    polish.css

  types/                          # genuinely shared domain/transport types
```

## `App.vue`

`App.vue` remains the largest legacy hotspot. Its long-term responsibilities are limited to:

- current top-level view;
- selected game;
- shell-level notifications;
- composing library/module feature views.

It should not own implementation details for every installer, verification checklist, update checker or modal.

Because the GitHub connector currently replaces whole files instead of applying partial patches, decomposing this 100+ KB file must be done conservatively. Do not reconstruct it blindly just to make the file smaller.

The architecture guard freezes its current growth budget while extraction continues.

## Shell

`src/components/shell` owns application-shell composition that is independent from the selected game.

Current example:

- `GlobalTools.vue` composes Activity and OpenXR;
- `global-tools.css` owns the placement/gap of their trigger dock;
- each feature still owns its own dialog and behavior.

Future shell-level concerns may include navigation, notification hosting or a command palette.

## Feature boundary

Substantial UI should live in `src/features/<feature>`.

A feature should colocate what belongs specifically to it:

```text
src/features/<feature>/
  FeatureView.vue
  feature-view.css
  copy.ts
  service.ts
  types.ts
  useFeature.ts
```

Not every feature requires every file. Create boundaries because responsibilities exist, not to satisfy a template.

### View

The `.vue` file should primarily render state and connect user interactions to the feature API.

It should not contain direct Tauri command names or large dictionaries of translated copy.

### `service.ts`

Feature-native commands live behind a typed service boundary. Services use `invokeDebug` so diagnostics and persistent action logging remain consistent.

New native-command access should not be added directly to feature views/composables.

### Composable

Stateful orchestration belongs in `useFeature.ts` when it has enough behavior to justify extraction.

Examples already implemented:

- Activity filtering/loading/clearing/expanded state;
- OpenXR discovery, selected game persistence, inspection and runtime mutations.

A composable should expose a small public API and should not know about CSS/layout.

### `types.ts`

Types that belong to only one feature stay with that feature. Types used across multiple domains remain under `src/types`.

Example: OpenXR runtime types are feature-owned; `InstalledGame` remains shared because the library and OpenXR both use it.

## Library and module presentation

The next major extraction from `App.vue` should be presentation that does not own native mutation logic.

Likely boundaries:

```text
src/features/library/
  LibraryHeader.vue
  GameFilters.vue
  GameList.vue
  GameDetails.vue
  SetupOverview.vue

src/features/modules/
  ModuleGrid.vue
  ModuleCard.vue
  ModuleVerification.vue
  ModuleUpdateStatus.vue
  CompatibilityPanel.vue
```

A generic module component may receive a `ToolModuleDefinition`, but it must not contain native installation logic.

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

Direct `@tauri-apps/api/core` usage is infrastructure-only. Feature commands should go through `invokeDebug` from a feature service.

## Catalog boundary

Game recipes under `src/catalog/games/*.yaml` are auto-discovered.

`src/services/catalog.ts`:

- validates each YAML with Zod;
- rejects duplicate game ids;
- rejects duplicate Steam App IDs or Epic App IDs;
- rejects duplicate module ids inside one game;
- indexes entries by game id and store-specific App ID.

Adding a normal game recipe should not require registering it manually in TypeScript.

## Styling

The global stylesheet entrypoint is `src/styles/index.css`.

Current cascade:

1. `styles/base.css` — historical application base while `App.vue` is decomposed;
2. `styles/module-states.css` — shared module-state presentation;
3. `styles/library-layout.css` — current library/workspace layout layer;
4. `environment.css` — environment/inspection-specific legacy styles;
5. `styles/tokens.css` — shared product tokens and compatibility aliases;
6. `styles/polish.css` — final cross-component interaction/visual polish.

`src/style.css` must not return.

Feature-owned styles should be colocated with the feature and loaded through `<style scoped src="...">` when appropriate.

Do not append another redesign block to a legacy stylesheet. When a component is extracted, move its effective rules with it and consolidate old-plus-override declarations into their final values.

The UI must preserve:

- visible keyboard focus;
- disabled states for unsafe actions;
- reduced-motion preferences;
- readable contrast;
- keyboard-accessible controls;
- modal focus containment and Escape dismissal.

## Dialog lifecycle

`useDialogLifecycle` is the shared boundary for modal behavior.

Current guarantees:

- remembers the control that opened the dialog;
- moves focus into the dialog after it renders;
- traps `Tab` and `Shift+Tab` inside the dialog;
- closes on Escape;
- returns focus to the opener on close;
- removes global listeners when the consumer unmounts.

Do not reimplement those behaviors independently in a feature.

## Internationalization

Shared locale infrastructure lives under `src/i18n`.

- `locale.ts` owns supported locales, fallback resolution and date-locale mapping;
- `locales/*.ts` contains application-wide translations;
- `localizedCopy.ts` provides typed local copy for feature-specific text;
- `i18n.ts` only boots `vue-i18n` and preserves public exports.

PT-BR is currently the key-shape reference. EN and ES must implement matching keys at compile time.

Rules:

- user-facing strings should not be embedded in feature logic;
- game/module names from the catalog may remain data;
- stable native-result/safety messages should prefer translation keys when practical;
- locale persistence is a frontend concern handled through the safe storage boundary.

## State and persistence

Use in-memory component/composable state for transient UI such as loading, open dialogs and filters.

Persist only preferences/evidence that must survive restarts, such as:

- locale;
- compatibility test results;
- selected per-game options.

All direct local storage access should go through `src/services/storage.ts`, which tolerates unavailable or policy-restricted WebView storage without throwing.

## Activity logging and debug

Developer diagnostics and user-visible action history are separate concerns.

- `debug.ts` owns the bounded debug stream, runtime instrumentation and the `invokeDebug` gateway;
- `services/activity-log.ts` owns persistent action-log policy/payloads;
- `services/debug-panel.ts` owns the emergency DOM error panel;
- Activity feature owns reading/filtering/clearing the user-visible history.

Mutating commands meaningful to users must be present in the persistent activity allowlist. Preview/inspection calls should remain in the bounded debug stream.

A failure to persist diagnostics must never turn a successful mod/game mutation into a failure.

## Module lifecycle

All user-visible modules should converge on the same lifecycle vocabulary:

```text
inspect -> verify -> preview -> apply -> verify -> update/remove -> rollback
```

Technical evidence belongs in expandable diagnostics. The primary card should answer three questions quickly:

1. What is this?
2. What state is it in?
3. What is the safe next action?

## Architecture guard

`npm run check:architecture` runs `scripts/check-frontend-architecture.mjs` and is part of CI before the frontend build.

Current guardrails intentionally encode the migration state:

- `App.vue` may not grow past 115 KB while it is being decomposed;
- other Vue files are limited to 30 KB;
- feature views may not drift back into the generic `src/components/` root;
- `src/style.css` may not be recreated;
- `src/i18n.ts` must remain a small bootstrap;
- direct Tauri-core access is restricted to infrastructure boundaries;
- new `invokeDebug` consumers must use feature service boundaries, with a temporary exception for legacy `App.vue`.

These are maintenance budgets, not permanent product constraints. Tighten or remove legacy exceptions as extraction progresses.

## Remaining refactor sequence

Completed foundations:

- shared style tokens and explicit style layers;
- shell/global-tools boundary and dock;
- split translations and typed feature copy;
- safe storage boundary;
- Activity feature decomposition;
- OpenXR feature decomposition;
- shared dialog lifecycle/focus trap;
- catalog auto-discovery, validation and indexes;
- frontend architecture guard.

Highest-value remaining work:

1. extract library/game-list presentation from `App.vue`;
2. extract module-card/verification/update presentation;
3. move verification/update orchestration into focused composables;
4. move module-specific request builders into feature folders;
5. shrink `App.vue` to workspace coordination;
6. continue removing obsolete selectors from `polish.css`/legacy layers as ownership moves.

Every code change should keep architecture checks, `npm run build`, Rust tests and `cargo check` green whenever CI runners are available.
