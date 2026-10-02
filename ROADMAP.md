# Roadmap

Last rewritten on 2026-09-30, after a fourth pass. The previous three are preserved in
git history; the evidence lives in [docs/AUDIT-2026-09-29.md](docs/AUDIT-2026-09-29.md), and
every item below cites the audit ID it comes from.

The passes after the audit have one finding in common worth stating once, at the top,
because it is the thing that kept happening: **the guards this project writes are usually
right about the class and wrong about the inventory.** The step-kind guard compared four of
five lists and the fifth had already drifted. The engine vocabulary now lives in five places
and the guard compares five. A baseline file in a validator is a sixth place a defect can be
named. Each time, the omission was found by a human reading the source, not by a tool.

The fourth pass is the sharpest instance, and it is worth reading as a method rather than an
anecdote. Generalising two spec invariants — "a step may not reference an undeclared config
field" and "a check may only inspect what its recipe writes" — from one hand-picked recipe to
all nine cost three false positives before the guard was right, and every one of them was the
same mistake:

- `ofxr-bridge` was reported as declaring `sha256` for nothing, while its `archive-sha256`
  **check** reads exactly that field. The first version scanned steps and not checks.
- `bepinex` was reported as checking a file no step writes, when `extract-zip` is what puts
  it there. An extracted archive is the one artifact whose whole tree is the product.
- The sibling rule, "every declared field is consumed", turned out to be **unstatable from
  where the guard sits**: `configSchema` is the union of three consumers in three languages —
  the chain, `src/features/modules/module-registry.ts` for the typed module commands, and
  `build_metadata`, which records `version` on the transaction. Six of six available recipes
  looked broken.

That last one is the general form. **A guard that names a whole category as broken on the day
you write it does not get read the second time**, so the rule that can be stated honestly is
the one that ships, and the rule that cannot is written down as a known gap instead. The
lesson generalises to the three previous cases: find the whole inventory before writing the
check, not after it fails.

## October 1 assessment follow-up

Implemented locally, pending the signed release dry-run:

- Release artifacts use Tauri v2's setup executable and `.sig`; CI generates
  `latest.json`, supplies secrets during the build, validates all three versions
  against the tag, and supports a manual dry-run without publication.
- Capability installation rolls back on planning, backup extension, execution
  and commit failures. Rollback errors report the transaction and original error.
- Community YAML must match `yamlSha256` inside the signed catalog. `SIGNED-BY`
  keys must be compiled/trusted and non-revoked. Consent cannot bypass either
  requirement or downgrade an entry marked signed.
- Community, Collections and Contribute consume the reactive library selection;
  AI receives catalog game IDs. Regression fixtures provide that same channel.
- Runtime debug overlays require opt-in/development mode and can be dismissed.
  Vue template checking is strict; the prior UI pass already imported GameList's
  empty state and isolated capability cards when switching games.
- `kill-process` resolves `processNameField` and refuses missing/broad targets.
  Capability install/removal and community installation enter activity history.

Remaining release/community work: run the Release workflow manually with the
signing secrets, inspect/install its artifacts, then publish the intended tag;
regenerate the external community catalog with YAML digests and re-sign it.
See [UPDATER.md](docs/UPDATER.md) for the dry-run contract.

Next cycle still includes AI permissions/portable resource discovery, safe Undo
across overlapping mods, a beta updater channel, and Authenticode. This follow-up
does not close those items or claim the external catalog was changed.

## How to read this

- **Now** — blocks the next beta. One item needs a maintainer's key; the rest are closed, and one of them was found by auditing a branch this roadmap had repeatedly deferred closing.
- **Next** — the current cycle.
- **Later** — real, but not yet earned.
- Every item has a **Done when** line. An item is not done until that line is true and reviewable.
- Effort is rough: **XS** < 1 h · **S** half a day · **M** 1–2 days.

---

# Now — release blockers

Every P0 and every F-03 remainder is closed. **One thing needs a key rather than a
change**, and one thing that was a change is now closed too — it was found by
cross-checking the branch this roadmap keeps postponing, which is the argument for
closing that item earlier than it wanted.

- [x] **The bundled app had no frontend in it.** — new, found while auditing
      `feat/agent-mcp`
      `src-tauri/Cargo.toml` declared `tauri = { version = "2", features = [] }`.
      Without the `custom-protocol` Cargo feature, `tauri`'s build script takes
      `let dev = !custom_protocol` and emits `cargo:dev`; `tauri-build` then runs
      `cfg_alias("dev", is_dev())` for the whole crate; and `Manager::get_app_url`
      resolves the app URL to `build.devUrl` — `http://127.0.0.1:1420` — instead
      of the embedded `frontendDist`.

      *This is not a defect that announces itself, which is why it survived
      forty-seven commits of hardening.* `cargo check` passes, `cargo build
      --release` passes, `beforeBuildCommand` runs, `dist/` is produced, the
      installer builds, installs and launches. The only symptom is a blank
      WebView2 on a user's machine. And it was not a theory: the release build
      directory on this machine, dated earlier today, records
      `cargo:rustc-cfg=dev` and has **no `tauri-codegen-assets` directory at
      all** — the exact state the mechanism predicts. The sibling build from the
      `feat/agent-mcp` branch, taken a week earlier, records `dev=false` and
      fifty-six embedded asset files.

      It arrived on `feat/agent-mcp` as `ef9553c` and was never merged, because
      the roadmap read that branch as "partly cherry-picked" product work rather
      than as a fix that belonged on this one. **A merge is not the only way a
      commit reaches a release, and "not in main" is not the same as "not
      needed".**

      `scripts/check-bundle-features.mjs` now gates the manifest, and it walks
      for every `Cargo.toml` rather than naming `src-tauri/Cargo.toml`, for the
      reason at the top of this file: a guard is only as good as its inventory.
      It declares no dependencies and runs through `node` directly, so it is the
      first step in CI, before `npm ci`. *The limit is written into the guard
      itself:* it reads the manifest, not the artifact, and cargo does not
      re-run a dependency's build script when a feature flips — so a stale
      target directory can hold the old binary. **The artifact-level proof is
      the dry-run tag below**, and nothing short of it should be called
      verification.

- [ ] **Re-sign the community catalogue.** — `F-11`, XS, **by hand only**
      `catalog.json` changed twice today — the revocations moved inside it, and the engine
      vocabulary is now enforced against it — so it must be regenerated and signed once, with
      the final format. `regenerate_catalog.py` refuses to sign without `MODDIN_SIGNING_KEY`,
      which is not in this environment and must not be pasted into a chat or a commit.
      Until it is signed the desktop app fails signature verification on the community fetch.
      That is correct fail-closed behaviour, and it is release-blocking by hand.
      *Also worth doing before announcing a release:* a dry-run tag. It is the first
      installer built by CI with a digest computed from the artifact rather than typed beside
      it, and the updater's signing setup is not complete either.

Everything below was closed in this pass. It is kept rather than deleted, because the
reasoning is what the next person needs:

- [x] **Capability engine integrity.** — `P0-1`, `P0-2`, `P0-3`, `F-02`
      `download-file` in the built-in specs, the transaction opened before the step loop,
      `bepinex`/`ue4ss`/`reframework` registered, steps constrained to the install root, and
      `SKILL.md` corrected to say what is actually enforced — that a step cannot traverse out
      of the root, and that the store will not back anything up outside it. Absolute paths
      from `path`-typed config fields stay honoured, because those fields exist to name a
      location outside the game folder; both the refusal and the exception are stated now.

- [x] **The four step kinds the last two recipes needed.** — `P0-1` follow-ups
      `extract-zip` gained a target subdirectory, `move-file` gained directory support,
      `git-checkout` and `build-project` are new. `ofxr-bridge` is `available`; `uevr` stays
      `planned` and its `safetyNotes` say why, which is not the reason the roadmap assumed —
      the chain is correct, but UEVR publishes prebuilt archives from three repositories, so
      its `backend` enum is a matrix one checkout and one build cannot express.

- [x] **Trust: the pin, the cache, the fetch host, the revocations.** — `F-03`, all four
      The key pin is checked against a compiled keyring. The cached catalogue is re-verified
      against the signature on **each read** rather than trusting a `meta.json` flag. The YAML
      fetch has a host allow-list. And the revocations now travel **inside the signed
      `catalog.json`**, so one signature covers both halves and they cannot disagree —
      before, `revoked-ids.json` was honoured over a connection the app verified and trusted
      anyway, which meant an intercepted GET could suppress a revocation until the TTL
      expired. The empty case is a table in `SECURITY.md`, not a paragraph.

- [x] **The id-space bug, the missing descriptions, the dead commands.** — `P0-5`, `P0-6`, `F-14`
      *F-14 gained a rule, and it is a rule rather than a fact:* the snapshot commands were
      deleted for having no caller and then restored. Deleting a user-facing feature because
      the wiring was outside an agent's file ownership trades a visible problem for an
      invisible one. An unreachable command is a question — wire it, or say why it is not
      wanted. Two deletions were the opposite call and both were right: ReShade's Rust module
      was unreachable while the declarative path was live, and GOG discovery shipped in the
      build with no caller at all, so its scan now runs where the UI actually calls.

- [x] **The release job builds what it signs.** — `P0-8`, `O-06`
      Plus a follow-up the notes move exposed: the job read its body from the repo root, where
      the notes no longer are, and the next tag would have failed.

      *Done when:* `ofxr-bridge` is no longer `planned` because the engine lacked a step
      kind. It is now `available`: download, extract to staging, promote the manifest, write
      the tray, start it. Its `safetyNotes` name the one element this repository cannot
      verify — the manifest filename inside the archive — and say that a wrong name fails
      loudly and rolls back rather than installing nothing.

      `uevr` stays `planned`, and the reason is not the one the roadmap assumed. The chain
      is correct; the recipe is not yet self-contained. UEVR publishes prebuilt archives
      from three different repositories, so its `backend` enum is a repository matrix that
      one checkout and one build command cannot express. Its `safetyNotes` say so.

- [x] **Add a registry test that parses every `include_str!` spec** and asserts each install
      chain's first step resolves to a local archive source. — `P0-1` guard

### Trust and safety

- [x] **Pin the community key against a compiled keyring.** — `F-03` (key pinning)
- [x] **Re-verify the cached catalogue before using its `downloadUrl`.** — `F-03` remainder
      The cache stored a pretty-printed re-serialisation and no signature, and `fetch()`
      copied a `signatureVerified` flag out of a local `meta.json` any writer could set. The
      cache now keeps the raw bytes plus the signature and verifies against every
      non-revoked key in the keyring on **each read**; the flag is derived from an actual
      verification. Missing or corrupt signature is a cache miss, not a fallback.
- [x] **Allow-list the YAML fetch host.** — `F-03` remainder
      The default adds `raw.githubusercontent.com`, because every `downloadUrl` in the
      catalogue points there and matching `download-file`'s list byte for byte would have
      made community installs impossible rather than safer.
- [x] **Honour `revoked-ids.json` in the app.** — `F-03` remainder
      Fail-closed: no list, unreadable list, non-2xx and unparseable content all refuse. A
      *network* failure falls back to the cached list, which is what `SECURITY.md` promises; a
      *corrupt* list does not, because a bad upstream file is not masked by a stale one. The
      kill switch is consulted before the signature, deliberately — a tampered list can only
      add refusals, never remove one. A revoked capability stays visible with the maintainers'
      reason rather than silently vanishing.
- [x] **Sign `revoked-ids.json`, or fold it into the signed catalogue.** — `F-03`, new
      The revocation list was fetched over a verified connection but was **not itself signed**:
      an attacker who can intercept that one GET could suppress a revocation until the TTL
      expired, and the cache re-verification added in this pass does not help, because the list
      is trusted precisely when it is stale. The revocations now live **inside** the
      already-signed `catalog.json`, which closes it without a second signature to manage. The
      "kill switch before the signature" ordering was deliberately kept even though both are
      verified in the same read now: a tampered catalogue can only add refusals, never remove
      one, and a revocation that is visible to an attacker who can suppress it is not a
      revocation. *Consequence, by hand:* the published `catalog.json` now fails the drift
      check until the maintainer regenerates and signs it — see `docs/UPDATER.md`.

### Two broken user paths

- [x] **Fix the id-space bug in the AI install flow.** — `P0-5`
- [x] **Render a human description on capability cards.** — `P0-6`
- [x] **Wire or delete the unused backend commands** — `F-14`
      `capability_reload` is wired, so a locally dropped recipe shows up without a restart.
      `get_compat_report`, `reshade`, `reshade_module`, `library_state` and seven orphaned
      snapshot helpers are gone. `validate_recommendations_yaml` is **kept** — the audit was
      wrong, it is called from the AI assistant.
      *The snapshot commands were deleted for having no caller, then restored.* That is the
      rule this item produced: an unreachable command is a question — wire it, or say why it
      is not wanted. Deleting a user-facing feature because the wiring was outside the
      current file ownership trades a visible problem for an invisible one.
      *Two of the deletions were the opposite call and both were right.* ReShade's Rust module
      was unreachable while the declarative path was live and registered, so it was
      redundancy. GOG was the reverse: `refresh_installed_games_async` was the only caller of
      `detect_gog_installed_games_blocking` and had no caller itself, so GOG discovery shipped
      in the build and was unreachable in the app. The scan now runs where the UI calls.

- [x] **Fix `create_snapshot`, which was not per-game.** — new, found while wiring the UI
      The filter compared `record.game_id` against a helper returning `record.game_id.clone()`
      — a tautology, under a `placeholder for future per-game filtering` note the original
      author left. A snapshot of Elden Ring captured every applied transaction across every
      installed game, and rolling it back undid mods in titles the user never named. It also
      made the most destructive button in the app a silent no-op after one use, because
      `rollback_snapshot` skips spent transactions and returns nothing. Both fixed; a spent
      snapshot now disables its own button, and restore is deliberately *not* gated on
      revocation — refusing it would strand files the user cannot remove.

### Delivery

- [x] **Make `release.yml` build the installer.** — `P0-8`
      *Follow-up, found when the release notes moved:* the job read its body from
      `.release-notes-<tag>.md` at the repo root, and the notes now live in
      `docs/release-notes/`. The next tag would have failed. Fixed.
- [x] **Verify the bundled runtime.** — `O-06`
- [x] **Give the staged runtime a lockfile.** — `O-06`, second half
      The blocker was that the generated manifest lives under a gitignored path — but the
      lockfile that matters already existed and was already tracked:
      `moddin-agent/package-lock.json`. The staged folder copies it in and installs with
      `npm ci --omit=dev` against exact-pinned versions. Two consecutive runs produce an
      identical `node_modules`: 3,869 files, same tree hash. Pinning the manifest alone would
      not have been enough — it covers four direct dependencies and leaves ninety-one
      transitives to the registry.
      *Also found:* the script threw on Windows PowerShell 5.1, because `ConvertFrom-Json`
      rejects the lockfile's `packages[""]` key. CI runs `pwsh` 7, so it would have shipped
      green and broken for every maintainer on 5.1.

---

# Next — current cycle

## Quality gates

- [x] **Add the missing quality gates.** — `F-12`, `O-07`
      A pinned `rust-toolchain.toml` (1.98.1, the version `stable-msvc` resolved to, so the
      build is unchanged and no longer floats) and `vitest`, now at **195 tests** over the
      files where behaviour has broken before.
- [x] **Turn `fmt` and `clippy` into blocking gates.** — `O-07`
      Done in the order that makes it meaningful: delete the dead adapters, fix the real
      lints, *then* turn the gates on. The CI count of "144 / 30" was raw emissions including
      duplicates across the lib, lib-test and example targets; deduplicated it was **81 unique
      diagnostics, 23 lints, 34 of them `dead_code`**. The tree is now **zero** and both gates
      block.
      Six files went: the five `*_module.rs` adapters and `updater.rs`, each confirmed
      unreferenced by a repo-wide search before deletion. The cascade was accounted for rather
      than tolerated, and it found three real things — `archive.rs`'s SHA-256 layer could never
      verify anything because `with_expected_sha256` was never called; `StepContext::spec` was
      never read; and `ai_assistant_setup::restore_from_backup` is never called, so setup
      writes a backup nothing restores. The third is a genuine gap in the config-write safety
      net, left flagged rather than fixed here, because wiring it is a behaviour change.
      `too_many_arguments` got the real fix: adjacent same-typed `&Path` parameters that
      could be swapped silently and point the backup scope at the wrong root are one
      `InstallScope` now. Four `#[allow]`s remain, each item-scoped and each with a `reason`;
      no `[lints]` block was added, because the tree is clean without one and a blanket allow
      would turn the gate into a switched-off guard.

- [x] **Migrate the `ofxr-framegen` module cards.** — consequence of the deletion, new
      Deleting `ofxr_module.rs` removed the crate's only `"ofxr-framegen"` string, which is the
      pattern `validate-catalog.mjs` builds its module inventory from. Five catalogues were
      advertising a card whose only backing was a file nothing dispatched to — the same dead
      card `reshade` was. The module is now `ofxr-bridge` in all five, its dead config block
      dropped, and it joins the capability-backed list in `App.vue`. No Rust shim was added to
      satisfy the check; that would be creating dead code to turn a number green.

## Recipe-kind parity

- [x] **Sync the recipe-kind lists across every repo that declares one** — `F-05`, `F-06`
- [x] **Add a guard so the lists cannot drift again** — `F-05`
      `scripts/check-capability-kind-parity.mjs` reads the lists and fails the build on any
      disagreement.
- [x] **Cover the fifth source, and check params, not just names.** — `F-05`, new
      `moddin-agent/src/mcp-server.mjs` keeps its own `STEP_KINDS` map, feeds it to the agent
      through `get_step_kinds`, and was not in the guard. It had **already drifted**: two
      step kinds and one check kind missing, and its `registry-write` entry documented
      `keyField`/`nameField`/`typeField`/`dataField` while the Rust step reads
      `key`/`value`/`type`. An agent authoring a recipe through MCP would have produced a
      step that passed every kind check and failed at install time with "key is required".

      Kind parity turned out to be necessary but not sufficient, so the guard now also
      verifies that every param name the map advertises is a name the runner reads. That
      check was written per-function first and produced five false positives, because several
      steps resolve their path through a shared helper whose literals live in another
      function's body. It is file-global instead: weaker, but it cannot cry wolf, and a guard
      people ignore protects nothing.

- [x] **Guard the engine vocabulary too, and the gate's verdicts.** — new
      `supportedEngines` is now enforced, which made the vocabulary load-bearing in the
      community validator, which made it a **fifth** list. The guard compares four
      vocabularies across five sources — step kinds, check kinds, engine ids, engine-match
      verdicts — and every one of the four was found by a human reading the source.

## Catalogue integrity

- [x] **Gate capabilities on the engine.** — `F-09`
      In Rust, because it is the only layer holding the spec while the list is built. A game
      whose `enginePreset` no recipe names gets everything — `doom-2016` is `idtech` and
      nothing Moddin ships targets it, so a strict gate there empties the page. An empty
      `supportedEngines` means every engine, on the evidence of the repository's own template
      and the only shipped community recipe; the opposite reading would have made that recipe
      uninstallable everywhere. `elden-ring` declares no engine at all and is the flagship VR
      title, so gating on an undeclared engine would hide UEVR from the game people most use
      it with. Both are written down rather than left to the next reader.

- [x] **Validate the catalogue in CI.** — new
      `scripts/validate-catalog.mjs` checks a wrong `executable`, an `enginePreset` no preset
      defines, a module whose id nothing implements, a game pointing at another game's
      directory, and a game offering a module its own preset's description excludes — the
      `dead-island-2` shape, read from the preset rather than matched by hand. It found eight
      real defects and reports them by name; see **Later**.

## Product surface

- [x] **Profile import/export.** — Later item, closed
      A versioned `moddin-profile` document, schema version checked *before* the parse because
      a future-version file is well-formed JSON and would otherwise be read as v1 with fields
      missing. Applying uses `capability_install` — the same command the cards use — so
      preflight, transaction and rollback are the existing ones and there is no second install
      path. Community capabilities are refused from a profile, because the signature check and
      the revocation kill switch live in `community_capability_install`. The secrets rule is
      one function called on both sides, so a hand-edited file cannot widen it.
      *The roadmap's premise was wrong twice:* the storage layer is not a profile store, it is
      a YAML shape over the catalogue module list with zero callers; and there is no file
      dialog in this app at all, so export writes a named file under `%LOCALAPPDATA%` and
      reveals it in Explorer. A real Save-As needs `tauri-plugin-dialog`.

- [x] **Collections and Contribute.** — `O-04`
      Ported from `feat/agent-mcp` and mounted. The branch drove a backend session with its
      own install/resume/abort — 1,127 lines that would have been a second install path — so
      the frontend is a loop over the existing `community_capability_install` instead, with
      ordered steps, a stop-and-ask on failure, newest-first revert and a transaction record
      per step. Two branch files were deliberately not ported: its `TransactionsView` is a
      worse rendering of data the History panel already owns, and its `LibraryEmptyState` has
      nothing left to own. *The loader has no content* — see **Later**.

- [x] **A real CSP, and 23% off the installer.** — `F-13`
      Verified rather than assumed: the app was launched and the header read out of WebView2
      over CDP with zero console entries. The interesting part is what is *absent* — the MCP
      agent is not loaded into the webview at all, it is written into an external tool's
      config and launched over stdio, so `node://` would have been decoration. The frontend
      makes no `fetch`; every GitHub, catalogue and UEVR request is Rust-side `reqwest`.
      *The size claim was pointing at the wrong file.* The 28.5 MB `node.zip` is not in the
      installer — `bundle.resources` is an explicit list and 7-Zip finds zero zip entries.
      What was shipping is **56 MB**: the bundler copied `moddin-agent/node_modules`, the
      full dev install, renamed it `node_modules.old`, and never deleted the rename. 6,296
      entries, in every build, hidden behind a "best-effort" cleanup that had been failing
      silently. Installer 31,705,095 → 24,403,351; unpacked payload 160,073,106 → 103,975,929.
      *Costs accepted and written down:* `strip` removes the shipped symbol table, so a
      user-reported crash yields no symbols, and `npm ci` now fails where `npm install` would
      have limped.

## UI/UX consistency

- [x] **Tier 1 (XS).** — `UX-30`, `UX-13`, `UX-05`, `UX-04`, `UX-01`
      Design tokens, `pre-line` on error paragraphs, verification passed to `ModuleCard` with
      the duplicated checklist deleted, card state derived from verification and busy flags,
      two labelled sidebar groups and a distinct glyph for IA local.
- [x] **Tier 2 (S).** — `UX-32`, `UX-09`, `UX-16`, `UX-07`, `UX-02`
      One error path (`useFriendlyError` + `ErrorCallout`) replacing three classifiers and ten
      raw `err.message` dumps; one `.empty-state` contract; `BaseDialog` as the only dialog
      implementation, with `useDialogLifecycle` as its only focus entry point; the five AI
      verbs collapsed into one.
- [x] **Tier 3: the localization boundary.** — `UX-21`
      The four backend-authored identifiers now map to locale keys at the service layer, and
      the capability-card description that was English in every locale is the thing it fixes.
      The decision that mattered is the one about ids the table does not declare: they render
      **the backend's own text, verbatim** — the command name, the recipe's own sentence, the
      real I/O message. Not blank, not refused. A log row with no action is untriageable.

      The enforcement is the other half: every table is checked by set equality against the
      source the ids come from — the persistent-action list, the literal `warnings.push`
      strings in `openxr.rs`, the `Some("…")` values in `inspect_agent`, the recipe YAMLs. So
      the fallback is reachable only by a string the app does not know about, and a renamed
      recipe becomes a build failure rather than a stale entry.

      *What it does not cover, stated rather than implied:* a pt-BR card still shows English
      in `safetyNotes`, `configSchema[].description`, `displayName` and each check's `detail`.
      None is one of the four named identifiers; the config-field half is a separate item.
      A community or AI-authored recipe keeps its own English — it is the author's copy.

- [x] **Tier 3: the accessibility sweep.** — `UX-26`, `UX-27`, `UX-28`, `UX-29`
      Per-item `aria-label`s on every repeated icon-only action, built from the visible text so
      label-in-name holds for voice control; `.sr-only` labels on the activity-log controls;
      `aria-required` / `aria-invalid` on config fields, wired to the real state rather than to
      first paint, because a required field technically reads as missing before the user has
      done anything; and `aria-busy` on six regions. The `ToastStack` was announcing every
      message twice — a polite container plus alert children — and the container is now silent.

- [-] **Tier 3: `UX-25`, closed as written.** — its subject no longer exists
      There is no `role="menu"` anywhere in the tree. Tier 2 collapsed the five AI verbs into
      one button and took the menu with it. Inventing a menu to give it a keyboard model would
      be building the opposite of what Tier 2 did.

- [-] **Tier 3: `UX-06`, closed as written, with a real gap found instead.** — its premise is stale
      The three call sites do not share a spec-driven config form. The game page is the only
      one that has one, and it is inline in the card rather than a dialog; the Community panel
      installs with an empty config and has no form; the AI flow shows the field names
      read-only beside a YAML editor. Extracting a shared dialog would have had one caller.

      *The gap it exposed is real and is now a Later item:* the Community panel cannot install
      a recipe that declares required config fields, because it never collects them. That is a
      feature, not a refactor, and it needs field labels first.


## Organization and delivery hygiene

- [x] **Correct the docs that actively mislead** — `O-02`, `O-03`, `O-05`
- [x] **Add a docs index, delete the dead files, rehome the release notes** — `O-12`
- [x] **Write the conventions the history already follows** — `O-13`
      Counted from the real history, not asserted. Two things it surfaced rather than
      smoothed over: `chore/` has never been used for a branch, and `ci.yml` only triggered on
      `main`, `feat/**` and `refactor/**` — so this `fix/` branch, and every `hardening/`,
      `docs/` and `chore/` branch, shipped unverified until someone opened a PR. Fixed.
- [x] **Cache the `Bundle moddin-runtime` CI step** — `O-08`
- [x] **Stop tracking generated and wrong-platform weight** — `O-11`
- [x] **Document which of each `src-tauri/src/*` vs `*_module.rs` pair is authoritative** — `O-09`
      [docs/BACKEND-MODULES.md](docs/BACKEND-MODULES.md). Five of the adapters are
      unreferenced and `updater.rs` has never had a `mod` declaration in any commit, so it has
      never been compiled. *Still open:* whether `module.rs` should survive once the adapters
      go, and the Rust-command/YAML-capability duplication for `ofxr-bridge`, `optiscaler`,
      `openxr-helpers` and `uevr` — both are live, and which one is right is a product call.
- [x] **Decide `App.vue`'s fate, and do it.** — `O-10`
      Decomposed, not the budget raised. **110,278 → 73,213 bytes** on disk; the guard
      measures 107,739 → 71,488 after CRLF→LF normalisation, so headroom went from ~7 KB to
      ~43.5 KB with the 115,000 budget untouched.

      The seams were found before the sizes, and the largest one was the per-module action
      table — the `if (module.id === ...)` chain running through preview, remove, update and
      verify. That became a module the app *imports* rather than a branch it runs, and it now
      has 23 tests holding it against the three things it claims to mirror: the catalogue, the
      backend's `invoke_handler`, and the services that actually invoke each command. Two
      module ids have already been dead cards in this repo's history — `reshade` and
      `ofxr-framegen` — and both were found by a catalogue validator rather than by the guard.
      That test is what makes the next one a build failure instead of a support ticket.

      It was proven by mutation, not asserted: dropping `ofxr-bridge` from the
      capability-backed list fails with all five games that would have rendered a dead card.
      And it caught a case while being written — `kharvox-vr` and `cheeky-foveated-dlss-uevr`
      are `planned`, so their cards are disabled rather than dead, which is *why* the
      assertion is on `available` modules and not on every id.

      Four behaviour bugs were found and left alone, which is the right call for a refactor
      whose value is being reviewable: the hero progress bar counts modules the grid never
      verifies so it cannot reach its total; UEVR never self-verifies; the update path writes
      an untranslated detail string; and `updateModule` sets a `configured` flag nothing reads.

- [ ] **Extract `feat/agent-mcp`, then delete the merged branches.** — `O-04`, corrected
      The roadmap said the branch was "partly cherry-picked". It is not. `feat/agent-mcp`
      holds **~1,500 lines that never reached main**: Collections (tab, install dialog,
      `useCollectionInstall`, `types/collection.ts`), Contribute (dialog and trigger) and a
      `TransactionsView`. The automated release code on it *is* superseded by `P0-8`; the
      product line is not, and it renders nowhere. The decision taken is to port the product
      line, not to discard it. Porting needs 60 locale keys across three languages, and the
      components must be re-mounted — copying the files alone would leave dead components,
      which is the same mistake as the ReShade card.

      The other two branches are safe: `feat/optiscaler-safe-install` (annotated `[ahead 82]`)
      and `feat/capability-deps-and-build-compat` (`[ahead 2]`) have **zero** commits not in
      `main`. Those annotations are stale upstreams, not unpublished work, exactly as the
      roadmap guessed. `backup/pre-unify-20260912` and `feat/bootstrap-mvp` are both at a
      commit fully merged into main.

## Bugs found by the new test suite

These were not on the roadmap. All are fixed, each with a test that fails without the fix.

- [x] **`getMissingDependenciesForModule` returned the module itself.** `configureModule`
      treats a non-empty result as "open the prerequisite dialog", so clicking Configure on
      any uninstalled module showed a dialog listing the very module being configured and
      never reached the install flow. Its own doc said the opposite.
- [x] **Escape closed a ref no template read.** A keyboard user could not dismiss a
      destructive-action prompt at all — capability removal, the Local AI disconnect, history
      undo. Those prompts are the ones this pass added.
- [x] **`actionCancel` existed in no locale file.** `ConfirmDialog`'s own default label and
      the capability removal call site both rendered the literal text `actionCancel`.
- [x] **`ModuleCard`'s Check button stayed live during an install.** It disabled on
      `verifyBusy` and `blockedReason` but not on the busy flag that disables every other
      control beside it; the click was then swallowed silently.
- [x] **The topbar Contribute button dispatched a window event nothing in the repo listens
      to.** A dead button, before this pass touched it.

---

# Later

- [-] **App self-updater** — `F-04`, `P0-4` · wired, waiting on one key
      The code is done and it **refuses to install anything** while the signing key is a
      placeholder. Verified by running it: `cargo check --release --lib` fails with E0080 and a
      message naming the command and the document. The guard is gated on
      `not(debug_assertions)` on purpose — an always-on version would leave the two blocking
      gates permanently red, and a permanently red guard gets deleted.

      Three more refusals back it: both commands return `not-configured` before any network
      call, the install re-checks and refuses if the release moved from the version the user
      confirmed, and the error mapping is keyed on the plugin's error *variants* rather than
      their strings, so upstream rewording cannot reclassify a signature failure into
      something that looks recoverable.

      `release.yml` throws rather than skipping when the payload is incomplete. That step was
      written to skip gracefully first, on the reasoning that a release before the updater is
      configured should still produce an installer — the reasoning was wrong, and the skip
      branch would have published a release whose update check could never succeed.

      **Left for the maintainer, in `docs/UPDATER.md`:** the pubkey in `tauri.conf.json`, and
      the `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` repository
      secrets. A valid-but-wrong key is undetectable at build time, so the dry-run tag is the
      guard. There is deliberately **no** build check against reusing the community keyring —
      duplicating those private consts would create exactly the drifting list this repo keeps
      getting bitten by.
- [-] **A second community capability, signed** — `F-11`
      `community-specialk-tweak-profile` is authored, validated by the community validator and
      exercises a transactional path (`write-text-file` + `move-file`) no first-party recipe
      touches. It is **not** signed, and cannot be: signing is the maintainer's key. The recipe
      is therefore refused at load time inside a collection, and installable only from the
      Community panel with the signature gate in front of it. That is the intended shape for an
      unsigned recipe, not a defect — but the trust chain is still not exercised in production
      until a signed entry exists. *Blocked on the same key as the catalogue.*
- [-] **UE4SS and REFramework specs** — `F-10`
      Split, and the split is the finding. **UE4SS is `available`**: its old blocker was the
      "engine-generation matrix", which was wrong — v3.0.1 ships one runtime archive, verified by
      downloading and listing the real release rather than trusting the docs. **REFramework is
      still `planned`, now for a specific reason**: its layout is verified, but v1.5.9.1's
      release note requires non-VR users to extract *only* `dinput8.dll`, and `extract-zip`
      writes every member with no include/exclude param and no conditional step. Staging plus
      `move-file` installs the flat-screen case correctly and silently gives a VR user no OpenXR
      support and no autorun scripts. *Done when:* `extract-zip` can filter its members —
      see the runner gap below.

      **The engine can now filter, and REFramework still does not ship.** The blocker moved
      from "the runner cannot express this" to authoring, and the remaining work is specific
      enough to name: a VR/no-VR config field the *user* answers, because the choice is
      per-user and not per-game, so a `boolean` is the honest shape. Then the `include`
      patterns have to be verified against a real v1.5.9.1 archive rather than against this
      roadmap's summary of it — the flat case must be provably `dinput8.dll` alone, and the
      VR case provably everything including the `reframework/autorun/**` tree, which the
      release note warns can crash a non-VR game.

      *The shape the engine offers is a flat list, and that is the interesting part: the
      recipe cannot branch on the answer itself.* It takes one step per case with a
      conditional, or the collection picks the variant. Choosing between those is a product
      call, and it is why this is still `planned` rather than a half-built recipe.
- [x] **A member filter on `extract-zip`.** — new, found while closing `F-10`
      `include` and `exclude`, arrays of glob patterns over archive members. `include`,
      when non-empty, selects; `exclude` then removes. `*` stays inside one segment, `**`
      crosses any number, a pattern with no `/` matches the basename at any depth, and
      matching is ASCII case-insensitive because the filesystem is. `?` and `[` are
      **literals**: a Windows member name cannot contain them, so reading them as
      wildcards could only ever silently drop a member the recipe asked for.

      *The filter runs before `payload` and the proxy rename, not after.* The filtering is
      the point — a recipe that filters to one DLL still needs `payload` to name it, and the
      two compose in that order. Both the zip and the 7z branch consume the same selection,
      one by archive ordinal and one by member name, because a 7z reader hands its entries
      over in block order. A filter that selects nothing is **refused**, naming both pattern
      lists and the archive's member count, and nothing is written — a filter that silently
      installed nothing is the exact failure this item exists to prevent.

      Seven tests, including one that asserts `plan_step_targets` and the run agree
      member-for-member. **Preflight and the run sharing a function is the part that matters:**
      they are two code paths, and a transaction that backs up different files than the step
      writes is a rollback that restores nothing.

- [x] **Environment-variable expansion in `render_template` / `resolve_path`.** — new
      `%NAME%` for `[A-Za-z_][A-Za-z0-9_]*`, no `${VAR}` — this is a Windows app and
      `%LOCALAPPDATA%` is the case that has to work. **A variable the process does not have
      is left literal**, because a template is also prose and a file's content has `%` for
      reasons that have nothing to do with the environment. `%%`, `%1`, `%PATH:1%` and a
      trailing `%` all pass through untouched.

      *No new exemption in the install-root rule.* An expanded path lands in
      `resolve_write_target` and gets exactly the treatment a literal one gets: absolute is
      honoured, which is the pre-existing `path`-typed-field exception, and a `..` walk out
      of the root is still refused — pinned by a test whose variable *is* `..`. The obvious
      wrong fix here was to let expansion bypass the scope check because "the user put it in
      a config field", and that would have turned the missing capability into a hole.

      `write-text-file` gains `format: json`: each `{param}` is inserted as escaped JSON
      string **content** and the recipe keeps its own quotes and punctuation, and the
      rendered result is parsed with `serde_json` **before the parent directory is created**.
      That last part is the fix, not a nicety — the failure this item exists to end is a
      Windows path rendered unescaped into a preference document that `read_game_preference`
      then silently answers `None` for. A refusal beats a silent no-op. An unknown `format`
      is refused rather than falling back to text.

      **The four stuck `ofxr-bridge` fields are unblocked, and `openxr-helpers`' two named
      gaps are closed in the runner.** Neither recipe is flipped — see the two items above;
      the remaining obstacle in both is authoring and a safety call, not a missing step.

      *A behaviour change beyond the item, flagged because the roadmap's own rule asks for
      it:* `render_template` no longer consumes a `{…}` run it cannot resolve. The old
      scanner took everything up to the next `}` and put it back, which for a JSON template
      means the object's opening brace swallows the first placeholder — `{"runtimePath":
      "{preferredRuntime}"}` rendered the whole `{"runtimePath": "{preferredRuntime` as one
      unresolvable name. `format: json` is not implementable without the fix. The output is
      byte-identical for a template that is nothing but placeholders; only the resume point
      differs. The same reasoning forced a second fix in the recipe-wide field scanner, which
      would otherwise have reported `{"runtimePath": "{preferredRuntime` as an undeclared
      field and failed every recipe the JSON format exists to enable.

- [x] **The default proxy rename collides.** — new, found while fixing `extract-zip`
      With no `payload` declared, every archive member basenamed `reshade64.dll` or `dxgi.dll`
      is renamed onto the same single proxy path, and the **last one wins silently**. Now
      refused: `archive_member_targets` accumulates a destination → claimants map and returns
      an error naming the destination, the members claiming it, and the fix (`payload`, or
      `include`/`exclude` for the rest of the archive).

      *Determinism is the whole point, so it is tested as a property rather than an
      outcome:* two archives with the same members in opposite order produce a byte-equal
      message. The map is a `BTreeMap` keyed on the lowercased destination — because Windows
      compares paths case-insensitively, so `DXGI.dll` and `dxgi.dll` are one destination —
      with a `BTreeSet` of claimants inside it, and the lowest-spelled spelling kept for the
      message. It also catches a rename landing on a member already named like the proxy,
      which is the same defect arriving by a different route. A declared `payload` stays
      immune, as it was.

- [ ] **`chain_artifacts` does not know about the filter.** — new, found while closing the
      member filter
      The invariant that says "a check may only inspect what its recipe writes" reads
      `extract-zip` params in `capability_runner.rs` and sets `extracts = true` for any
      `extract-zip` step, recording only `target` and `targetSubdir`. So a recipe that
      filters, plus a `file-exists` check on a member the filter removed, still passes the
      guard and then fails its own check at install time. Nothing ships this way today — no
      recipe uses a filter yet — so it is a latent hole, not a live one.

      Left open deliberately: fixing it means deciding what a *filtered* archive's artifact
      set is, and that is an authoring-policy question, not a mechanical one. It is the
      roadmap's own recurring shape — a guard that is right about the class and does not
      know the inventory — arriving one level up from where the previous ones were found.
- [ ] **Per-game flat profiles.** No `flat` key exists anywhere in the catalogue yet. · M
- [ ] **Nexus integration** where permitted. Still genuinely open — no code, no URL.
- [x] **Collections need content.** Two shipped collections with real members, and a test that
      reads the published files rather than a fixture — `every_built_in_collection_fits_the_game_it_targets`.
      `feat/agent-mcp` had shipped the loader with zero collection YAML. The branch's 1,127-line
      session runner stays unported on purpose: the frontend drives the existing
      `capability_install`, so porting it would reintroduce the second install path this pass
      removed.
- [x] **Let the Community panel install a recipe with required config fields.** — found
      while closing `UX-06`. The panel renders the entry's own `configSchema` as a form and
      installs with resolved values. Resolving went further than the item asked: the catalogue
      `config:` block is now the source for both the panel and collections, through one shared
      precedence rule in `src/features/capability-modules/config.ts`, and a required field with
      no value is refused **before** any install call instead of failing inside the first step
      the run had already begun.
- [-] **Translate `safetyNotes`, `configSchema[].description` and `displayName`.** — new
      The localization boundary covers the four backend-authored identifiers and the card
      description. These three were not. **The decision, and the part worth arguing about:**
      `displayName` is in scope, and the other two are not.

      `displayName` is a short label behind a stable id, and the app already rewrites
      `description` the same way, so it gets a `CAPABILITY_NAMES` table beside
      `capabilityDescriptionKey` — same shape, same exhaustiveness guard, three locales written
      by hand.

      `safetyNotes` and `configSchema[].description` stay in the recipe's own language, for the
      reason this codebase already wrote down about community recipes: *"that text is the
      author's, not the backend's, and the app does not get to rewrite what a person wrote."*
      That was written to protect an outside author from being paraphrased. It applies with more
      force to first-party prose that is mostly **warnings** — a note saying a registry write
      needs UAC, or that a transaction rollback is partial, is text the reader is relying on.
      A machine rendering of a safety warning is worse than an English one, because the reader
      has no way to check it. Translating them is not a localization gap; it is a safety
      regression wearing one.

      If that call is wrong, the honest way to reverse it is per-field opt-in with a human
      reviewer on the copy, not a blanket pass. `check.detail` stays as-is either way: it is
      data with an embedded version, not prose.
- [x] **A real Save-As in the profile import/export.** — new
      `tauri-plugin-dialog`, pinned to the 2.7 line because 2.8 requires
      `tauri = "^2.12"` and the graph is deliberately on 2.11.5. Export
      opens a native save dialog and reports the path the file really
      landed at, replacing a writer that picked
      `%LOCALAPPDATA%\Moddin\profiles\exports\` on the user's behalf. Import
      gains a Browse button that opens a native open dialog and drops the
      answer into the path box. A dismissed dialog is not a failure: it
      resolves to "nothing happened" and leaves the box and the preview
      untouched. The webview gets no dialog permission, so a profile can
      only be written to a path the user picked.
      *The correction this item carried was the useful part:* the previous
      wording claimed the storage layer "already exists" and implied only
      a dialog was missing. The layer existed and had zero callers.
- [x] **Resolve the eight baselined catalogue defects.** — new
      `scripts/validate-catalog.mjs` found them; they were reported by name rather than
      suppressed, and all eight are now fixed. The validator reports **zero** open data defects
      and `KNOWN_DEFECTS` is an empty list — with the record of the eight kept in the file above
      the array, because an empty baseline otherwise reads as "nothing was ever wrong".

      They were one mistake in five costumes: the catalogue listing a module id nothing
      implements. All eight were `status: planned`, so each rendered as a disabled "Planned"
      card — nothing crashed, which is exactly why they survived. The `cyberpunk-2077` `uevr`
      pair was the only real product call: the comment there had framed it as "teach the recipe
      about REDengine, or drop the module" and left it open, and it is the second one, because
      UEVR injects Unreal Engine and Cyberpunk 2077 is REDengine. Each removal leaves a comment
      saying what would bring it back. *A baseline is a sixth place a defect can be named. Read
      every line in it as a bug.*
- [ ] **OpenXR per-game runtime binding** — new, and deliberately not faked
      The `openxr-helpers` recipe was `available` and wrote
      `HKCU\...\OpenXR\1\per-game\{gameExecutable}\ActiveRuntime`, described as binding a runtime
      for one game. The OpenXR loader specification puts `ActiveRuntime` under a single
      machine-level key and states that runtime selection is handled external to the loader;
      **there is no per-game registry key in it.** The recipe was writing a key no loader reads,
      so it is `planned` with an empty chain and the evidence cited in its safety notes. The
      two mechanisms that really do scope a runtime to one game — `XR_RUNTIME_JSON` per process,
      and Moddin's own `%LOCALAPPDATA%\Moddin\profiles\openxr\<gameId>.json` — are both outside
      what a step can express: no step expands environment variables, and `write-text-file` has
      no JSON escaping, so a Windows path rendered into the preference document is invalid JSON
      and `read_game_preference` silently returns `None`. *Done when:* a step can expand an
      environment variable and write JSON with escaping. **This is the second time a recipe
      claimed an effect with no mechanism behind it, which is why it is an item and not a
      footnote.**

      **Both named gaps are now closed in the runner, and the recipe still cannot ship.** Its
      *Done when* line was written as though the capability were the whole obstacle, and it
      was not: a step can now write a preference document at `%LOCALAPPDATA%` with a path
      escaped into valid JSON, and the document still cannot be written. What is missing is
      `gameId` — the recipe has to know which game's document it is writing — and
      `run_uninstall` resolves no config, so the uninstall half cannot be expressed in a step
      at all. Underneath both sits the question the roadmap already refused to guess at:
      **what Undo does for a file outside the install root**, which the transaction store
      will not back up. The empty chain is still the honest shape of a `planned` recipe, and
      that is a better answer than a half-written one.

      *The generalisation is the reason this was worth an item rather than a note:* a recipe
      that writes outside the install root is the whole difficulty, and until now the engine
      could not even address such a path. It still cannot offer one for undo.

---

# Shipped

The accurate baseline, so the next person does not re-derive it. A full table lives in
`docs/SCOPE.md:83-103`.

**Solid:** Steam + Epic + GOG discovery (GOG only became reachable this pass), game scan and
catalogue matching, the capability install pipeline with transactional undo, the community
catalogue fetch with Ed25519 verification and revocation travelling *inside* the signed
catalogue, the AI assistant and MCP agent shell, profile import/export, collections and
contribute, snapshot create/list/rollback/delete, engine-gated capability lists, a frontend
architecture guard, a parity guard comparing four vocabularies across five sources including
param names, a catalogue validator, a frontend test runner, a real CSP, a guard on the Cargo
feature a bundled build silently depends on, and compile-enforced
locale parity across pt-BR / en / es.

**Known not solid:** the trust chain is not exercised in production at all right now, because
the catalogue has not been re-signed and the app is correctly refusing the community fetch;
the bundled app's frontend embedding was broken until this pass and its guard reads the
manifest rather than the artifact; catalogue breadth is still six games, one of which
(`doom-2016`) is a single-module near-duplicate of the `desktop-shortcut` every other game
carries; and the four UX-20 items the audit raised that this pass closed as *written* left
real gaps behind them, recorded at each item rather than here.

*This paragraph was four separate lies a moment ago, and every one of them was true when it
was written.* It said `fmt` and `clippy` were reported-only — they have blocked since the
quality-gate pass. It said the collection loader had no content — it ships two collections
with a test that reads the published files. It said the catalogue carried eight known data
defects — `KNOWN_DEFECTS` is an empty list with the record of the eight kept above it. It
said there was no file dialog — `tauri-plugin-dialog` landed with a real Save-As this pass.
**A summary of what is not solid is the first thing to go stale and the last thing anyone
re-reads**, because nothing makes it false: a defect that gets fixed leaves the sentence
behind, still true-looking, describing a repository that no longer exists. The items it
contradicted are all `[x]` above, and the honest rule is the one already written as standing
constraint 5 — the summary has to be rewritten in the change that moves the code, and the
change that moves the code is the only moment anyone is looking at it.

---

# Standing constraints

Recurring sources of drift, recorded so they stay visible:

1. **A list of valid step kinds lived in five places across three repositories, and nothing
   kept them in sync.** It drifted twice. The first guard covered four of the five; the
   unguarded one had already gone stale on its own. *A guard is only as good as its inventory
   of sources* — the fix for the next one is to find the fifth list before writing the
   check, not after it fires.
2. **The signing key is a Rust constant**, so rotation requires shipping a binary — and the
   one time it was needed, users downloaded a knowingly broken installer.
3. **`App.vue` has ~5 KB of headroom** against its own CI budget.
4. **The bundle step gates `cargo test`**, so every Rust check depends on nodejs.org and
   registry.npmjs.org being reachable. Now cached, but still a hard dependency.
5. **Docs drift silently.** Six documents made claims the code contradicted; they are
   corrected, and `docs/README.md` indexes what remains. The standing fix is still to update
   docs in the same change that moves the code.
6. **Deleting a command is not the same as resolving one.** An unreachable command is a
   question: wire it, or say why it is not wanted. Deleting it because wiring it was outside
   the current file ownership trades a visible problem for an invisible one.


### Fechamento beta.6 — IA e MCP (2026-10-02)

- Consentimento obrigatório por execução antes de enviar contexto ao provedor.
- CLIs sem bypass; autoria restrita, cancelamento da árvore do processo e timeout.
- Leitura concorrente e limitada das saídas; resposta final lida antes da limpeza.
- Recursos MCP mapeados explicitamente no instalador e catálogo embarcado,
  testado no smoke com o mesmo snapshot que será instalado.
- Pipeline comunitário emite `yamlSha256` e recusa a chave aposentada. Em
  2026-10-02 o run 36962458600 confirmou segredo e247ca4981f22245, enquanto
  public-keys.json exigia d489a3a0be894b19. Com autorização do mantenedor,
  foi gerada a nova chave bbe4bcb7ebd11a6f, armazenada fora do Git e registrada
  no segredo do GitHub e no bootstrap do beta.6. A chave e247 foi removida
  da confiança deste build; d489 permanece apenas durante a transição.
