# Roadmap

Last rewritten on 2026-09-29, after a second pass over the same audit. The first rewrite
is preserved in git history; the evidence for both lives in
[docs/AUDIT-2026-09-29.md](docs/AUDIT-2026-09-29.md), and every item below cites the audit
ID it comes from.

## How to read this

- **Now** — blocks the next beta.
- **Next** — the current cycle.
- **Later** — real, but not yet earned.
- Every item has a **Done when** line. An item is not done until that line is true and reviewable.
- Effort is rough: **XS** < 1 h · **S** half a day · **M** 1–2 days.

---

# Now — release blockers

All the P0s from the audit are closed. Two things remain, and one of them needs a maintainer's
key rather than a change.

### Capability engine integrity

- [x] **Add the `download-file` step to the built-in specs.** — `P0-1`
- [x] **Open the transaction before the step loop.** — `P0-2`
- [x] **Register `bepinex`, `ue4ss` and `reframework`.** — `P0-3`
- [x] **Constrain step targets to the install root.** — `F-02`
- [x] **Make `SKILL.md` tell the truth about path containment.** — `F-02` remainder
      It claimed Moddin "rejects HKLM". What is enforced is that a step cannot traverse out
      of the install root and that the transaction store will not back anything up outside
      it. Absolute paths from `path`-typed config fields are deliberately honoured, because
      those fields exist to name a location outside the game folder. Both the refusal and
      the exception are now stated.
- [x] **Add a target subdirectory to `extract-zip`, a directory move to `move-file`, a
      `git-checkout` step and a `build-project` step.** — `P0-1` follow-ups
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
- [ ] **Sign `revoked-ids.json`, or fold it into the signed catalogue.** — `F-03`, new
      The revocation list is fetched over a verified connection but is **not itself signed**.
      An attacker who can intercept that one GET can suppress a revocation until the TTL
      expires. The cache re-verification added in this pass does not help: the list is
      trusted precisely when it is stale. Moving the revocations into the already-signed
      `catalog.json` closes it without a second signature to manage.
      *Done when:* a revoked id cannot be suppressed by intercepting one unauthenticated
      request.

### Two broken user paths

- [x] **Fix the id-space bug in the AI install flow.** — `P0-5`
- [x] **Render a human description on capability cards.** — `P0-6`
- [x] **Wire or delete the unused backend commands** — `F-14`
      Every registered command now has a frontend caller or is gone. `capability_reload` is
      wired, so a locally dropped recipe shows up without a restart. `get_compat_report`, the
      four snapshot commands, `reshade`, `reshade_module`, `library_state` and seven orphaned
      snapshot helpers are gone. `validate_recommendations_yaml` is **kept** — the audit was
      wrong, it is called from the AI assistant.
      *Snapshot exception, recorded because it is a rule and not a fact:* the snapshot
      commands were deleted for having no caller, then restored. Deleting a user-facing
      feature because the wiring was outside the agent's file ownership is the same defect
      this pass exists to remove. If a command is unreachable, either wire it or say why it
      is not wanted — do not delete it as a side effect of a sweep.

### Delivery

- [x] **Make `release.yml` build the installer.** — `P0-8`
      *Follow-up, found when the release notes moved:* the job read its body from
      `.release-notes-<tag>.md` at the repo root, and the notes now live in
      `docs/release-notes/`. The next tag would have failed. Fixed.
- [x] **Verify the bundled runtime.** — `O-06`
- [ ] **Re-sign the community catalogue.** — `F-11`, XS, **by hand only**
      `catalog.json` changed and `regenerate_catalog.py` refuses to sign without
      `MODDIN_SIGNING_KEY`. Until it is re-signed the desktop app fails signature verification
      on the community fetch. That is correct fail-closed behaviour, and it is release-blocking
      by hand. The key is not in the environment and must not be pasted into a chat or a
      commit.
- [ ] **Commit a lockfile for the staged runtime.** — `O-06`, second half
      The Node archive is digest-verified, but `bundle-runtime.ps1` generates a fresh
      `package.json` with caret ranges and runs `npm install --omit=dev` with no lockfile.
      The repo states this rule for itself and breaks it for its own shipped artifact. It
      cannot be fixed without deciding where the generated manifest lives: it is currently
      under a gitignored path, so a lockfile for it cannot be committed.

---

# Next — current cycle

## Quality gates

- [x] **Add the missing quality gates.** — `F-12`, `O-07`
      A pinned `rust-toolchain.toml` (1.98.1, the version `stable-msvc` resolved to, so the
      build is unchanged and no longer floats), `cargo fmt --check`, `cargo clippy`, and
      `vitest` with 114 tests over the files where behaviour has broken before.
- [ ] **Turn `fmt` and `clippy` into blocking gates.** — `O-07`
      Both run in CI and report. Neither blocks, because neither is satisfiable today:
      `fmt --check` would rewrite **3,653 lines across 34 files**, and `clippy --all-targets`
      reports **144 diagnostics across 30 lints**, 53 of them `dead_code` from the module
      implementations that are meant to stay dark. A guard nobody can satisfy is a guard
      people disable. This is a tree-wide cleanup pass plus an `[lints]` block in
      `src-tauri/Cargo.toml`, and it is its own change with its own owner.
      *Done when:* CI is red on a formatting or lint regression.

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
- [ ] **Tier 3 (M).** — `UX-06`, `UX-21`, `UX-25`…`UX-29`
      Not attempted, on purpose. `UX-21`'s boundary is a four-identifier mapping across the
      service layer with a `CapabilityCardState` dependency, and a half-applied boundary is
      worse than none, because it leaves ids half-translated. `UX-06` collapses three install
      paths and is not an S. `UX-25…29` is a broad mechanical sweep that wants its own pass.
      *Done when:* a pt-BR user reads no backend-authored English in the capability section.

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
- [ ] **Decide `App.vue`'s fate.** — `O-10`
      It is at 109,687 bytes against the guard's 115,000 budget, so ~5 KB of headroom. The
      decision taken is to decompose rather than raise the budget. Not started.
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

- [ ] **App self-updater** — `F-04`, `P0-4` · M
- [ ] **Import/export profiles.** The storage layer already exists. · S
- [ ] **Engine-based capability filtering.** `supportedEngines[]` is parsed and then ignored. — `F-09` · S
- [ ] **Cross-platform validation.** A wrong `executable` path or `enginePreset` reaches
      production today; `dead-island-2` is the proof. Nothing in CI catches it. · S
- [ ] **UE4SS and REFramework specs** (currently `status: planned`, no install block). — `F-10` · M
- [ ] **Per-game flat profiles.** No `flat` key exists anywhere in the catalog yet. · M
- [ ] **Nexus integration** where permitted. Still genuinely open — no code, no URL.
- [ ] **A second community capability that is signed**, so the trust chain is exercised in
      production. — `F-11`
- [ ] **CSP.** `csp: null` in `tauri.conf.json` while shipping a bundled `node.exe` and an
      MCP agent. — `F-13` · S
- [ ] **Installer size.** 67.8 MB `node.exe` plus a 28.5 MB orphan `node.zip` in every
      build; a `[profile.release]` with `lto`/`strip` is also unset. — `F-13` · M

---

# Shipped

The accurate baseline, so the next person does not re-derive it. A full table lives in
`docs/SCOPE.md:83-103`.

**Solid:** Steam + Epic + GOG discovery (GOG only became reachable this pass), game scan and
catalogue matching, the capability install pipeline with transactional undo, the community
catalogue fetch with Ed25519 verification and capability-level revocation, the AI assistant
and MCP agent shell, the frontend architecture guard, a capability-kind parity guard over
five sources including param names, a frontend test runner, and compile-enforced locale
parity across pt-BR / en / es.

**Known not solid:** app self-update; catalogue breadth beyond six games, one of which is a
one-module stub; `fmt` and `clippy` as reported-only gates; the four `*_module.rs` adapters
and `updater.rs`, which are dead but still compiled.

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
