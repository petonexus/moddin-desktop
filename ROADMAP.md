# Roadmap

Rewritten on 2026-09-29 from a full audit of the UI/UX, backend, catalog, docs and delivery pipeline.
Findings and evidence live in [docs/AUDIT-2026-09-29.md](docs/AUDIT-2026-09-29.md); every item below
cites the audit ID it comes from.

The previous version of this file was a changelog with checkboxes. It marked four things done that
are not done, marked two things open that are shipped, and omitted an entire shipped product line.
It is replaced rather than amended.

## How to read this

- **Now** — blocks the next beta. Do these before anything else.
- **Next** — the current cycle. Ordered, with acceptance criteria.
- **Later** — real, but not yet earned.
- Every item has a **Done when** line. An item is not done until that line is true and reviewable.
- Effort is rough: **XS** < 1 h · **S** half a day · **M** 1–2 days.

---

# Now — release blockers

The theme is not polish. Three of the P0s are cases where the app's own promises are false:
"every change is reversible" is not true on the capability path, "adding a mod is one YAML file" is
not true, and "we sign what we ship" has a hole in the key pinning.

### Capability engine integrity

- [x] **Add the `download-file` step to the built-in specs** and drop the misused `move-file`
      steps. — `P0-1` · done on `fix/capability-engine-integrity`
      *Done for* `optiscaler`, `reshade`, `cheeky-foveated-dlss` (the proxy rename now uses
      `extract-zip`'s native `proxyField` instead of a `move-file` that pointed a URL at a
      rename). `ofxr-bridge` and `uevr` were **not** fixed — see the open item below.
      *Guard:* `every_available_built_in_recipe_reads_its_payload_from_the_download_cache`.

- [ ] **Add a target-subdirectory option to `extract-zip` and a directory move to `move-file`,
      then wire `ofxr-bridge`.** — `P0-1` follow-up · M
      *Done when:* the spec extracts to its staging directory and promotes one named manifest file
      into the OpenXR `ApiLayers` folder. Until then the spec stays `status: planned` and the game
      page keeps offering the working OFXR module. Its `safetyNotes` names the missing capability.

- [ ] **Add a `git-checkout` step kind and a per-backend build step, then wire `uevr`.**
      — `P0-1` follow-up · M
      `pinnedTag` is a git tag, not an archive URL, so no download or extract step can consume it.
      Until then the spec stays `status: planned`.

- [x] **Open the transaction before the step loop** in `capability_runner.rs::execute_install`, and
      precompute the target set from the spec instead of post-hoc `affected_paths`.
      — `P0-2` · done
      *Done when:* overwriting an existing game file and then undoing restores the original content;
      a step that fails mid-install leaves no orphan writes and produces a rollback record.
      *Both covered by new tests:* `an_install_backs_up_an_existing_game_file_before_overwriting_it`
      and `a_failed_step_rolls_back_what_the_install_already_wrote`.

- [x] **Register `bepinex`, `ue4ss` and `reframework`** in `CapabilityRegistry::load()`.
      — `P0-3` · done
      *Also:* the six-spec list became one `BUILT_IN_YAML` const shared by `load()` and
      `load_with_local_dir()`, which had drifted apart.

- [x] **Constrain step targets to the install root.** — `F-02` · done, narrower than first proposed
      `..` traversal out of the install directory is refused outright, with a test
      (`a_step_cannot_write_outside_the_install_root_without_saying_so`).
      **Scope correction:** absolute paths stay allowed. `path`-typed config fields exist precisely
      to name a location outside the game folder, and the transaction store already refuses to back
      up anything outside the install root. `SKILL.md`'s HKLM claim is still too strong.
      *Also fixed:* `spawn-process` no longer puts `pid:NNN` in `affected_paths`, and `move-file`
      rejects a source equal to its destination.

- [ ] **Make `SKILL.md` tell the truth about path containment.** It claims Moddin "rejects HKLM";
      what is now enforced is that a step cannot traverse out of the install directory, and that
      the transaction store will not back up anything outside it. Absolute paths from `path`-typed
      config fields are still honoured. — `F-02` remainder · XS

- [x] **Add a registry test that parses every `include_str!` spec** and asserts each install chain's
      first step resolves to a local archive source. — `P0-1` guard · done
      `every_available_built_in_recipe_reads_its_payload_from_the_download_cache` also asserts that
      any spec extracting an archive downloads one first. The parse panic now names the offending
      recipe, because a YAML scalar containing `": "` silently becomes a mapping — the single most
      likely authoring mistake, and it bit twice during this fix pass.

### Trust and safety

- [x] **Pin the community key properly.** — `F-03` (key pinning) · done
      The on-disk `pinned-public-key.bin` used to be returned unverified: any 32
      bytes there became the key the catalog was checked against. It is now
      validated against a compiled **keyring** holding the active key plus the one
      retired in the 2026-09-24 rotation. A pin that does not match is discarded,
      logged, and replaced by the anchor. `REVOKED_KEY_FINGERPRINTS` is wired for a
      key that turns out to be compromised. Four tests, one of which asserts its
      "attacker" key really decodes so it cannot pass vacuously.

- [ ] **Honour `revoked-ids.json` in the app.** — `F-03` remainder · S
      Key-level revocation is wired. *Capability*-level revocation is not: the
      community repo maintains the file and the app never reads it.

- [ ] **Re-verify the cached catalog before using its `downloadUrl`, and allow-list
      the YAML fetch host.** — `F-03` remainder · M
      `community_candidate_install` still resolves the download URL from the
      **cached, unsigned** catalog. Bounded now by the keyring fix, but not fixed.

- [x] **Route the AI install path through the same unsigned-consent gate** as the
      Community panel. — `UX-12` · done
      `acceptUnsigned: true` was hard-coded, so the consent the Community panel
      asks for was simply skipped. The path now splits the batch by the verified
      catalog's `signed` flag and **fails closed** on the unsigned ones, naming
      them. Consent is persisted per capability id and written by the same
      checkbox the Community panel already showed.
      *Still open:* sharing one spec-driven install dialog with the Community
      panel and the game page — that is `UX-06`.

- [x] **Give the four unconfirmed destructive actions a confirmation** — capability
      removal, the Windows-wide OpenXR switch, Local AI disconnect, and history
      undo. — `UX-11` · done
      New `ConfirmDialog`, built on `BaseDialog` so the focus trap, Escape,
      focus-restore and `aria-modal` are inherited rather than re-implemented. Each
      dialog names what is about to change and what it costs to undo; the History
      one also counts the files, which the "Desfazer" button never did.
      *Also added:* `.btn-danger-solid` and `.confirm-details` / `.confirm-footnote`,
      none of which the design system had.
      *Not covered:* there is no frontend test runner (`F-12`), so nothing asserts
      these dialogs actually appear.

### Two broken user paths

- [x] **Fix the id-space bug in the AI install flow.** — `P0-5` · done
      Both entry points already passed a *catalog* id while `resolveInstallTarget` matched it
      against a *store* app id, so every install ended at "no game selected". Added
      `findInstalledGameForCatalogGame` (the inverse of the existing
      `findCatalogGameByInstalledGame`) and renamed the parameter to `catalogGameId`.
      *Still to do on this item:* the untranslated `NO_GAME_SELECTED` constant at the end of
      that branch, and the hard-coded `acceptUnsigned: true` (see `UX-12` above).

- [x] **Render a human description on capability cards** instead of `capability.id`.
      — `P0-6` · done, and the original prescription was wrong
      The `description` field did not exist — not on `CapabilitySpec`, not on `CapabilitySummary`,
      not in the TypeScript interfaces, not in any built-in YAML. It had to be *created* before the
      card could use it, which is why the id was the only option a community mod had. Added to both
      Rust structs, both TS interfaces and all nine YAMLs. The card falls back to the id only when a
      recipe declares no description.
      *Follow-up:* these lines are English in every locale, so a pt-BR or es user reads English
      descriptions. That is part of `UX-21`.

- [x] ~~**Add `open_external_url` and `open_web_url`**~~ — `P0-4` · **retracted, never needed**
      Both commands already exist (`lib.rs:84`, `:96`), are registered (`:168-169`), validate the
      scheme and are tested (`:237-246`). The audit's agent read the wrong file.
      *What remains real:* a set of registered-but-unused backend commands. → **F-14**.

- [ ] **Wire or delete the unused backend commands** — `get_compat_report` (529 lines), the four
      snapshot commands, `install_reshade` / `preview_reshade` / `uninstall_reshade`,
      `detect_gog_installed_games`, `get_cached_installed_games`, `refresh_installed_games_async`,
      `validate_recommendations_yaml`, `capability_reload`. — `F-14` · S
      *Done when:* every `#[tauri::command]` in the backend has a caller, or is gone.

### Delivery

- [x] **Make `release.yml` build the installer.** — `P0-8` · done
      A `windows-latest` job runs checkout → `npm ci` → `npm run build` →
      `bundle-runtime.ps1` → `npm run tauri build -- --bundles nsis`, then hashes
      the artifact it actually found (no glob left loose) and uploads it. The
      `release` job `needs: build`, downloads that artifact, and appends the
      CI-computed SHA-256 to the release body — so the digest and the file can no
      longer disagree, which is the whole point.
      *Note:* the first tag pushed after this lands produces the first CI-built,
      honestly-hashed installer. Worth a dry-run tag before announcing a release.

- [x] **Fix the `.msi` references** in `README.md`, `docs/USER-GUIDE.md` and
      `docs/CONTRIBUTING.md`. — `P0-7` · done
      Three said `.msi`; the product ships NSIS (`*_x64-setup.exe`).

- [x] **Verify the bundled runtime.** — `O-06` · done
      `bundle-runtime.ps1` now fetches Node's published `SHASUMS256.txt`, refuses
      to install a node build that is not listed there, and aborts on a digest
      mismatch instead of extracting it. In a product whose pitch is that it
      verifies what it installs, the shipped artifact was the one download that
      was not checked.
      *Also fixed:* the script's header claimed Node was not required, while step 3
      runs `npm install`. — `O-05`

---

# Next — current cycle

## Finish the pipeline the last two betas started

- [x] **Sync the recipe-kind lists across every repo that declares one.** — `F-05`, `F-06` · done
      `download-file` and `exe-version` shipped in Rust while the community validator rejected them,
      and the TypeScript union — what the AI/MCP authoring path types against — was missing **six of
      the eleven** step kinds. All four sources now agree.

- [x] **Add a guard so the lists cannot drift again.** — `F-05` · done
      `scripts/check-capability-kind-parity.mjs` (zero dependencies, like the architecture guard)
      reads `builtin_steps.rs`, `builtin_checks.rs`, `capability.ts`,
      `moddin-agent/schema/capability.schema.json` and the Python validator, and fails the build on
      any disagreement. Wired into `npm run check:capability-kinds` and into CI.
      *Done when:* the bug class cannot come back without a red build. **Verified** by deliberately
      removing `download-file` from the Python set — the guard failed, and passed again on restore.

- [ ] **Re-sign the community catalogue.** — `F-11` · XS, manual
      `catalog.json` changed and `regenerate_catalog.py` refuses to sign without `MODDIN_SIGNING_KEY`.
      Until it is re-signed the desktop app fails signature verification on the community fetch. That
      is correct fail-closed behaviour, and it is release-blocking by hand.

- [x] **Resolve `uninstallSkipsFiles`.** — `F-07` · done in block 1
      The field existed only in `cheeky-foveated-dlss.yaml`, which block 1 rewrote. Nothing in the
      repo uses it now, and the agent schema never declared it.

- [x] **Stop the preflight blocking on a full archive download.** — `F-08` · done
      `archive-sha256` is a blocker that ran while the UI rendered a card, and it pulled the whole
      archive. Added `RemoteArchive::content_length()` (HEAD) and a 64 MB preflight ceiling: above
      it, the check reports "verified during install" and skips. Not a weakening — the
      `download-file` step re-verifies the same digest with the archive already in flight.

- [x] **Replace the stub community capability.** — `F-11` · done
      It had four defects, not one: a `kill-process` naming `{{gameExecutableBaseName}}` (no engine
      has `{{...}}` syntax), an `extract-zip` pointed at a **URL**, a `move-file` onto itself, and an
      `uninstall` that deletes a **directory** with `file-delete`. The chain is now structurally
      correct and the capability is `status: planned` until verified against a real game.
      *Also added, for the class rather than the symptom:* validator rules that reject an
      unresolvable `{placeholder}` in any string param, an `extract-zip` reading a `type: url`
      field, and a `move-file` whose source equals its destination. All three failed on the file as
      it was.

- [x] **Extend `src/types/capability.ts` to every implemented step kind.** — `F-06` · done
      5 → 11, with a comment naming the parity guard so the next edit knows to run it.

- [x] **Wire or delete the unused backend commands** — `get_compat_report` (529 lines), the four
      snapshot commands, `install_reshade` / `preview_reshade` / `uninstall_reshade`,
      `detect_gog_installed_games`, `get_cached_installed_games`, `refresh_installed_games_async`,
      `validate_recommendations_yaml`, `capability_reload`. — `F-14` · S
      *Done when:* every `#[tauri::command]` in the backend has a caller, or is gone. **Still open.**

## UI/UX consistency

- [ ] **Collapse the four hand-rolled dialogs into `BaseDialog`**, and make `useDialogLifecycle` the
      single focus entry point so keyboard users land in the same place in all six.
      — `UX-07` · S
      *This is the largest single copy-paste mass in `src/`.*

- [ ] **Pass `verification` to `ModuleCard` from the capability section** and delete the duplicated
      checklist block. — `UX-05` · XS

- [ ] **Derive capability card state from verification and busy flags** so a failed check no longer
      looks like a healthy module. — `UX-04` · XS

- [ ] **One install dialog, three callers:** the spec-driven config form is used by the game page,
      the Community panel and the AI flow. — `UX-06` · M

- [ ] **One `useFriendlyError` + `<ErrorCallout>`**, replacing three copy-pasted classifiers and ~8
      raw `err.message` dumps. — `UX-32` · S

- [ ] **Group the sidebar tools** under two labelled sections and give "IA local" its own icon.
      — `UX-01` · XS

- [ ] **Collapse the five AI entry-point verbs into one**, naming modes only inside the dialog.
      — `UX-02` · S

- [ ] **Localization boundary:** map known backend ids — `entry.action`, `check.label`,
      `state.warnings`, `agent.detail` — to locale keys at the service layer. This also closes the
      debt already recorded at `FRONTEND.md:363`. — `UX-21` · M

- [ ] **Accessibility pass:** the `role="menu"` keyboard model, per-item `aria-label` on repeated
      actions, `.sr-only` labels on the activity-log controls, `aria-required`/`aria-invalid` on
      config fields, and `aria-busy` throughout. — `UX-25`…`UX-29` · M

- [ ] **Fix the token violations** in `AiTopbarMenu.vue` and `AiGameSuggestions.vue`, including the
      `--moddin-surface` fallback that does not exist and therefore renders its hard-coded hex.
      — `UX-30` · XS

- [ ] **Consolidate empty states** into one `.empty-state` contract, and give the capability section
      the loading state its composable already maintains. — `UX-09`, `UX-16` · S

- [ ] **`white-space: pre-line` on error paragraphs** so multi-line validation errors stay readable.
      — `UX-13` · XS

## Organization and delivery hygiene

- [ ] **Extract `feat/agent-mcp`, then delete the four dead branches.** The `[ahead 82]` and
      `[ahead 2]` annotations are stale upstreams, not unpublished work — but the only automated
      release code the project has written lives on that branch and is partly cherry-picked already.
      — `O-04` · S

- [ ] **Document which of each `src-tauri/src/*` vs `*_module.rs` pair is authoritative**, and why
      both exist. Nine undocumented parallel implementations is the worst structural ambiguity in
      the repo. — `O-09` · S

- [ ] **Decide `App.vue`'s fate** before the guard forces it: it is at 93.6 % of its 115 KB CI
      budget. Decompose it, or raise the budget deliberately and say why in `FRONTEND.md`.
      — `O-10` · M

- [ ] **Add the missing quality gates:** `cargo clippy`, `cargo fmt --check`, a pinned
      `rust-toolchain.toml`, and a `vitest` suite for `services/catalog.ts` and
      `useCapabilityModules.ts` first. — `F-12`, `O-07` · M

- [ ] **Correct the docs that actively mislead:** `MODULES.md:48-50` teaches the pattern beta.5
      fixed, and `tools/README.md:9-10` contradicts `MODULES.md:110-116` on whether reusable work
      becomes a Rust module or a YAML capability. Also document `bundle-runtime.ps1` as a prerequisite
      in `DEVELOPMENT.md`, and correct that script's false header. — `O-02`, `O-03`, `O-05` · S

- [ ] **Make the bundled runtime reproducible:** verify Node's SHA-256 in `bundle-runtime.ps1`, and
      switch `npm install` to `npm ci` with a committed lockfile for the staged runtime. The repo
      already states this rule for itself and breaks it for its own shipped artifact. — `O-06` · S

- [ ] **Cache the `Bundle moddin-runtime` CI step**, which downloads 28 MB and runs a full
      `npm install` on every push and gates `cargo test`. — `O-08` · XS

- [ ] **Add a docs index and delete the dead files** (`MERGE-NOTES.md`, `.gh-pr-body.md`,
      `scripts/fix-locale-mojibake.py`), and give the release notes a real home. — `O-12` · S

- [ ] **Write the conventions the history already follows** — branch naming, Conventional Commits,
      one roadmap story per commit. They are practiced but documented nowhere, so they cannot be
      taught. — `O-13` · XS

- [ ] **Stop tracking generated and wrong-platform weight:** root `catalog.json`, the committed
      `.zip` whose contents already live in `tools/`, the macOS/iOS icons, `src-tauri/gen/schemas/`.
      — `O-11` · XS

## Catalog honesty

- [x] **Fix `dead-island-2.yaml:5`** — `enginePreset: re-engine` → `unreal5`. — `F-10` · XS
      It declared RE Engine while shipping UEVR, OFXR and Cheeky modules that the `re-engine`
      preset's own description says it does not target. Dead Island 2 is Unreal 5.

- [x] **Restate the claims that are not true yet** in `SCOPE.md`. — `F-10`, `P0-2` · XS
      VR launch profiles (two games, not five); ReShade host (not reachable from the UI);
      "Auto-update | Beta" (there is no app self-updater at all — no `tauri-plugin-updater`, no
      `plugins.updater`); Transactional Undo (correct on both paths as of this branch);
      catalog breadth (six games, one of them a one-module stub).

- [x] **Regenerate `references/game-support-matrix.md`** from the six catalog YAMLs, and record
      what is missing. — `F-10` · XS
      It listed **four** games and omitted Elden Ring and Cyberpunk 2077 — the two flagship titles.
      It now carries the `vr-launch` distinction, notes that Elden Ring declares no engine preset
      at all, and keeps the "nothing validates a wrong `executable` or `enginePreset`" caveat that
      has bitten this file twice.

---

# Later

- [ ] **App self-updater** — `tauri-plugin-updater` plus a real keyring read, so signing-key rotation
      stops being a binary release. — `F-04`, `P0-4` · M
- [ ] **Import/export profiles.** The storage layer already exists; this is cheap and high value. · S
- [ ] **Engine-based capability filtering.** `supportedEngines[]` is parsed and then ignored.
      — `F-09` · S
- [ ] **Cross-platform validation.** A wrong `executable` path or `enginePreset` reaches production
      today; `dead-island-2` is the proof. Nothing in CI catches it. · S
- [ ] **UE4SS and REFramework** specs (currently `status: planned`, no install block). — `F-10` · M
- [ ] **Per-game flat profiles.** No `flat` key exists anywhere in the catalog yet. · M
- [ ] **Nexus integration** where permitted. Still genuinely open — no code, no URL.
- [ ] **A second community capability that is signed**, so the trust chain is exercised in production.
      — `F-11`
- [ ] **CSP.** `csp: null` in `tauri.conf.json:24` while shipping a bundled `node.exe` and an MCP
      agent. — `F-13` · S
- [ ] **Installer size.** 67.8 MB `node.exe` plus a 28.5 MB orphan `node.zip` in every build; a
      `[profile.release]` with `lto`/`strip` is also unset. — `F-13` · M

---

# Corrections to the previous roadmap

What changed, and why. Full evidence in the audit.

| Previous | Was | Reality | Action |
| --- | --- | --- | --- |
| `BepInEx` (v0.4) | `[x]` | Spec exists but is never registered; no `install_bepinex` command; its only engine preset (`unity`) is used by zero games. Renders nowhere. | **Reopened** → Now |
| `ReShade` (v0.3) | `[x]` | Rust commands exist but are never invoked from the UI; the catalog module is `planned`; the registered spec's install step is broken. | **Reopened** → Now (via `P0-1`) |
| `GitHub Releases updater` (Later) | `[x]` | That is a per-module version check. There is no app self-updater: no `tauri-plugin-updater`, no `plugins.updater`. | **Split** into two lines |
| `signed/versioned remote catalog` (Later) | `[ ]` | Shipped: full Ed25519 verify, key pinning, fingerprint, TTL cache, `SCOPE.md:96` calls it Stable. | **Closed**; hardening → `F-03` |
| `generic action engine primitives` (v0.2) | `[ ]` | Built: 11 step kinds. The real gap is that the built-in specs cannot use it. | **Re-scoped** → `P0-1` |
| `per-game VR / flat profiles` (v0.3) | `[ ]` | Half done: VR exists for 2 of 6 games; `flat` exists nowhere. | **Split** and restated |
| `compatibility metadata by game build` (v0.4) | `[ ]` | Engine implemented and tested; zero recipes use it. | **Re-scoped** → needs one recipe, not new code |
| `mod loaders / framework dependency graph` (v0.4) | `[x]` | Engine implemented and tested; zero recipes use it. | **Re-scoped** → needs one recipe |
| `UE4SS`, `REFramework` (v0.4) | `[ ]` | Correctly open. | **Kept** → Later |
| `Nexus integration` (Later) | `[ ]` | Correctly open. | **Kept** → Later |
| `import/export profiles` (Later) | `[ ]` | Correctly open, and cheap. | **Kept** → Next |
| *(absent)* | — | The AI assistant, the `moddin-agent` MCP server, the community catalog, PCGW and GOG discovery all shipped and appear nowhere. | **Now documented** in the audit |

---

# Shipped

The accurate baseline, so the next person does not re-derive it. A full table lives in
`docs/SCOPE.md:83-103`, which this list is aligned with.

**Solid:** Steam + Epic + GOG discovery, game scan and catalog matching, the legacy module installers
(OBS VR capture, OptiScaler, OFXR Bridge, UEVR, Cheeky Foveated, OpenXR per-game override),
transactional undo on the legacy path, activity log, the community catalog fetch with Ed25519
verification, the AI assistant and the MCP agent shell, the frontend architecture guard, and
compile-enforced locale parity across pt-BR / en / es.

**Known not solid:** everything on the capability install path (see Now), app self-update, and
catalog breadth beyond six games — one of which is a one-module stub.

---

# Standing constraints

Recurring sources of drift, recorded so they stay visible:

1. **The Rust enums and the community Python validator are kept in sync by hand.** They have already
   diverged, and the divergence breaks the documented minimum capability. A CI check comparing them is
   the only durable fix.
2. **The signing key is a Rust constant**, so rotation requires shipping a binary — and the one time
   it was needed, users downloaded a knowingly broken installer. Read the keyring instead.
3. **`App.vue` has ~7.7 KB of headroom** against its own CI budget. The next non-trivial change will
   trip the guard unless this is addressed deliberately.
4. **The bundle step gates `cargo test`**, so every Rust check depends on nodejs.org and
   registry.npmjs.org being reachable.
5. **Docs drift silently.** Six documents currently make claims the code contradicts. The audit
   lists them; the standing fix is to update docs in the same change that moves the code.
